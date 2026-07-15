import { createClient, type SupabaseClient } from "@supabase/supabase-js";

// Server-only Supabase client using the service-role key. This key bypasses
// Row Level Security and must NEVER be exposed to the browser — that's why the
// env vars are NOT prefixed with NEXT_PUBLIC_ and this module is only ever
// imported from server code (route handlers, server components).
let cached: SupabaseClient | null = null;

export function getSupabaseAdmin(): SupabaseClient {
  if (cached) return cached;

  const url = process.env.SUPABASE_URL;
  const serviceRoleKey = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (!url || !serviceRoleKey) {
    throw new Error(
      "Missing SUPABASE_URL or SUPABASE_SERVICE_ROLE_KEY. Copy .env.local.example to .env.local and fill them in.",
    );
  }

  cached = createClient(url, serviceRoleKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  return cached;
}

let cachedAnon: SupabaseClient | null = null;

// Anon-key client for triggering magic-link emails (auth.signInWithOtp).
// Stateless — no session to persist here, the browser picks up the session
// when it follows the link to /auth/callback.
export function getSupabaseAnon(): SupabaseClient {
  if (cachedAnon) return cachedAnon;

  const url = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const anonKey = process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY;

  if (!url || !anonKey) {
    throw new Error(
      "Missing NEXT_PUBLIC_SUPABASE_URL or NEXT_PUBLIC_SUPABASE_ANON_KEY. Copy .env.local.example to .env.local and fill them in.",
    );
  }

  cachedAnon = createClient(url, anonKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  return cachedAnon;
}
