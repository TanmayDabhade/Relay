"use client";

import { useState, type FormEvent } from "react";

const MIN_PASSWORD_LENGTH = 6;

export function WaitlistCta() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [alreadyRegistered, setAlreadyRegistered] = useState(false);
  const [sentTo, setSentTo] = useState("");

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError("");
    setAlreadyRegistered(false);
    try {
      const res = await fetch("/api/waitlist", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      });
      if (!res.ok) {
        const data = (await res.json().catch(() => ({}))) as {
          error?: string;
          alreadyRegistered?: boolean;
        };
        setError(data.error ?? "Something went wrong. Try again.");
        setAlreadyRegistered(Boolean(data.alreadyRegistered));
        setLoading(false);
        return;
      }
      // Position is recorded, but viewing /waitlist requires confirming the
      // account we just emailed a confirmation link for.
      setSentTo(email);
    } catch {
      setError("Network error. Try again.");
      setLoading(false);
    }
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
        {sentTo ? (
          <p className="mt-8 text-sm text-relay-muted">
            Check <span className="text-relay-text">{sentTo}</span> for a link
            to confirm and see your spot.
          </p>
        ) : (
          <form
            onSubmit={handleSubmit}
            className="mt-8 flex flex-col gap-3 sm:mx-auto sm:w-80"
          >
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@yourdomain.dev"
              disabled={loading}
              className="w-full border border-relay-border-strong bg-relay-surface px-4 py-3 text-sm text-relay-text placeholder:text-relay-muted focus:border-relay-primary focus:outline-none disabled:opacity-60"
            />
            <input
              type="password"
              required
              minLength={MIN_PASSWORD_LENGTH}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Password"
              disabled={loading}
              className="w-full border border-relay-border-strong bg-relay-surface px-4 py-3 text-sm text-relay-text placeholder:text-relay-muted focus:border-relay-primary focus:outline-none disabled:opacity-60"
            />
            <button
              type="submit"
              disabled={loading}
              className="border border-relay-primary bg-relay-primary px-6 py-3 text-sm font-bold text-relay-primary-contrast transition-opacity hover:opacity-90 disabled:opacity-60"
            >
              {loading ? "$ Joining…" : "$ Get Access"}
            </button>
          </form>
        )}
        {error && (
          <p className="mt-4 text-sm text-relay-red">
            {error}
            {alreadyRegistered && (
              <>
                {" "}
                <a href="/login" className="underline hover:text-relay-text">
                  Sign in
                </a>
              </>
            )}
          </p>
        )}
      </div>
    </section>
  );
}
