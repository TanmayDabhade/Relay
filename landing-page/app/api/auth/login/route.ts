import { NextResponse } from "next/server";
import { signInWithPassword } from "@/lib/auth";

// Deliberately permissive email shape check — same as /api/waitlist.
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

// Generic email+password sign-in, used by /admin/login. Anyone can attempt to sign in with
// any email — that's fine, since a wrong password just fails, and /admin separately checks
// the allowlist (lib/admin.ts) before showing anything even for a correct login.
export async function POST(request: Request) {
  let body: { email?: unknown; password?: unknown };
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: "Invalid request." }, { status: 400 });
  }

  const email =
    typeof body.email === "string" ? body.email.trim().toLowerCase() : "";
  if (!EMAIL_RE.test(email)) {
    return NextResponse.json(
      { error: "Enter a valid email address." },
      { status: 400 },
    );
  }

  const password = typeof body.password === "string" ? body.password : "";
  if (password === "") {
    return NextResponse.json({ error: "Enter your password." }, { status: 400 });
  }

  const result = await signInWithPassword(email, password);
  if (!result.ok) {
    return NextResponse.json({ error: result.message }, { status: 401 });
  }

  return NextResponse.json({ ok: true });
}
