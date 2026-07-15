import { NextResponse } from "next/server";
import { signUpWithPassword } from "@/lib/auth";
import { joinWaitlist } from "@/lib/waitlist";

// Deliberately permissive email shape check — real validation is deliverability,
// which we don't do here. This just rejects obvious garbage before hitting the DB.
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
// Supabase's own default minimum — matches what signUp() would otherwise reject with a
// less friendly error, so this fails fast with a clear message instead.
const MIN_PASSWORD_LENGTH = 6;

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
  if (password.length < MIN_PASSWORD_LENGTH) {
    return NextResponse.json(
      { error: `Password must be at least ${MIN_PASSWORD_LENGTH} characters.` },
      { status: 400 },
    );
  }

  const position = await joinWaitlist(email);
  if (position === null) {
    return NextResponse.json(
      { error: "Something went wrong. Please try again." },
      { status: 500 },
    );
  }

  // Position is recorded immediately, but /waitlist is gated behind a real
  // session — confirming the account is what proves this signup owns the address.
  const { origin } = new URL(request.url);
  const result = await signUpWithPassword(email, password, origin, "/waitlist");
  if (result === "already-registered") {
    return NextResponse.json(
      { error: "You already have an account. Sign in instead.", alreadyRegistered: true },
      { status: 409 },
    );
  }
  if (result === "error") {
    return NextResponse.json(
      { error: "Joined, but couldn't create your account. Please try again." },
      { status: 500 },
    );
  }

  return NextResponse.json({ position });
}
