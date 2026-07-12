const ROWS: { label: string; relay: boolean; cloud: boolean; ide: boolean }[] = [
  { label: "Runs entirely on your machine", relay: true, cloud: false, ide: true },
  {
    label: "Multi-CLI support (claude/codex/cursor/gemini)",
    relay: true,
    cloud: false,
    ide: false,
  },
  { label: "Session replay & fork", relay: true, cloud: true, ide: false },
  { label: "Kanban → real terminal spawn", relay: true, cloud: false, ide: false },
  { label: "Per-agent spend tracking", relay: true, cloud: true, ide: false },
  { label: "Zero telemetry, zero cloud", relay: true, cloud: false, ide: false },
  { label: "Uses your existing CLI auth", relay: true, cloud: false, ide: true },
];

function Mark({ ok }: { ok: boolean }) {
  return ok ? (
    <span className="text-relay-green">✓</span>
  ) : (
    <span className="text-relay-muted">—</span>
  );
}

export function Compare() {
  return (
    <section id="compare" className="border-t border-relay-border">
      <div className="mx-auto max-w-6xl px-6 py-20">
        <p className="text-sm text-relay-primary">{"// vs the rest"}</p>
        <h2 className="mt-3 font-display text-3xl font-black tracking-tight uppercase sm:text-4xl">
          The receipts.
        </h2>
        <p className="mt-4 max-w-2xl text-relay-muted">
          Feature-for-feature against cloud agent platforms and IDE plugins.
          No asterisks, no fine print.
        </p>
        <div className="mt-12 overflow-x-auto border border-relay-border">
          <table className="w-full min-w-[640px] border-collapse text-sm">
            <thead>
              <tr className="border-b border-relay-border bg-relay-surface text-left">
                <th className="p-4 font-medium text-relay-muted">Capability</th>
                <th className="p-4 font-display text-relay-primary">
                  RELAY
                  <span className="block font-mono text-xs font-normal text-relay-muted">
                    native · local
                  </span>
                </th>
                <th className="p-4 font-medium text-relay-muted">
                  Cloud Agents
                  <span className="block text-xs font-normal">devin · replit</span>
                </th>
                <th className="p-4 font-medium text-relay-muted">
                  IDE Plugins
                  <span className="block text-xs font-normal">vscode ext.</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {ROWS.map((r) => (
                <tr key={r.label} className="border-b border-relay-border last:border-0">
                  <td className="p-4 text-relay-text">{r.label}</td>
                  <td className="p-4">
                    <Mark ok={r.relay} />
                  </td>
                  <td className="p-4">
                    <Mark ok={r.cloud} />
                  </td>
                  <td className="p-4">
                    <Mark ok={r.ide} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
