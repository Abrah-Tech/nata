-- What one hour of production overhead costs this tenant.
--
-- Recipes carry labor_minutes for one batch, and the same minutes price both
-- labor and overhead, so a run's duration is read one way. Zero means the
-- tenant has not set a rate yet, and orders keep snapshotting material and
-- labor only.

alter table tenants
    add column overhead_rate_per_hour numeric(18,2) not null default 0
    check (overhead_rate_per_hour >= 0);
