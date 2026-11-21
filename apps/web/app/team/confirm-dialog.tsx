"use client";

import { ApiError, type Invitation, type Member } from "@nata/api-client";
import { useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { FormError } from "@/components/ui/field";
import { api } from "@/lib/api";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { teamError } from "./error-copy";
import { contactOf } from "./roles";

export type ConfirmAction =
  | { kind: "remove"; member: Member }
  | { kind: "suspend"; member: Member }
  | { kind: "activate"; member: Member }
  | { kind: "revoke"; invitation: Invitation };

type Props = {
  action: ConfirmAction;
  onClose: () => void;
  /** Called after the server accepted, so the list can refetch. */
  onDone: (action: ConfirmAction) => void;
};

type Copy = {
  title: string;
  body: string;
  confirm: string;
  /** Activating restores access, so it does not wear the destructive tint. */
  destructive: boolean;
};

function copyOf(t: Dictionary, action: ConfirmAction): Copy {
  switch (action.kind) {
    case "remove":
      return {
        title: t.team.dialog.remove.title,
        body: fill(t.team.dialog.remove.body, { name: action.member.name }),
        confirm: t.team.dialog.remove.confirm,
        destructive: true,
      };
    case "suspend":
      return {
        title: t.team.dialog.suspend.title,
        body: fill(t.team.dialog.suspend.body, { name: action.member.name }),
        confirm: t.team.dialog.suspend.confirm,
        destructive: true,
      };
    case "activate":
      return {
        title: t.team.dialog.activate.title,
        body: fill(t.team.dialog.activate.body, { name: action.member.name }),
        confirm: t.team.dialog.activate.confirm,
        destructive: false,
      };
    case "revoke":
      return {
        title: t.team.dialog.revoke.title,
        body: fill(t.team.dialog.revoke.body, {
          contact: contactOf(action.invitation),
        }),
        confirm: t.team.dialog.revoke.confirm,
        destructive: true,
      };
  }
}

export function ConfirmDialog({ action, onClose, onDone }: Props) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const copy = copyOf(t, action);

  async function confirm() {
    if (busy) return;

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    setBusy(true);
    setError(null);

    try {
      switch (action.kind) {
        case "remove":
          await api.removeMember(token, action.member.user_id);
          break;
        case "suspend":
          await api.changeMemberStatus(token, action.member.user_id, "suspended");
          break;
        case "activate":
          await api.changeMemberStatus(token, action.member.user_id, "active");
          break;
        case "revoke":
          await api.revokeInvitation(token, action.invitation.id);
          break;
      }

      onDone(action);
    } catch (cause) {
      setError(teamError(t, cause));

      if (cause instanceof ApiError && cause.status === 401) onClose();
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open
      title={copy.title}
      closeLabel={t.team.dialog.close}
      onClose={onClose}
    >
      <div className="flex flex-col gap-4">
        <p className="text-sm leading-relaxed text-muted-foreground">
          {copy.body}
        </p>

        {error && <FormError>{error}</FormError>}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onClose}>
            {t.team.dialog.cancel}
          </Button>
          <Button
            variant={copy.destructive ? "destructive" : "default"}
            type="button"
            onClick={confirm}
            aria-disabled={busy ? true : undefined}
            aria-busy={busy ? true : undefined}
          >
            {copy.confirm}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
