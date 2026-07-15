import { createContext, useCallback, useContext, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import type { EmailOtpType, User } from "@supabase/supabase-js";
import { supabase } from "../lib/supabase";
import { setCurrentPlan } from "../lib/tauri";

type Status = "loading" | "signed-out" | "confirm-sent" | "signed-in";

interface AuthState {
  status: Status;
  user: User | null;
  plan: "free" | "paid";
  error: string | null;
  signUp: (email: string, password: string) => Promise<void>;
  signInWithPassword: (email: string, password: string) => Promise<void>;
  signOut: () => Promise<void>;
  /** Clears a "confirm-sent"/error state back to the sign-in form, e.g. to try again. */
  resetToSignIn: () => void;
}

const AuthContext = createContext<AuthState | null>(null);

async function fetchPlan(userId: string): Promise<"free" | "paid"> {
  const { data, error } = await supabase.from("profiles").select("plan").eq("id", userId).single();
  if (error || !data) {
    // profiles row is created by a DB trigger on signup (see profiles_schema.sql) — a
    // missing/errored read here means "not caught up yet or unreachable," not "paid,"
    // so free is the only safe fallback.
    return "free";
  }
  return data.plan === "paid" ? "paid" : "free";
}

/** Fetches the user's current plan from Supabase and pushes it down to Rust — the one
 * path every plan-changing event (sign-in, deep-link completion, window focus, manual
 * refresh after upgrading) funnels through, so Rust's cache never diverges from the
 * frontend's view of it. */
async function syncPlan(user: User) {
  const plan = await fetchPlan(user.id);
  await setCurrentPlan(user.email ?? null, plan);
  return plan;
}

/**
 * Owns the Supabase session lifecycle for the desktop app — email+password sign-up/sign-in,
 * the relay://auth/callback deep-link handoff for signup confirmation, and keeping Rust's
 * cached plan (`auth::PlanState`) in sync with `profiles.plan`. See CLAUDE.md / the auth
 * design notes for why Rust rather than the frontend enforces free-tier limits: this hook's
 * job is only "know the plan, tell Rust," not gate anything itself.
 */
export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<Status>("loading");
  const [user, setUser] = useState<User | null>(null);
  const [plan, setPlan] = useState<"free" | "paid">("free");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      if (session?.user) {
        setUser(session.user);
        syncPlan(session.user).then(setPlan);
        setStatus("signed-in");
      } else {
        setStatus("signed-out");
      }
    });

    const { data: subscription } = supabase.auth.onAuthStateChange((_event, session) => {
      if (session?.user) {
        setUser(session.user);
        setError(null);
        syncPlan(session.user).then(setPlan);
        setStatus("signed-in");
      } else {
        setUser(null);
        setPlan("free");
        setStatus("signed-out");
      }
    });

    // signUp's confirmation email points at relay://auth/callback with `token_hash`+`type`
    // params (PKCE email-confirmation shape, not the `code` param a magic link would carry
    // — see lib.rs's deep-link setup for how macOS hands the URL to this running app, and
    // landing-page/app/auth/confirm/route.ts for the equivalent web-side handler).
    const unlistenDeepLink = listen<string>("deep-link", async (event) => {
      const url = new URL(event.payload);
      const tokenHash = url.searchParams.get("token_hash");
      const type = url.searchParams.get("type") as EmailOtpType | null;
      if (!tokenHash || !type) return;

      const { error: verifyError } = await supabase.auth.verifyOtp({ type, token_hash: tokenHash });
      if (verifyError) {
        setError(verifyError.message);
        setStatus("signed-out");
      }
      // On success, onAuthStateChange above fires with the new session — no further action
      // needed here.
    });

    // Plan can change server-side at any time (Stripe webhook after a checkout the user
    // completed in their browser) — re-check whenever the window regains focus, since
    // that's the natural moment a user returns from an "Upgrade" tab.
    const onFocus = () => {
      if (user) syncPlan(user).then(setPlan);
    };
    window.addEventListener("focus", onFocus);

    return () => {
      subscription.subscription.unsubscribe();
      unlistenDeepLink.then((fn) => fn());
      window.removeEventListener("focus", onFocus);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `user` is read, not depended on: re-subscribing per sign-in would drop deep-link events mid-flow.
  }, []);

  const signUp = useCallback(async (email: string, password: string) => {
    setError(null);
    const { data, error: signUpError } = await supabase.auth.signUp({
      email,
      password,
      options: { emailRedirectTo: "relay://auth/callback" },
    });
    if (signUpError) {
      setError(signUpError.message);
      return;
    }
    // Supabase obfuscates whether an email is already registered (anti-enumeration):
    // signUp() on an existing *confirmed* address returns success with an empty
    // `identities` array instead of an error — see lib/auth.ts on the landing-page side
    // for the same check.
    if (data.user && Array.isArray(data.user.identities) && data.user.identities.length === 0) {
      setError("You already have an account — sign in instead.");
      return;
    }
    setStatus("confirm-sent");
  }, []);

  const signInWithPassword = useCallback(async (email: string, password: string) => {
    setError(null);
    const { error: signInError } = await supabase.auth.signInWithPassword({ email, password });
    if (signInError) {
      setError(signInError.message);
    }
    // On success, onAuthStateChange above fires with the new session immediately — no
    // email round trip needed for sign-in, unlike signUp's confirmation step.
  }, []);

  const signOut = useCallback(async () => {
    await supabase.auth.signOut();
    await setCurrentPlan(null, "free");
  }, []);

  const resetToSignIn = useCallback(() => {
    setError(null);
    setStatus("signed-out");
  }, []);

  return (
    <AuthContext.Provider
      value={{ status, user, plan, error, signUp, signInWithPassword, signOut, resetToSignIn }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthState {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within an AuthProvider");
  return ctx;
}
