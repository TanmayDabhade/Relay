"use client";

import { useState, type FormEvent } from "react";

export function WaitlistCta() {
  const [email, setEmail] = useState("");
  const [submitted, setSubmitted] = useState(false);

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitted(true);
  }

  return (
    <section id="waitlist" className="border-t border-relay-border">
      <div className="mx-auto max-w-3xl px-6 py-24 text-center">
        <h2 className="font-display text-3xl font-black tracking-tight uppercase sm:text-4xl">
          Get Relay before <span className="text-relay-primary">everyone else.</span>
        </h2>
        <p className="mt-4 text-relay-muted">
          Beta invites drop weekly. Waitlist gets 3 months of Pro free at
          launch.
        </p>
        {submitted ? (
          <p className="mt-8 text-relay-green">
            $ You&apos;re on the list. Check your inbox.
          </p>
        ) : (
          <form
            onSubmit={handleSubmit}
            className="mt-8 flex flex-col gap-3 sm:flex-row sm:justify-center"
          >
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@yourdomain.dev"
              className="w-full border border-relay-border-strong bg-relay-surface px-4 py-3 text-sm text-relay-text placeholder:text-relay-muted focus:border-relay-primary focus:outline-none sm:w-80"
            />
            <button
              type="submit"
              className="border border-relay-primary bg-relay-primary px-6 py-3 text-sm font-bold text-relay-primary-contrast transition-opacity hover:opacity-90"
            >
              $ Get Access
            </button>
          </form>
        )}
      </div>
    </section>
  );
}
