-- Relay waitlist schema.
-- Run this in your Supabase project's SQL editor (SQL Editor → New query → paste → Run).
-- Safe to re-run: every statement is idempotent.

-- 1. The waitlist table. One row per email; position is derived from `id` order.
create table if not exists public.waitlist (
  id         bigint generated always as identity primary key,
  email      text        not null unique,
  created_at timestamptz not null default now()
);

create index if not exists waitlist_created_at_idx on public.waitlist (created_at);

-- 2. Lock the table down. The app talks to Supabase with the service-role key,
--    which bypasses RLS, so no anon/public policies are needed. Enabling RLS
--    with no policies means the public anon key cannot read or write this table.
alter table public.waitlist enable row level security;

-- 3. Atomic, race-safe signup. Inserts the email if new, then returns the
--    caller's 1-based position on the list. Idempotent: signing up again with
--    the same email returns the original position and already_joined = true.
create or replace function public.join_waitlist(p_email text)
-- `position` is a Postgres reserved word, so it MUST be double-quoted here; unquoted
-- it's a syntax error and this whole file fails to deploy. Quoting keeps the literal
-- column (and PostgREST/RPC JSON key) named `position`, which is what lib/waitlist.ts reads.
returns table ("position" bigint, already_joined boolean)
language plpgsql
security definer
set search_path = public
as $$
declare
  v_email   text := lower(trim(p_email));
  v_id      bigint;
  v_existed boolean;
begin
  insert into public.waitlist (email)
  values (v_email)
  on conflict (email) do nothing
  returning id into v_id;

  if v_id is null then
    -- Row already existed; fetch its id.
    select id into v_id from public.waitlist where email = v_email;
    v_existed := true;
  else
    v_existed := false;
  end if;

  return query
    select
      (select count(*) from public.waitlist w where w.id <= v_id)::bigint,
      v_existed;
end;
$$;


-- 4. Lock the RPC down to the service role. The app only ever calls this from the
--    server with the service-role key (landing-page/lib/waitlist.ts). Without this,
--    the default PUBLIC grant lets any anon caller hit /rest/v1/rpc/join_waitlist and
--    insert arbitrary emails, bypassing the API route's validation. (Flagged by the
--    Supabase security advisor: anon_security_definer_function_executable.)
revoke execute on function public.join_waitlist(text) from public, anon, authenticated;
grant  execute on function public.join_waitlist(text) to service_role;
