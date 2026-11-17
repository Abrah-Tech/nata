-- Finished-goods batches (D8).
--
-- Finished goods become a batch ledger exactly like materials: a sale takes
-- from product_batches in FEFO order, and products.stock_qty is only a
-- read-only cache written in the same transaction. Expiry comes from the
-- product's shelf life on the day the run was produced.

alter table products
    add column shelf_life_days int
        check (shelf_life_days is null or shelf_life_days > 0);

comment on column products.stock_qty is
    'Read-only cache of sum(product_batches.qty_remaining), written in the same transaction as the batch change.';
comment on column products.shelf_life_days is
    'Days from production until a batch expires. Null means the product does not expire.';

-- A batch points at the run that produced it, and the composite key proves the
-- run belongs to the same tenant.
alter table production_orders
    add constraint production_orders_id_tenant_unique unique (id, tenant_id);

create table product_batches (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    product_id uuid not null,
    production_order_id uuid,
    produced_on date not null,
    expires_at date,
    qty_received numeric(18,3) not null check (qty_received > 0),
    qty_remaining numeric(18,3) not null
        check (qty_remaining >= 0 and qty_remaining <= qty_received),
    -- Null when the cost cannot be known, for example stock migrated from the
    -- scalar column with no production order behind it. A sale that takes such
    -- a batch reports its cost as incomplete instead of inventing a number.
    unit_cost numeric(18,4) check (unit_cost >= 0),
    created_at timestamptz not null default now(),
    constraint product_batches_product_tenant
        foreign key (product_id, tenant_id)
        references products (id, tenant_id)
        on delete cascade,
    constraint product_batches_order_tenant
        foreign key (production_order_id, tenant_id)
        references production_orders (id, tenant_id)
        on delete restrict
);

-- FEFO reads: earliest expiry first, then the earliest production day. Null
-- expiry sorts last in ascending order, which is exactly the rule.
create index product_batches_fefo_idx
    on product_batches (tenant_id, product_id, expires_at, produced_on)
    where qty_remaining > 0;

alter table product_batches enable row level security;
alter table product_batches force row level security;
create policy product_batches_tenant on product_batches
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

-- One movement trail for both ledgers: exactly one batch reference is set.
-- `production` is finished goods coming in, `sale` is finished goods going
-- out. Waste, expired, and adjustment already existed for materials and apply
-- to a product batch the same way.
alter table stock_movements
    alter column batch_id drop not null,
    add column product_batch_id uuid references product_batches (id) on delete cascade;

alter table stock_movements
    add constraint stock_movements_source_check
        check ((batch_id is null) <> (product_batch_id is null));

alter table stock_movements drop constraint stock_movements_type_check;
alter table stock_movements
    add constraint stock_movements_type_check
        check (type in
            ('purchase', 'production', 'production_use', 'sale',
             'waste', 'expired', 'adjustment'));

alter table stock_movements drop constraint stock_movements_sign;
alter table stock_movements
    add constraint stock_movements_sign check (
        (type in ('purchase', 'production') and qty > 0)
        or (type in ('production_use', 'sale', 'waste', 'expired') and qty < 0)
        or (type = 'adjustment')
    );

create index stock_movements_product_batch_idx
    on stock_movements (tenant_id, product_batch_id, created_at);

-- Existing scalar stock becomes one synthetic batch per product so no stock
-- disappears. The cost is the product's latest production cost; stock with no
-- production order keeps a null cost. History before D8 has no per-unit
-- movement trail, so the trail deliberately starts here.
insert into product_batches
        (tenant_id, product_id, produced_on, expires_at,
         qty_received, qty_remaining, unit_cost)
select p.tenant_id, p.id, current_date, null,
       p.stock_qty, p.stock_qty, latest.unit_cost
from products p
left join (
    select distinct on (product_id) product_id, unit_cost
    from production_orders
    where status = 'done' and unit_cost is not null
    order by product_id, produced_on desc, created_at desc
) latest on latest.product_id = p.id
where p.stock_qty > 0;
