"use client";

import { useState, type FormEvent } from "react";

interface LoginFormProps {
  /** Where to send the browser after a successful sign-in. */
  next: string;
}

/** Shared by /login and /admin/login — same email+password sign-in, different destination.
 * Unlike the old magic-link flow, this establishes a session synchronously (see
 * lib/auth.ts's signInWithPassword), so success means an immediate redirect, not a
 * "check your email" step. */
export function LoginForm({ next }: LoginFormProps) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError("");
    try {
      const res = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      });
      if (!res.ok) {
        const data = (await res.json().catch(() => ({}))) as { error?: string };
        setError(data.error ?? "Something went wrong. Try again.");
        setLoading(false);
        return;
      }
      window.location.href = next;
    } catch {
      setError("Network error. Try again.");
      setLoading(false);
    }
  }

  return (
    <form onSubmit={handleSubmit} className="mt-8 flex flex-col gap-3">
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
        {loading ? "$ Signing in…" : "$ Sign in"}
      </button>
      {error && <p className="mt-2 text-sm text-relay-red">{error}</p>}
    </form>
  );
}
