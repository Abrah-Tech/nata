import { ApiError } from "@nata/api-client";

import type { Dictionary } from "@/lib/i18n";

/**
 * Maps a member or invitation failure to its sentence. The server owns the
 * rule; this only picks the words, so an unmapped code still says something
 * usable instead of surfacing a raw identifier.
 */
export function teamError(t: Dictionary, cause: unknown): string {
  if (!(cause instanceof ApiError)) return t.team.errors.generic;

  switch (cause.code) {
    case "contact_required":
      return t.team.errors.contactRequired;
    case "invalid_email":
      return t.team.errors.invalidEmail;
    case "invalid_phone":
      return t.team.errors.invalidPhone;
    case "last_owner":
      return t.team.errors.lastOwner;
    case "role_outranks_caller":
      return t.team.errors.roleOutranks;
    case "role_not_invitable":
      return t.team.errors.roleNotInvitable;
    case "already_a_member":
      return t.team.errors.alreadyMember;
    case "invitation_exists":
      return t.team.errors.invitationExists;
    case "member_not_found":
      return t.team.errors.memberNotFound;
    case "invitation_not_found":
      return t.team.errors.invitationNotFound;
    case "invitation_not_pending":
      return t.team.errors.invitationNotPending;
    case "invitation_expired":
      return t.team.errors.invitationExpired;
    case "invitation_contact_mismatch":
      return t.team.errors.invitationContactMismatch;
    case "plan_limit_reached":
      return t.team.errors.planLimit;
    case "permission_denied":
      return t.team.errors.forbidden;
    default:
      return t.team.errors.generic;
  }
}
