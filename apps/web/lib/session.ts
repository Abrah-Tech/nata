const TOKEN_KEY = "nata.token";

export function readToken(): string | null {
  if (typeof window === "undefined") {
    return null;
  }

  return window.localStorage.getItem(TOKEN_KEY);
}

export function writeToken(token: string): void {
  window.localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  window.localStorage.removeItem(TOKEN_KEY);
}

/**
 * An invitation a signed-out visitor was carrying when sent to sign in.
 *
 * `token` is the bearer token to accept with. It is null until sign-in
 * succeeds, and when it is set it is a tenant-selection token rather than a
 * session: the account has no membership yet. Keeping it here instead of in
 * `nata.token` matters, because `/me` rejects a tenant-selection token and
 * `useMe` would sign the visitor out.
 */
export type PendingInvite = { code: string; token: string | null };

const INVITE_KEY = "nata.invite";

export function readPendingInvite(): PendingInvite | null {
  if (typeof window === "undefined") {
    return null;
  }

  const raw = window.sessionStorage.getItem(INVITE_KEY);

  if (!raw) {
    return null;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<PendingInvite>;

    return typeof parsed.code === "string"
      ? { code: parsed.code, token: parsed.token ?? null }
      : null;
  } catch {
    return null;
  }
}

export function writePendingInvite(invite: PendingInvite): void {
  window.sessionStorage.setItem(INVITE_KEY, JSON.stringify(invite));
}

export function clearPendingInvite(): void {
  window.sessionStorage.removeItem(INVITE_KEY);
}
