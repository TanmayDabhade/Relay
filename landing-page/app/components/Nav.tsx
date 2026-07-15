const LINKS = [
  { href: "#features", label: "Features" },
  { href: "#security", label: "Security" },
  { href: "#compare", label: "Compare" },
  { href: "#pricing", label: "Pricing" },
];

export function Nav() {
  return (
    <header className="sticky top-0 z-50 border-b border-relay-border bg-relay-bg/90 backdrop-blur">
      <div className="mx-auto flex max-w-6xl items-center justify-between px-6 py-4">
        <a href="#top" className="font-display text-lg font-black tracking-tight">
          RELAY<span className="text-relay-primary">.</span>
        </a>
        <nav className="hidden items-center gap-8 text-sm text-relay-muted md:flex">
          {LINKS.map((l) => (
            <a
              key={l.href}
              href={l.href}
              className="transition-colors hover:text-relay-text"
            >
              {l.label}
            </a>
          ))}
        </nav>
        <div className="flex items-center gap-4">
          <a
            href="/login"
            className="hidden text-sm text-relay-muted transition-colors hover:text-relay-text sm:inline"
          >
            Sign in
          </a>
          <a
            href="#waitlist"
            className="border border-relay-primary px-4 py-2 text-sm font-medium text-relay-primary transition-colors hover:bg-relay-primary hover:text-relay-primary-contrast"
          >
            Join Waitlist →
          </a>
        </div>
      </div>
    </header>
  );
}
