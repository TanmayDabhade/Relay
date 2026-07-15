import { NextResponse } from "next/server";
import type { EmailOtpType } from "@supabase/supabase-js";
import { createSupabaseServerClient } from "@/lib/supabase-server";

// Landing spot for Supabase's password-signup confirmation email. Unlike a magic link
// (which carries a `code` for exchangeCodeForSession), an email/password signup
// confirmation carries `token_hash` + `type` and is completed via verifyOtp — see
// https://supabase.com/docs/guides/auth/server-side/email-based-auth-with-pkce-flow-for-ssr.
// Verifying sets the session cookie (via the server client's cookie adapter), then redirects
// to wherever the link was generated for (/waitlist or /admin) via the `next` param.
export async function GET(request: Request) {
  const { searchParams, origin } = new URL(request.url);
  const token_hash = searchParams.get("token_hash");
  const type = searchParams.get("type") as EmailOtpType | null;
  const next = searchParams.get("next") ?? "/";

  if (token_hash && type) {
    const supabase = await createSupabaseServerClient();
    const { error } = await supabase.auth.verifyOtp({ type, token_hash });
    if (!error) {
      return NextResponse.redirect(`${origin}${next}`);
    }
  }

  return NextResponse.redirect(`${origin}/?auth_error=1`);
}
