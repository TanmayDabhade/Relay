import { getSupabaseAnon } from "./supabase";
import { createSupabaseServerClient } from "./supabase-server";

export type SignUpResult = "sent" | "already-registered" | "error";

/**
 * Creates a password-based account for `email` and sends a confirmation email. Clicking it
 * lands on /auth/confirm, which verifies the token and redirects to `next`. No session is
 * created until confirmation — /waitlist and /admin both gate on a real session, so an
 * unconfirmed signup can't view either.
 *
 * Supabase intentionally obfuscates whether an email is already registered (to prevent
 * enumeration): signUp() on an existing *confirmed* address returns success with an empty
 * `identities` array instead of an error. That's the only signal available to tell "brand
 * new signup" apart from "you already have an account" — checked below, defensively (an
 * absent/unexpected shape is treated as a normal new signup, not as "already registered").
 */
export async function signUpWithPassword(
  email: string,
  password: string,
  origin: string,
  next: string,
): Promise<SignUpResult> {
  const supabase = getSupabaseAnon();
  const { data, error } = await supabase.auth.signUp({
    email,
    password,
    options: {
      emailRedirectTo: `${origin}/auth/confirm?next=${encodeURIComponent(next)}`,
    },
  });
  if (error) {
    console.error("signUp failed:", error);
    return "error";
  }
  if (data.user && Array.isArray(data.user.identities) && data.user.identities.length === 0) {
    return "already-registered";
  }
  return "sent";
}

/**
 * Signs an already-confirmed user in with email + password. Unlike magic-link, this
 * establishes a session immediately — no email round trip — via the cookie-aware server
 * client, so the session cookie is written directly onto this request/response.
 */
export async function signInWithPassword(
  email: string,
  password: string,
): Promise<{ ok: true } | { ok: false; message: string }> {
  const supabase = await createSupabaseServerClient();
  const { error } = await supabase.auth.signInWithPassword({ email, password });
  if (error) {
    return { ok: false, message: error.message };
  }
  return { ok: true };
}
