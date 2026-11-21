import { ApiError } from "@nata/api-client";

import type { Dictionary } from "@/lib/i18n";

/** Maps a finished-goods correction error code to copy the user can act on. */
export function batchError(t: Dictionary, cause: unknown): string {
  if (!(cause instanceof ApiError)) {
    return t.batches.errors.generic;
  }

  switch (cause.code) {
    case "invalid_quantity":
      return t.batches.errors.invalidSign;
    case "insufficient_batch_quantity":
      return t.batches.errors.insufficient;
    case "quantity_exceeds_received":
      return t.batches.errors.exceedsReceived;
    case "batch_not_found":
      return t.batches.errors.notFound;
    case "permission_denied":
      return t.batches.errors.forbidden;
    default:
      return t.batches.errors.generic;
  }
}
