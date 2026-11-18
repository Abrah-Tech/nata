-- Audit log: who did what, for the actions that have no other history.
--
-- Domain documents already carry their own author and are the trail for those:
-- a sale, a production order, and a recipe version all record who created them,
-- and a stock correction is a signed stock_movements row with a note. This table
-- covers the changes that would otherwise leave no trace: membership and role
-- changes, revoking an invitation, deleting a payment, verifying a payment from
-- the operator CLI, and tenant settings.
--
-- Append-only by construction: there is no update or delete API, and the RLS
-- policy is `for all` only so the tenant owner can read it. Nothing in the API
-- writes to this table except nata_db::audit::record.

create table audit_logs (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    -- Null when the actor is not a member of the tenant: the operator who
    -- verifies a payment from the CLI has no row in `users`.
    actor_user_id uuid references users (id) on delete set null,
    -- Dotted action name, e.g. `member.role_changed`.
    action text not null check (length(btrim(action)) between 1 and 80),
    -- What the action acted on, e.g. `member`, `invitation`, `payment`.
    entity_type text not null check (length(btrim(entity_type)) between 1 and 40),
    entity_id uuid,
    -- Action-specific detail: before and after values, the operator label, the
    -- payment amount. Schemaless on purpose, so a new action does not need a
    -- migration.
    metadata jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

comment on table audit_logs is
    'Append-only record of sensitive actions; the only writer is nata_db::audit::record.';
comment on column audit_logs.actor_user_id is
    'The member who acted. Null for an action taken outside the member directory, such as the operator verifying a payment.';
comment on column audit_logs.metadata is
    'Schemaless per-action detail. Never store credentials or tokens here.';

-- The audit screen reads one tenant, newest first.
create index audit_logs_tenant_idx on audit_logs (tenant_id, created_at desc);

alter table audit_logs enable row level security;
alter table audit_logs force row level security;
create policy audit_logs_tenant on audit_logs
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());
