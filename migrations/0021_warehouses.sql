-- Warehouses (D6).
--
-- The roadmap settles this now rather than in phase 4: the column exists before
-- the feature, so P4-01 adds the API and the UI without a large migration. Nothing
-- writes a warehouse_id yet and there is no warehouse endpoint — the schema is
-- the preparation, the feature is later.

create table warehouses (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    name text not null check (name <> ''),
    created_at timestamptz not null default now(),
    unique (tenant_id, name),
    -- The target of the composite foreign keys below, which is what makes a
    -- batch and its warehouse prove they belong to the same tenant.
    unique (id, tenant_id)
);

alter table warehouses enable row level security;
alter table warehouses force row level security;
create policy warehouses_tenant on warehouses
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

comment on table warehouses is
    'A storage location, counted against the plan limit. P4-01 adds the API and the UI.';

-- Null until a tenant puts stock somewhere, so existing batches and movements
-- are untouched. `restrict`, not `set null`: deleting a location must not
-- silently move stock or rewrite history, it has to be reassigned first.
alter table material_batches
    add column warehouse_id uuid,
    add constraint material_batches_warehouse_tenant
        foreign key (warehouse_id, tenant_id)
        references warehouses (id, tenant_id)
        on delete restrict;

alter table product_batches
    add column warehouse_id uuid,
    add constraint product_batches_warehouse_tenant
        foreign key (warehouse_id, tenant_id)
        references warehouses (id, tenant_id)
        on delete restrict;

alter table stock_movements
    add column warehouse_id uuid,
    add constraint stock_movements_warehouse_tenant
        foreign key (warehouse_id, tenant_id)
        references warehouses (id, tenant_id)
        on delete restrict;

comment on column material_batches.warehouse_id is
    'Null until P4-01 assigns stock to a location.';
comment on column product_batches.warehouse_id is
    'Null until P4-01 assigns stock to a location.';
comment on column stock_movements.warehouse_id is
    'Null until P4-01 records where a movement happened.';
