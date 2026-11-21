"use client";

import {
  ApiError,
  type InvitationCreated,
  type Role,
} from "@nata/api-client";
import { useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { formatTimestampDate } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { teamError } from "./error-copy";
import { roleLabel } from "./roles";

type Props = {
  /** Seat usage from `/subscription`; a pending invitation reserves a seat. */
  seats: { used: number; limit: number | null };
  /** The roles the caller may grant, already filtered by rank. */
  roles: Role[];
  onClose: () => void;
  onCreated: (invitation: InvitationCreated) => void;
};

/**
 * The acceptance code is returned once and only its hash is stored, so a
 * successful create switches to a panel that shows it. Without that the
 * invitation could never reach the invitee.
 */
export function InviteDialog({ seats, roles, onClose, onCreated }: Props) {
  const { t, locale } = useI18n();

  const [contact, setContact] = useState("");
  const [role, setRole] = useState<Role>("production");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<InvitationCreated | null>(null);
  const [copied, setCopied] = useState(false);

  const full = seats.limit !== null && seats.used >= seats.limit;
  const selected = roles.includes(role) ? role : roles[0];

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (sending) return;

    const value = contact.trim();

    if (!value) {
      setError(t.team.errors.contactRequired);
      return;
    }

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    setSending(true);
    setError(null);

    try {
      const invitation = await api.createInvitation(token, {
        contact: value,
        role: selected,
      });

      setCreated(invitation);
      onCreated(invitation);
    } catch (cause) {
      setError(teamError(t, cause));

      if (cause instanceof ApiError && cause.status === 401) onClose();
    } finally {
      setSending(false);
    }
  }

  async function copyCode() {
    if (!created) return;

    try {
      await navigator.clipboard.writeText(created.token);
      setCopied(true);
      return;
    } catch {
      // A plain HTTP origin is not a secure context, so the async clipboard
      // API is missing on the Android WebView. The textarea path still works.
    }

    const area = document.createElement("textarea");
    area.value = created.token;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    document.body.removeChild(area);
    setCopied(true);
  }

  if (created) {
    const name = created.email ?? created.phone ?? "";

    return (
      <Modal
        open
        title={fill(t.team.dialog.invite.createdTitle, { contact: name })}
        closeLabel={t.team.dialog.invite.close}
        onClose={onClose}
      >
        <div className="flex flex-col gap-4">
          <p className="text-sm leading-relaxed text-muted-foreground">
            {fill(t.team.dialog.invite.createdBody, {
              contact: name,
              date: formatTimestampDate(created.expires_at, locale),
            })}
          </p>

          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-semibold text-muted-foreground">
              {t.team.dialog.invite.code}
            </span>
            <code className="rounded-lg border border-border bg-surface-alt px-3 py-2.5 font-mono text-sm break-all">
              {created.token}
            </code>
          </div>

          <div className="flex justify-end gap-2">
            <Button variant="secondary" type="button" onClick={copyCode}>
              {copied ? t.team.dialog.invite.copied : t.team.dialog.invite.copy}
            </Button>
            <Button type="button" onClick={onClose}>
              {t.team.dialog.invite.close}
            </Button>
          </div>
        </div>
      </Modal>
    );
  }

  return (
    <Modal
      open
      title={t.team.dialog.invite.title}
      subtitle={t.team.dialog.invite.subtitle}
      closeLabel={t.team.dialog.invite.close}
      onClose={onClose}
    >
      <form className="flex flex-col gap-4" onSubmit={submit}>
        <Field
          label={t.team.dialog.invite.contact}
          hint={t.team.dialog.invite.contactHint}
          hintId="team-invite-contact-hint"
        >
          <Input
            type="text"
            autoComplete="off"
            inputMode="email"
            aria-describedby="team-invite-contact-hint"
            value={contact}
            onChange={(event) => setContact(event.target.value)}
            required
          />
        </Field>

        <Field
          label={t.team.dialog.invite.role}
          hint={t.team.dialog.invite.roleHint}
          hintId="team-invite-role-hint"
        >
          <Select
            value={selected}
            onChange={(event) => setRole(event.target.value as Role)}
            aria-describedby="team-invite-role-hint"
            required
          >
            {roles.map((option) => (
              <option value={option} key={option}>
                {roleLabel(t, option)}
              </option>
            ))}
          </Select>
        </Field>

        <p className="text-2xs text-ink-3 tabular-nums">
          {seats.limit === null
            ? fill(t.team.dialog.invite.seatsUnlimited, { used: seats.used })
            : fill(t.team.dialog.invite.seats, {
                used: seats.used,
                limit: seats.limit,
              })}
        </p>

        {full && <FormError>{t.team.dialog.invite.full}</FormError>}
        {error && <FormError>{error}</FormError>}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onClose}>
            {t.team.dialog.cancel}
          </Button>
          <Button
            type="submit"
            aria-disabled={sending || full ? true : undefined}
            aria-busy={sending ? true : undefined}
          >
            {sending
              ? t.team.dialog.invite.sending
              : t.team.dialog.invite.submit}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
