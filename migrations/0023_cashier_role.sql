-- The register desk. A cashier sells and reads the sales document, so it joins
-- the roles an invitation may carry.
--
-- It holds no read on materials, recipes, production, or reports: those are the
-- paths that expose cost and margin. The permission matrix lives in
-- `crates/domain/src/rbac.rs`; this constraint only keeps stored rows honest.

alter table memberships drop constraint memberships_role_check;
alter table memberships
    add constraint memberships_role_check
        check (role in ('owner', 'admin', 'production', 'viewer', 'cashier'));

alter table invitations drop constraint invitations_role_check;
alter table invitations
    add constraint invitations_role_check
        check (role in ('admin', 'production', 'viewer', 'cashier'));
