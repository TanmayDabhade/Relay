import { getSupabaseAdmin } from "./supabase";

/**
 * Registers `email` on the waitlist (idempotent) and returns the 1-based
 * position on the list, or null if the write failed.
 */
export async function joinWaitlist(email: string): Promise<number | null> {
  const supabase = getSupabaseAdmin();
  const { data, error } = await supabase.rpc("join_waitlist", { p_email: email });
  if (error) {
    console.error("join_waitlist rpc failed:", error);
    return null;
  }
  const row = Array.isArray(data) ? data[0] : data;
  const position = Number(row?.position);
  return Number.isFinite(position) ? position : null;
}

/**
 * Looks up the current waitlist position for an already-registered email.
 * Returns null if the email isn't on the list or the query failed.
 */
export async function getPositionForEmail(email: string): Promise<number | null> {
  const supabase = getSupabaseAdmin();

  const { data, error } = await supabase
    .from("waitlist")
    .select("id")
    .eq("email", email)
    .maybeSingle();
  if (error || !data) return null;

  const { count, error: countError } = await supabase
    .from("waitlist")
    .select("*", { count: "exact", head: true })
    .lte("id", data.id);
  if (countError) return null;

  return count ?? null;
}

export type WaitlistEntry = { email: string; createdAt: string };

/** Full signup list, oldest first (i.e. in position order), for the admin view. */
export async function listWaitlist(): Promise<WaitlistEntry[]> {
  const supabase = getSupabaseAdmin();
  const { data, error } = await supabase
    .from("waitlist")
    .select("email, created_at")
    .order("id", { ascending: true });
  if (error || !data) return [];
  return data.map((row) => ({ email: row.email, createdAt: row.created_at }));
}
