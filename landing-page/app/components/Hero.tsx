import { TerminalWindow } from "./TerminalWindow";
import { CliMarquee } from "./CliMarquee";

const STATS = [
  { label: "Agents", value: "4" },
  { label: "Sessions", value: "127" },
  { label: "Spend", value: "$18.42" },
];

const TAGS = ["Local-first", "No cloud tax", "Zero telemetry"];

export function Hero() {
  return (
    <section id="top" className="mx-auto max-w-6xl px-6 pt-16 pb-20 sm:pt-24">
      <div className="grid gap-12 lg:grid-cols-2 lg:items-center lg:gap-16">
        <div>
          <span className="inline-block border border-relay-border-strong px-3 py-1 text-xs tracking-widest text-relay-muted">
            v0.1 · PRIVATE BETA
          </span>
          <h1 className="mt-6 font-display text-4xl leading-[1.05] font-black tracking-tight uppercase sm:text-5xl lg:text-6xl">
            Run your agents <span className="text-relay-primary">locally.</span>
          </h1>
          <p className="mt-6 max-w-xl text-base leading-relaxed text-relay-muted">
            Relay is the native-first project management &amp; observability
            layer for AI coding agents on your machine. Auto-detects{" "}
            <code className="text-relay-text">.claude</code>,{" "}
            <code className="text-relay-text">.codex</code>,{" "}
            <code className="text-relay-text">.cursor</code>, and{" "}
            <code className="text-relay-text">.gemini</code> — track spend,
            replay sessions, and deploy tasks straight from a Kanban that
            spawns real terminals.
          </p>
          <div className="mt-8 flex flex-wrap items-center gap-6">
            <a
              href="#waitlist"
              className="border border-relay-primary bg-relay-primary px-6 py-3 text-sm font-bold text-relay-primary-contrast transition-opacity hover:opacity-90"
            >
              $ Join Waitlist
            </a>
            <ul className="flex flex-wrap gap-x-5 gap-y-2 text-xs text-relay-muted">
              {TAGS.map((t) => (
                <li key={t}>{`// ${t}`}</li>
              ))}
            </ul>
          </div>
        </div>
        <div className="space-y-4">
          <TerminalWindow />
          <div className="grid grid-cols-3 divide-x divide-relay-border border border-relay-border bg-relay-surface text-center">
            {STATS.map((s) => (
              <div key={s.label} className="px-4 py-3">
                <p className="font-display text-xl font-bold text-relay-primary">
                  {s.value}
                </p>
                <p className="text-xs tracking-widest text-relay-muted uppercase">
                  {s.label}
                </p>
              </div>
            ))}
          </div>
        </div>
      </div>
      <div className="mt-16">
        <CliMarquee />
      </div>
    </section>
  );
}
