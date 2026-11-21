"use client";

import { ApiError } from "@nata/api-client";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";

import { LoadingRegion, SkeletonForm } from "@/components/skeletons";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { useI18n, type Dictionary } from "@/lib/i18n";
import {
  clearPendingInvite,
  clearToken,
  readPendingInvite,
  readToken,
  writePendingInvite,
  writeToken,
} from "@/lib/session";
import { clearMeCache } from "@/lib/use-me";

import { LocaleSwitch } from "../locale-switch";

/** Maps an accept failure onto copy. Unknown codes fall back to the generic line. */
function joinError(t: Dictionary, cause: unknown): string {
  if (cause instanceof ApiError) {
    switch (cause.code) {
      case "invitation_not_found":
        return t.join.errors.invitationNotFound;
      case "invitation_not_pending":
        return t.join.errors.invitationNotPending;
      case "invitation_expired":
        return t.join.errors.invitationExpired;
      case "invitation_contact_mismatch":
        return t.join.errors.invitationContactMismatch;
      case "already_a_member":
        return t.join.errors.alreadyAMember;
      case "invalid_token":
        return t.join.errors.invalidToken;
    }
  }

  return t.join.errors.generic;
}

function JoinFallback() {
  const { t } = useI18n();

  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-6 bg-bg px-4 py-8">
      <LoadingRegion label={t.join.loading} className="w-full max-w-[392px]">
        <SkeletonForm fields={2} />
      </LoadingRegion>
    </main>
  );
}

function JoinScreen() {
  const { t } = useI18n();
  const router = useRouter();
  const params = useSearchParams();

  const [authToken, setAuthToken] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const codeRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const fromLink = (params.get("token") ?? "").trim();
    const pending = readPendingInvite();
    const session = readToken();
    const token = session ?? pending?.token ?? null;

    // The code travels in the link when there is one, and in session storage
    // when this visit was sent to sign in first. Either is accepted here.
    const value = fromLink || pending?.code || "";

    setAuthToken(token);
    setCode(value);

    if (!token && value) {
      // Sign-in issues the bearer token that acceptance needs, so the code is
      // parked and the login screen hands it back afterwards.
      writePendingInvite({ code: value, token: null });
      router.replace("/login");
    }
  }, [params, router]);

  async function accept(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    // `aria-disabled` keeps the button focusable, so a repeat submission has to
    // be blocked here as well as in the styling.
    if (busy) return;

    const value = code.trim();

    if (!value) {
      setError(t.join.errors.codeRequired);
      codeRef.current?.focus();
      return;
    }

    if (!authToken) {
      writePendingInvite({ code: value, token: null });
      router.replace("/login");
      return;
    }

    setBusy(true);
    setError(null);

    try {
      const result = await api.acceptInvitation(authToken, { token: value });

      // The response is a session for the joined tenant, so the old token (a
      // session elsewhere, or a tenant-selection token) is replaced outright.
      clearPendingInvite();
      clearMeCache();
      writeToken(result.access_token);
      router.replace("/dashboard");
    } catch (cause) {
      if (cause instanceof ApiError && cause.status === 401) {
        // The bearer token is no longer usable. Sign-in issues a fresh one, and
        // the code is kept so the visitor does not have to find the link again.
        clearToken();
        writePendingInvite({ code: value, token: null });
        router.replace("/login");
        return;
      }

      setError(joinError(t, cause));
    } finally {
      setBusy(false);
    }
  }

  const signOut = () => {
    clearToken();
    clearPendingInvite();
    clearMeCache();
    router.replace("/login");
  };

  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-6 bg-bg px-4 py-8">
      <div className="flex w-full max-w-[392px] flex-col gap-4 rounded-2xl border border-border bg-surface p-[26px]">
        <header className="flex flex-col gap-1.5">
          <p className="m-0 text-2xs font-bold tracking-[0.12em] text-ink-3">
            {t.common.appName}
          </p>
          <h1 className="text-2xl leading-[1.25] font-bold tracking-[-0.01em]">
            {authToken ? t.join.title : t.join.signedOutTitle}
          </h1>
          <p className="m-0 text-xs leading-[1.55] text-ink-3">
            {authToken ? t.join.subtitle : t.join.signedOutBody}
          </p>
        </header>

        {authToken ? (
          <form className="flex flex-col gap-4" onSubmit={accept}>
            <Field
              label={t.join.code}
              hint={t.join.codeHint}
              hintId="join-code-hint"
            >
              <Input
                ref={codeRef}
                type="text"
                autoComplete="off"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                inputMode="text"
                aria-describedby="join-code-hint"
                className="font-mono"
                value={code}
                onChange={(event) => setCode(event.target.value)}
                required
              />
            </Field>

            {error && <FormError>{error}</FormError>}

            <div className="flex flex-col gap-2">
              <Button
                type="submit"
                className="w-full"
                aria-disabled={busy ? true : undefined}
                aria-busy={busy ? true : undefined}
              >
                {busy ? t.join.accepting : t.join.submit}
              </Button>

              <p className="text-2xs leading-snug text-ink-3">
                {t.join.wrongContact}
              </p>

              <Button
                type="button"
                variant="ghost"
                className="w-full"
                onClick={signOut}
              >
                {t.join.signOut}
              </Button>
            </div>
          </form>
        ) : (
          <div className="flex flex-col gap-3">
            <Button
              type="button"
              className="w-full"
              onClick={() => router.replace("/login")}
            >
              {t.join.signIn}
            </Button>
          </div>
        )}
      </div>

      <footer className="flex w-full max-w-[392px] items-center justify-between gap-3.5 text-2xs text-ink-3">
        <span>{t.common.tagline}</span>
        <LocaleSwitch />
      </footer>
    </main>
  );
}

/**
 * The invitation code arrives in the query string, and `useSearchParams` needs a
 * boundary during the static prerender. The fallback is a skeleton, never a
 * spinner, so the first paint already has the shape of the screen.
 */
export default function JoinPage() {
  return (
    <Suspense fallback={<JoinFallback />}>
      <JoinScreen />
    </Suspense>
  );
}
