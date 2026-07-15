import { createServerClient } from "@supabase/ssr";
import { cookies } from "next/headers";

// Session-aware Supabase client for Server Components and Route Handlers.
// Uses the public anon key + request cookies, so `auth.getUser()` reflects
// whoever's magic-link session is attached to this request — unlike
// getSupabaseAdmin() (service role), this respects RLS and cannot be spoofed
// by editing a cookie value.
export async function createSupabaseServerClient() {
  const cookieStore = await cookies();

  return createServerClient(
    process.env.NEXT_PUBLIC_SUPABASE_URL!,
    process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY!,
    {
      cookies: {
        getAll() {
          return cookieStore.getAll();
        },
        setAll(cookiesToSet) {
          try {
            cookiesToSet.forEach(({ name, value, options }) =>
              cookieStore.set(name, value, options),
            );
          } catch {
            // Called from a Server Component render, which can't set cookies.
            // proxy.ts refreshes the session cookie on every request instead.
          }
        },
      },
    },
  );
}
