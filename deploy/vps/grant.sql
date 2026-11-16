-- Runs as the Neon owner role after `sqlx migrate run` (see nata-migrate).
-- Idempotent: safe to run on every deploy.
--
-- The API connects as nata_app, a least-privilege role. Migrations own the
-- schema, so every table is created with RLS forced; nata_app still gets the
-- DML grants it needs and the row policies do the tenant filtering.

do $$
begin
  if not exists (select 1 from pg_roles where rolname = 'nata_app') then
    create role nata_app login;
  end if;
end
$$;

-- psql substitutes :'app_password' as a quoted literal, so the password never
-- has to be escaped by hand.
alter role nata_app password :'app_password';

grant usage on schema public to nata_app;

grant select, insert, update, delete on all tables in schema public to nata_app;
grant usage, select on all sequences in schema public to nata_app;

-- Covers app_tenant_id(), app_user_id() and the SECURITY DEFINER lookups that
-- run before a tenant is known (auth_find_user_id, auth_find_invitation,
-- auth_has_pending_invitation). Those functions are revoked from public.
grant execute on all functions in schema public to nata_app;

-- Future migrations create tables as the owner; extend the same grants.
alter default privileges in schema public
  grant select, insert, update, delete on tables to nata_app;
alter default privileges in schema public
  grant usage, select on sequences to nata_app;
alter default privileges in schema public
  grant execute on functions to nata_app;
