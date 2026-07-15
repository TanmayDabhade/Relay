const FEATURES = [
  {
    n: "01",
    tag: "drag = ship",
    title: "Kanban Deploy",
    body: "Plan the work, then run it. Drag a card → Relay opens a new terminal and launches your CLI agent with the task prompt already seeded. Backlog, in-flight, and done, all on your machine.",
  },
  {
    n: "02",
    tag: "zsh · fish · nu",
    title: "Native Terminals",
    body: "No custom runtime, no sandboxed shell. Relay spawns a real session in your own terminal, running the same `claude` / `codex` / `cursor-agent` command you'd type by hand.",
  },
  {
    n: "03",
    tag: "4 CLIs · 0 setup",
    title: "Every Agent, One Board",
    body: "Point Relay at ~/ and it discovers every .claude, .codex, .cursor, .gemini session on disk. No SDK, no instrumentation, no manual wiring — even for agents you never set up to be watched.",
  },
  {
    n: "04",
    tag: "unlimited history",
    title: "Session Replay",
    body: "Rewind any agent run. Diff prompts, tool calls, and file writes. Fork from any step to try a different path.",
  },
  {
    n: "05",
    tag: "$0.36 avg / session",
    title: "Spend Tracking",
    body: "Every token metered locally. Per-agent, per-repo, per-day. Set caps and get shell alerts before the bill spikes.",
  },
  {
    n: "06",
    tag: "sub-100ms",
    title: "Live Observability",
    body: "Watch prompts, tool calls, and file diffs stream in real time. Filter by agent, repo, or exit code.",
  },
];

export function Features() {
  return (
    <section id="features" className="border-t border-relay-border">
      <div className="mx-auto max-w-6xl px-6 py-20">
        <p className="text-sm text-relay-primary">{"// features"}</p>
        <h2 className="mt-3 font-display text-3xl font-black tracking-tight uppercase sm:text-4xl">
          A control room for your <span className="text-relay-primary">/agents.</span>
        </h2>
        <p className="mt-4 max-w-2xl text-relay-muted">
          Six primitives, one binary. Everything you need to plan, launch, and
          drive the agents you already run — observability is just the part
          that comes for free.
        </p>
        <div className="mt-12 grid gap-px overflow-hidden border border-relay-border bg-relay-border sm:grid-cols-2 lg:grid-cols-3">
          {FEATURES.map((f) => (
            <div key={f.n} className="bg-relay-bg p-6">
              <div className="flex items-center justify-between text-xs text-relay-muted">
                <span>{f.n}</span>
                <span className="text-relay-green">{f.tag}</span>
              </div>
              <h3 className="mt-4 font-display text-lg font-bold">{f.title}</h3>
              <p className="mt-2 text-sm leading-relaxed text-relay-muted">
                {f.body}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
