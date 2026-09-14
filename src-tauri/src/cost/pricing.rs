//! Static, bundled per-model USD pricing table (`resources/pricing.json`), compiled into the
//! binary via `include_str!` — never read from disk at runtime, so it can't be edited/spoofed
//! by an agent. Parsed once and cached; lookups are defensive by design (never panic on an
//! unrecognized model string, a missing pricing entry, or malformed JSON), per PLAN.md §5's
//! "cost accuracy... resilient to unknown model values".

use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

const PRICING_JSON: &str = include_str!("../../resources/pricing.json");

/// Sentinel key for the fallback rate applied to any model string that isn't an exact or
/// prefix match and isn't a `<...>` sentinel value.
const DEFAULT_KEY: &str = "_default";

/// Used only if `resources/pricing.json` somehow fails to parse (it's bundled and controlled
/// by us, so this should never happen in practice) — keeps lookups panic-free regardless.
const FALLBACK_DEFAULT_RATES: Rates = Rates {
    input: 3.0,
    output: 15.0,
    cache_write_5m: 3.75,
    cache_write_1h: 6.0,
    cache_read: 0.3,
};

const ZERO_RATES: Rates = Rates {
    input: 0.0,
    output: 0.0,
    cache_write_5m: 0.0,
    cache_write_1h: 0.0,
    cache_read: 0.0,
};

/// USD per million tokens. Cache writes are split by TTL because the API bills them
/// differently (5-minute: 1.25x input, 1-hour: 2x input) and Claude Code uses the 1-hour TTL —
/// pricing every write at the 5-minute rate under-reports real spend.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Rates {
    input: f64,
    output: f64,
    cache_write_5m: f64,
    cache_write_1h: f64,
    cache_read: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct ModelPricing {
    #[serde(flatten)]
    standard: Rates,
    /// Rates for `usage.speed == "fast"` responses, where the model publishes them.
    #[serde(default)]
    fast: Option<Rates>,
}

#[derive(Debug, Deserialize)]
struct PricingFile {
    #[allow(dead_code)] // parsed for completeness/future validation, not consulted at lookup time
    schema_version: u32,
    rates_per_million_tokens: HashMap<String, ModelPricing>,
}

fn pricing_table() -> &'static HashMap<String, ModelPricing> {
    static TABLE: OnceLock<HashMap<String, ModelPricing>> = OnceLock::new();
    TABLE.get_or_init(|| match serde_json::from_str::<PricingFile>(PRICING_JSON) {
        Ok(file) => file.rates_per_million_tokens,
        Err(e) => {
            log::error!("failed to parse bundled pricing.json, falling back to built-in default rates only: {e}");
            let mut fallback = HashMap::new();
            fallback.insert(
                DEFAULT_KEY.to_string(),
                ModelPricing { standard: FALLBACK_DEFAULT_RATES, fast: None },
            );
            fallback
        }
    })
}

fn default_pricing() -> ModelPricing {
    pricing_table()
        .get(DEFAULT_KEY)
        .copied()
        .unwrap_or(ModelPricing { standard: FALLBACK_DEFAULT_RATES, fast: None })
}

/// Longest-prefix match over every non-`_default` key in the pricing table — handles
/// versioned/dated suffixes on a known model family (e.g. `claude-haiku-4-5-20251001`
/// matches the `claude-haiku-4-5` entry).
fn longest_prefix_match<'a>(
    table: &'a HashMap<String, ModelPricing>,
    model: &str,
) -> Option<&'a ModelPricing> {
    table
        .iter()
        .filter(|(key, _)| key.as_str() != DEFAULT_KEY && model.starts_with(key.as_str()))
        .max_by_key(|(key, _)| key.len())
        .map(|(_, rates)| rates)
}

fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    let table = pricing_table();

    // 1. Exact match.
    if let Some(r) = table.get(model) {
        return Some(*r);
    }

    // 2. Longest-prefix match.
    if let Some(r) = longest_prefix_match(table, model) {
        return Some(*r);
    }

    // 3. Sentinel: values like `<synthetic>` are non-billable placeholders seen in real logs
    //    — treat as zero cost, do NOT fall through to `_default`.
    if model.starts_with('<') {
        return Some(ModelPricing { standard: ZERO_RATES, fast: None });
    }

    log_unknown_model_once(model);
    // 4. A Claude model newer than this table: `_default` is a reasonable estimate. Anything
    //    else (Codex's GPT models, Gemini) has no price here — pricing it at Claude rates
    //    produced wildly wrong spend, so it is reported as unpriced instead.
    model.starts_with("claude").then(default_pricing)
}

fn log_unknown_model_once(model: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let set = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = set.lock().unwrap();
    if set.insert(model.to_string()) {
        log::warn!(
            "encountered unrecognized model string for pricing: {model:?} (falling back to _default rates)"
        );
    }
}

/// Finds the one key in the bundled pricing table whose name contains `"haiku"` — the single
/// source of truth for "which model string does the AI-summarization pipeline (Task C2) send
/// to the Anthropic API," per PLAN.md §6 ("from pricing.json"). Doesn't parse any new JSON —
/// just a lookup over the table already loaded by `pricing_table()`. Returns `None` (rather
/// than panicking or guessing) if no such key exists; the caller treats that the same as "no
/// API key" and skips summarization for this app run.
pub fn haiku_model_id() -> Option<&'static str> {
    pricing_table()
        .keys()
        .find(|k| k.contains("haiku"))
        .map(|k| k.as_str())
}

/// Billable token counts for one API response (or an aggregate of several).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write_5m: i64,
    pub cache_write_1h: i64,
}

fn price(rates: &Rates, usage: &TokenUsage) -> f64 {
    (usage.input as f64 / 1e6) * rates.input
        + (usage.output as f64 / 1e6) * rates.output
        + (usage.cache_write_5m as f64 / 1e6) * rates.cache_write_5m
        + (usage.cache_write_1h as f64 / 1e6) * rates.cache_write_1h
        + (usage.cache_read as f64 / 1e6) * rates.cache_read
}

/// Exact cost of one API response: its own model, its own speed tier, and cache writes billed
/// by TTL. This is what Claude Code sessions are summed from (see `db::queries::session_usage`),
/// so a session that mixes models (e.g. Opus with a Haiku subagent) or toggles fast mode is
/// priced turn by turn rather than at one session-wide rate. A `"fast"` response on a model
/// without published fast rates is priced at standard rates. `None` means the model has no
/// known price. Never panics.
pub fn request_cost_usd(
    model: Option<&str>,
    speed: Option<&str>,
    usage: &TokenUsage,
) -> Option<f64> {
    if *usage == TokenUsage::default() {
        return Some(0.0);
    }
    let pricing = match model {
        None => default_pricing(),
        Some(m) => pricing_for_model(m)?,
    };
    let rates = match (speed, pricing.fast) {
        (Some("fast"), Some(fast)) => fast,
        _ => pricing.standard,
    };
    Some(price(&rates, usage))
}

/// Aggregate-total variant for agents whose logs don't expose per-response identity (Codex,
/// Gemini, Cursor) and for older Claude sessions whose raw logs no longer exist. Prices every
/// token at one model's standard rates.
pub fn cost_usd(model: Option<&str>, usage: &TokenUsage) -> Option<f64> {
    request_cost_usd(model, None, usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: i64, output: i64, cache_read: i64, w5: i64, w1h: i64) -> TokenUsage {
        TokenUsage { input, output, cache_read, cache_write_5m: w5, cache_write_1h: w1h }
    }

    fn close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("model should be priced");
        assert!((actual - expected).abs() < 1e-9, "expected {expected}, got {actual}");
    }

    #[test]
    fn opus_5_uses_published_rates_with_cache_writes_billed_by_ttl() {
        // $5 input / $25 output / $0.50 cache read / $6.25 5m write / $10 1h write per MTok.
        let cost = request_cost_usd(
            Some("claude-opus-5"),
            Some("standard"),
            &usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000),
        );
        close(cost, 5.0 + 25.0 + 0.5 + 6.25 + 10.0);
    }

    #[test]
    fn fast_mode_uses_fast_rates_only_where_published() {
        let u = usage(1_000_000, 1_000_000, 0, 0, 0);
        close(request_cost_usd(Some("claude-opus-5"), Some("fast"), &u), 10.0 + 50.0);
        // No published fast rates for Sonnet 5 — standard pricing, not a guess.
        close(request_cost_usd(Some("claude-sonnet-5"), Some("fast"), &u), 2.0 + 10.0);
    }

    #[test]
    fn current_model_rates_match_the_published_price_list() {
        let one_m_in_out = usage(1_000_000, 1_000_000, 0, 0, 0);
        for (model, expected) in [
            ("claude-fable-5-1", 60.0),
            ("claude-opus-4-8", 30.0),
            ("claude-opus-4-7", 30.0),
            ("claude-sonnet-5", 12.0),
            ("claude-sonnet-4-6", 18.0),
            ("claude-haiku-4-5", 6.0),
        ] {
            close(cost_usd(Some(model), &one_m_in_out), expected);
        }
        // Fable 5.1 cache reads are 0.025x input, not the usual 0.1x.
        close(cost_usd(Some("claude-fable-5-1"), &usage(0, 0, 1_000_000, 0, 0)), 0.25);
    }

    #[test]
    fn longest_prefix_match_resolves_a_dated_suffix_to_the_known_family_rate() {
        let u = usage(148, 700, 21800, 290, 0);
        assert_eq!(
            cost_usd(Some("claude-haiku-4-5-20251001"), &u),
            cost_usd(Some("claude-haiku-4-5"), &u)
        );
        assert!(cost_usd(Some("claude-haiku-4-5"), &u).is_some());
    }

    #[test]
    fn sentinel_model_is_non_billable_not_default() {
        // `<synthetic>` is a real sentinel value seen in Claude Code logs - must resolve to
        // exactly 0.0, not silently fall through to _default rates.
        let u = usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000);
        assert_eq!(cost_usd(Some("<synthetic>"), &u), Some(0.0));
    }

    #[test]
    fn unrecognized_claude_model_falls_back_to_default_rates() {
        // _default input rate is 3.0 per resources/pricing.json.
        close(cost_usd(Some("claude-future-9"), &usage(1_000_000, 0, 0, 0, 0)), 3.0);
    }

    #[test]
    fn non_anthropic_models_are_unpriced_not_billed_at_claude_rates() {
        let u = usage(1_000_000, 1_000_000, 0, 0, 0);
        assert_eq!(cost_usd(Some("gpt-5.6-sol"), &u), None);
        assert_eq!(cost_usd(Some("gemini-3-pro"), &u), None);
    }

    #[test]
    fn none_model_resolves_to_default_without_panicking() {
        close(cost_usd(None, &usage(1_000_000, 0, 0, 0, 0)), 3.0);
    }

    #[test]
    fn zero_tokens_yields_zero_cost_regardless_of_model() {
        assert_eq!(cost_usd(Some("claude-opus-5"), &TokenUsage::default()), Some(0.0));
        assert_eq!(cost_usd(Some("gpt-5.6-sol"), &TokenUsage::default()), Some(0.0));
        assert_eq!(cost_usd(None, &TokenUsage::default()), Some(0.0));
    }

    #[test]
    fn haiku_model_id_finds_the_bundled_haiku_entry() {
        // Pins that the summarization pipeline resolves its model string from pricing.json
        // rather than a second hardcoded literal.
        assert_eq!(haiku_model_id(), Some("claude-haiku-4-5"));
    }
}
