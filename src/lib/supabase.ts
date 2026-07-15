import { createClient } from "@supabase/supabase-js";

// Same Supabase project as landing-page's anon-key client (lib/supabase.ts there) — a user
// who verified their email on the waitlist signs into this app with the same account. This
// is the only network dependency auth introduces: session logs/transcripts/costs stay in
// the local SQLite DB and are never sent here or anywhere else (see CLAUDE.md's local-first
// architecture notes).
export const supabase = createClient(
  import.meta.env.VITE_SUPABASE_URL,
  import.meta.env.VITE_SUPABASE_ANON_KEY,
);
