interface ComingSoonViewProps {
  title: string;
}

export function ComingSoonView({ title }: ComingSoonViewProps) {
  return (
    <div>
      <h1 style={{ fontSize: "var(--text-lg)", marginBottom: "var(--space-2)" }}>
        {title}
      </h1>
      <p style={{ color: "var(--text-muted)" }}>Coming soon.</p>
    </div>
  );
}
