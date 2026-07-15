// Admin allowlist: comma-separated emails in the ADMIN_EMAILS env var.
// There's no admin role in the database — anyone whose authenticated email
// matches this list can view /admin. Keep the list short and server-only.
export function isAdminEmail(email: string): boolean {
  const allowlist = (process.env.ADMIN_EMAILS ?? "")
    .split(",")
    .map((e) => e.trim().toLowerCase())
    .filter(Boolean);
  return allowlist.includes(email.toLowerCase());
}
