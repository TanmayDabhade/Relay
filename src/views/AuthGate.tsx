import { useState } from "react";
import type { FormEvent } from "react";
import { Button } from "../components/ui/Button";
import { TextInput } from "../components/ui/TextInput";
import { useAuth } from "../hooks/useAuth";
import "./AuthGate.css";

/**
 * The whole app is gated behind this — see App.tsx, which renders only this (no sidebar, no
 * views) for every status except "signed-in". Nothing past this point (dashboard, projects,
 * sessions, local session data) is reachable without a session, matching the rest of the
 * app's local-first stance: auth/plan is the one thing that talks to Supabase, but it's a
 * hard gate, not a soft prompt layered on top of an already-visible dashboard.
 */
export function AuthGate() {
  const { status, error, signUp, signInWithPassword, resetToSignIn } = useAuth();

  return (
    <div className="auth-gate">
      <div className="dashboard-card auth-gate-card">
        <div className="auth-gate-brand">Relay</div>
        {status === "loading" && <p className="dashboard-view-status">Loading…</p>}
        {status === "signed-out" && (
          <AuthForm onSignUp={signUp} onSignIn={signInWithPassword} error={error} />
        )}
        {status === "confirm-sent" && <ConfirmSentNotice onBack={resetToSignIn} />}
      </div>
    </div>
  );
}

function AuthForm({
  onSignUp,
  onSignIn,
  error,
}: {
  onSignUp: (email: string, password: string) => Promise<void>;
  onSignIn: (email: string, password: string) => Promise<void>;
  error: string | null;
}) {
  const [mode, setMode] = useState<"sign-in" | "sign-up">("sign-in");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    await (mode === "sign-up" ? onSignUp(email, password) : onSignIn(email, password));
    setSubmitting(false);
  }

  return (
    <form className="auth-gate-form" onSubmit={handleSubmit}>
      <div className="profile-mode-toggle">
        <button
          type="button"
          className={`profile-mode-option${mode === "sign-in" ? " active" : ""}`}
          onClick={() => setMode("sign-in")}
        >
          Sign in
        </button>
        <button
          type="button"
          className={`profile-mode-option${mode === "sign-up" ? " active" : ""}`}
          onClick={() => setMode("sign-up")}
        >
          Sign up
        </button>
      </div>
      <TextInput
        type="email"
        placeholder="you@example.com"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        required
        disabled={submitting}
      />
      <TextInput
        type="password"
        placeholder="Password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        required
        minLength={6}
        disabled={submitting}
      />
      <Button type="submit" disabled={submitting || email.trim() === "" || password === ""}>
        {submitting
          ? mode === "sign-up"
            ? "Creating account…"
            : "Signing in…"
          : mode === "sign-up"
            ? "Create account"
            : "Sign in"}
      </Button>
      {error && <p className="profile-signin-error">{error}</p>}
    </form>
  );
}

function ConfirmSentNotice({ onBack }: { onBack: () => void }) {
  return (
    <div className="profile-link-sent">
      <p>Check your email for a confirmation link. Clicking it will bring you back here, signed in.</p>
      <button className="profile-link-back" onClick={onBack}>
        Back
      </button>
    </div>
  );
}
