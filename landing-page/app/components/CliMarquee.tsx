const CLIS = [
  { name: "claude-code", dir: ".claude" },
  { name: "codex", dir: ".codex" },
  { name: "cursor-cli", dir: ".cursor" },
  { name: "gemini-cli", dir: ".gemini" },
];

function Track() {
  return (
    <div className="flex shrink-0 items-center gap-8 pr-8">
      {CLIS.map((c) => (
        <span
          key={c.name}
          className="flex items-center gap-2 whitespace-nowrap text-sm text-relay-muted"
        >
          <span className="text-relay-green">✳</span> {c.name}
          <span className="text-relay-border-strong">·</span>
          <span>{c.dir}</span>
        </span>
      ))}
    </div>
  );
}

export function CliMarquee() {
  return (
    <div className="overflow-hidden border-y border-relay-border py-4 [mask-image:linear-gradient(to_right,transparent,black_10%,black_90%,transparent)]">
      <div className="flex w-max animate-marquee">
        <Track />
        <Track />
      </div>
    </div>
  );
}
