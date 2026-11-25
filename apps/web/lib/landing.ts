import type { MeResponse } from "@nata/api-client";

/**
 * The first screen a signed-in visitor sees.
 *
 * The overview reads materials, so a role without `material_read` — the
 * cashier — would only meet an error there. It goes straight to the register,
 * which is the screen it actually works from.
 */
export function homePath(me: MeResponse): string {
  return me.role === "cashier" ? "/sales" : "/dashboard";
}
