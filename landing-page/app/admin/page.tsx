import { redirect } from "next/navigation";
import { createSupabaseServerClient } from "@/lib/supabase-server";
import { isAdminEmail } from "@/lib/admin";
import { listWaitlist } from "@/lib/waitlist";

// Reads the auth session, so this route must render per-request.
export const dynamic = "force-dynamic";

export default async function AdminPage() {
  const supabase = await createSupabaseServerClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user?.email) redirect("/admin/login");
  if (!isAdminEmail(user.email)) redirect("/");

  const entries = await listWaitlist();

  return (
    <main className="mx-auto max-w-3xl px-6 py-16">
      <div className="flex items-center justify-between">
        <h1 className="font-display text-2xl font-black tracking-tight uppercase">
          Waitlist &mdash; {entries.length}
        </h1>
        <form action="/api/auth/logout" method="POST">
          <button
            type="submit"
            className="border border-relay-border-strong px-4 py-2 text-xs font-bold text-relay-muted hover:text-relay-text"
          >
            $ Sign out
          </button>
        </form>
      </div>
      <table className="mt-8 w-full text-left text-sm">
        <thead>
          <tr className="border-b border-relay-border text-relay-muted">
            <th className="py-2 pr-4 font-normal">#</th>
            <th className="py-2 pr-4 font-normal">Email</th>
            <th className="py-2 font-normal">Joined</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((entry, i) => (
            <tr key={entry.email} className="border-b border-relay-border">
              <td className="py-2 pr-4 text-relay-muted">{i + 1}</td>
              <td className="py-2 pr-4">{entry.email}</td>
              <td className="py-2 text-relay-muted">
                {new Date(entry.createdAt).toLocaleString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </main>
  );
}
