const TIERS = [
  {
    name: "Local",
    price: "$0",
    period: "/ forever",
    blurb: "Everything you need to run agents locally.",
    features: [
      "Unlimited agents on one machine",
      "All 4 CLI integrations",
      "Session history (30 days)",
      "Spend tracking",
      "Local Kanban",
    ],
    cta: "Download →",
    highlight: false,
  },
  {
    name: "Pro",
    price: "$12",
    period: "/ dev / month",
    blurb: "For solo devs shipping serious agent workloads.",
    features: [
      "Everything in Local, plus:",
      "Unlimited session history",
      "Session fork & diff",
      "Custom spend alerts",
      "Priority CLI support",
      "Early access to new integrations",
    ],
    cta: "Start Free Trial →",
    highlight: true,
  },
  {
    name: "Team",
    price: "$29",
    period: "/ seat / month",
    blurb: "Share dashboards across your engineering team.",
    features: [
      "Everything in Pro, plus:",
      "Local peer-to-peer team sync",
      "Shared Kanban across boxes",
      "Team spend rollups",
      "Role-based permissions",
      "SSO (self-hosted)",
    ],
    cta: "Contact Sales →",
    highlight: false,
  },
];

export function Pricing() {
  return (
    <section id="pricing" className="border-t border-relay-border bg-relay-surface">
      <div className="mx-auto max-w-6xl px-6 py-20">
        <p className="text-sm text-relay-primary">{"// pricing"}</p>
        <h2 className="mt-3 font-display text-3xl font-black tracking-tight uppercase sm:text-4xl">
          No cloud tax. Just software.
        </h2>
        <p className="mt-4 max-w-2xl text-relay-muted">
          Priced like a dev tool, not a SaaS. One flat fee, no per-token
          surprises, no seat traps.
        </p>
        <div className="mt-12 grid gap-6 lg:grid-cols-3">
          {TIERS.map((t) => (
            <div
              key={t.name}
              className={`relative flex flex-col border bg-relay-bg p-6 ${
                t.highlight ? "border-relay-primary" : "border-relay-border"
              }`}
            >
              {t.highlight && (
                <span className="absolute -top-3 left-6 bg-relay-primary px-2 py-0.5 text-xs font-bold text-relay-primary-contrast">
                  Most Popular
                </span>
              )}
              <h3 className="font-display text-lg font-bold">{t.name}</h3>
              <p className="mt-2">
                <span className="font-display text-3xl font-black">
                  {t.price}
                </span>{" "}
                <span className="text-sm text-relay-muted">{t.period}</span>
              </p>
              <p className="mt-3 text-sm text-relay-muted">{t.blurb}</p>
              <ul className="mt-6 flex-1 space-y-2 text-sm">
                {t.features.map((f) => (
                  <li key={f}>
                    {f.endsWith(":") ? (
                      <span className="text-relay-muted">{f}</span>
                    ) : (
                      <span className="text-relay-text">&gt; {f}</span>
                    )}
                  </li>
                ))}
              </ul>
              <a
                href="#waitlist"
                className={`mt-6 inline-flex items-center justify-center border px-4 py-2.5 text-sm font-bold transition-colors ${
                  t.highlight
                    ? "border-relay-primary bg-relay-primary text-relay-primary-contrast hover:opacity-90"
                    : "border-relay-border-strong text-relay-text hover:border-relay-primary hover:text-relay-primary"
                }`}
              >
                {t.cta}
              </a>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
