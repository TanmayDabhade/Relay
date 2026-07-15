const COLUMNS = [
  { title: "Product", links: ["Features", "Pricing", "Compare", "Security"] },
  { title: "CLIs", links: ["claude-code", "codex", "cursor-cli", "gemini-cli"] },
  { title: "Company", links: ["Manifesto", "Changelog", "Contact"] },
];

export function Footer() {
  return (
    <footer className="border-t border-relay-border">
      <div className="mx-auto max-w-6xl px-6 py-16">
        <p className="text-sm text-relay-primary">{"// built for the terminal"}</p>
        <p className="mt-3 font-display text-2xl font-black tracking-tight uppercase">
          Ship agents. Not dashboards.
        </p>
        <div className="mt-12 grid grid-cols-2 gap-8 sm:grid-cols-4">
          <div>
            <p className="font-display text-lg font-black tracking-tight">
              RELAY<span className="text-relay-primary">.</span>
            </p>
          </div>
          {COLUMNS.map((c) => (
            <div key={c.title}>
              <p className="text-xs tracking-widest text-relay-muted uppercase">
                {c.title}
              </p>
              <ul className="mt-4 space-y-2 text-sm">
                {c.links.map((l) => (
                  <li key={l}>
                    <a
                      href="#"
                      className="text-relay-text transition-colors hover:text-relay-primary"
                    >
                      {l}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <div className="mt-16 flex flex-col gap-2 border-t border-relay-border pt-6 text-xs text-relay-muted sm:flex-row sm:items-center sm:justify-between">
          <p>RELAY. © 2026 Relay</p>
          <p>v0.1 · dec 2026</p>
        </div>
      </div>
    </footer>
  );
}
