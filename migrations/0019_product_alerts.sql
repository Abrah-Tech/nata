-- Finished-goods expiry alerts (D8 follow-up).
--
-- The expiry rules already warn about material batches at H-7 and H-3. Finished
-- goods expire too, so an alert may now point at a product batch instead. An
-- alert still points at exactly one subject: a material batch, a product batch,
-- or a material's low-stock level.

alter table alerts
    alter column material_id drop not null,
    add column product_id uuid references products (id) on delete cascade,
    add column product_batch_id uuid references product_batches (id) on delete cascade;

comment on column alerts.product_id is
    'The product whose batch expired; null for material alerts.';
comment on column alerts.product_batch_id is
    'The finished-goods batch that expired; null for material alerts.';

alter table alerts drop constraint alerts_shape;
alter table alerts
    add constraint alerts_shape check (
        (type = 'expiry' and (
            (material_id is not null and batch_id is not null
             and product_id is null and product_batch_id is null)
         or (product_id is not null and product_batch_id is not null
             and material_id is null and batch_id is null)
        ))
        or (type = 'low_stock' and material_id is not null and batch_id is null
            and product_id is null and product_batch_id is null)
    );

-- The low-stock index must only cover low-stock alerts. It used to exclude
-- expiry alerts by `batch_id is null`, but a product expiry alert also has a
-- null batch_id, and an index on (rule, material_id) would then block a second
-- expiry alert for the same material.
drop index alerts_open_low_stock;
create unique index alerts_open_low_stock on alerts (tenant_id, rule_id, material_id)
    where resolved_at is null and type = 'low_stock';

-- One open alert per rule and product batch. Resolved alerts stay for history
-- but do not block a new one, exactly like material batches.
create unique index alerts_open_product_expiry
    on alerts (tenant_id, rule_id, product_batch_id)
    where resolved_at is null and product_batch_id is not null;
