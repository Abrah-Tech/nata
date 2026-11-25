import type { Role } from "@nata/api-client";

import type { Dictionary } from "@/lib/i18n";

/** Mirrors `Role::rank` in `crates/domain/src/rbac.rs`. */
export const ROLE_RANK: Record<Role, number> = {
  viewer: 0,
  production: 1,
  cashier: 1,
  admin: 2,
  owner: 3,
};

/** Mirrors `Role::can_manage`: strictly below, except owner over owner. */
export function canManage(mine: Role, target: Role): boolean {
  return (
    ROLE_RANK[mine] > ROLE_RANK[target] ||
    (mine === "owner" && target === "owner")
  );
}

/** The roles a member holding `mine` may hand out, highest first. */
export function grantableRoles(mine: Role): Role[] {
  return (Object.keys(ROLE_RANK) as Role[])
    .filter((role) => canManage(mine, role) && role !== "owner")
    .sort((left, right) => ROLE_RANK[right] - ROLE_RANK[left]);
}

export function roleLabel(t: Dictionary, role: Role): string {
  return t.dashboard.roles[role];
}

export function roleTone(role: Role): "brand" | "outline" | "neutral" {
  switch (role) {
    case "owner":
      return "brand";
    case "admin":
      return "outline";
    default:
      return "neutral";
  }
}

export function statusTone(status: string): "brand" | "warning" {
  return status === "active" ? "brand" : "warning";
}

/** The invitee's contact, whichever of the two the invitation carries. */
export function contactOf(source: {
  email: string | null;
  phone: string | null;
}): string {
  return source.email ?? source.phone ?? "";
}
