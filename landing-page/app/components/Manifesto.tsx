const POINTS = [
  {
    n: "01",
    title: "On-disk only",
    body: "Sessions, spend, config — persisted to ~/.relay. Never leaves your box.",
  },
  {
    n: "02",
    title: "Zero telemetry",
    body: "No pings, no analytics, no phone-home. Sniff the packets, we dare you.",
  },
  {
    n: "03",
    title: "Your keys, your rules",
    body: "Relay reads existing CLI auth. We never see, store, or proxy your API keys.",
  },
];

export function Manifesto() {
  return (
    <section id="security" className="border-t border-relay-border bg-relay-surface">
      <div className="mx-auto max-w-6xl px-6 py-20">
        <p className="text-sm text-relay-primary">{"// local-first manifesto"}</p>
        <h2 className="mt-3 max-w-3xl font-display text-3xl font-black tracking-tight uppercase sm:text-4xl">
          Data stays on <span className="text-relay-primary">your machine.</span>
        </h2>
        <p className="mt-4 max-w-2xl text-relay-muted">
          Cloud dashboards ship your prompts, code, and API keys to a
          stranger&apos;s S3 bucket. Relay refuses. Every byte lives beside
          your source, encrypted at rest, invisible to us.
        </p>
        <div className="mt-12 grid gap-8 sm:grid-cols-3">
          {POINTS.map((p) => (
            <div key={p.n} className="border-t border-relay-border-strong pt-4">
              <span className="font-display text-sm text-relay-muted">
                {p.n}
              </span>
              <h3 className="mt-2 font-display text-base font-bold">
                {p.title}
              </h3>
              <p className="mt-2 text-sm leading-relaxed text-relay-muted">
                {p.body}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
