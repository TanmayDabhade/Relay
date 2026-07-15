"use client";

import { useState } from "react";

export function WaitlistStatus({ position }: { position: number }) {
  const [copied, setCopied] = useState(false);

  async function copyShareLink() {
    const url = typeof window !== "undefined" ? window.location.origin : "";
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard can be unavailable (insecure context / permissions); ignore.
    }
  }

  return (
    <section className="border-b border-relay-border bg-relay-surface">
      <div className="mx-auto max-w-6xl px-6 py-14 text-center">
        <p className="text-xs tracking-widest text-relay-muted uppercase">
          You&apos;re on the waitlist
        </p>
        <p className="mt-3 font-display text-6xl font-black tracking-tight text-relay-primary sm:text-7xl">
          #{position}
        </p>
        <p className="mt-4 text-relay-muted">
          Beta invites drop weekly. We&apos;ll reach out when it&apos;s your turn.
        </p>
        <div className="mt-8 flex flex-col items-center gap-3">
          <p className="text-sm text-relay-muted">
            Share Relay to move up the list.
          </p>
          <button
            onClick={copyShareLink}
            className="border border-relay-primary px-6 py-3 text-sm font-bold text-relay-primary transition-colors hover:bg-relay-primary hover:text-relay-primary-contrast"
          >
            {copied ? "$ Link copied" : "$ Copy share link"}
          </button>
        </div>
      </div>
    </section>
  );
}
