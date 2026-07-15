-- Relay desktop app: user profiles + subscription plan.
-- Run this in your Supabase project's SQL editor (SQL Editor → New query → paste → Run).
-- Safe to re-run: every statement is idempotent.

-- 1. One row per authenticated user, keyed to auth.users. Plan starts at 'free' and is
--    flipped to 'paid' by the Stripe webhook once billing exists (not yet built — see
--    landing-page/AGENTS.md-adjacent design notes). The desktop app reads this row after
--    magic-link sign-in to decide what to enforce locally.
create table if not exists public.profiles (
  id         uuid primary key references auth.users (id) on delete cascade,
  email      text        not null,
  plan       text        not null default 'free' check (plan in ('free', 'paid')),
  created_at timestamptz not null default now()
);

-- 2. RLS: a signed-in user can only ever see/update their own row. No public/anon access —
--    everything here requires an authenticated session.
alter table public.profiles enable row level security;

drop policy if exists "profiles_select_own" on public.profiles;
create policy "profiles_select_own"
  on public.profiles for select
  to authenticated
  using ((select auth.uid()) = id);

drop policy if exists "profiles_update_own" on public.profiles;
create policy "profiles_update_own"
  on public.profiles for update
  to authenticated
  using ((select auth.uid()) = id)
  with check ((select auth.uid()) = id);

-- 3. Auto-create a profile row the moment a user signs up (their first magic-link
--    exchangeCodeForSession call). SECURITY DEFINER is required here — the inserting
--    user's own RLS policies (defined above) would never permit them to insert a row
--    before their session exists, so this runs as the trigger owner instead. It lives in
--    public (not auth) because the SQL editor's role has no CREATE privilege on the auth
--    schema itself — only Supabase's internal auth admin role owns that schema. Attaching
--    the trigger to auth.users is still fine; that's the supported Supabase extension point.
create or replace function public.handle_new_user()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
begin
  insert into public.profiles (id, email)
  values (new.id, new.email)
  on conflict (id) do nothing;
  return new;
end;
$$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute function public.handle_new_user();

-- 4. This function only ever runs from the trigger above (as its definer), so it never
--    needs to be callable over the REST API. Revoke the default grants that would expose
--    it at /rest/v1/rpc/handle_new_user. (Flagged by the Supabase security advisor:
--    anon_security_definer_function_executable.)
revoke execute on function public.handle_new_user() from public, anon, authenticated;
