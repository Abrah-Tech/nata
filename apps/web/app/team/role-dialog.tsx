"use client";

import { ApiError, type Member, type Role } from "@nata/api-client";
import { useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { teamError } from "./error-copy";
import { roleLabel } from "./roles";

type Props = {
  member: Member;
  /** Active owners in the tenant, this member included. */
  ownerCount: number;
  onClose: () => void;
  onSaved: (member: Member) => void;
};

const ROLES: Role[] = ["owner", "admin", "production", "viewer"];

/**
 * Only an owner reaches this dialog: `member_role_change` is not in any other
 * role. The one client-side rule left is the last owner, which the server
 * still enforces inside its transaction.
 */
export function RoleDialog({ member, ownerCount, onClose, onSaved }: Props) {
  const { t } = useI18n();

  const [role, setRole] = useState<Role>(member.role);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const lastOwner = member.role === "owner" && ownerCount <= 1;

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (saving || role === member.role) {
      onClose();
      return;
    }

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    setSaving(true);
    setError(null);

    try {
      onSaved(await api.changeMemberRole(token, member.user_id, role));
    } catch (cause) {
      setError(teamError(t, cause));

      if (cause instanceof ApiError && cause.status === 401) onClose();
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      open
      title={t.team.dialog.role.title}
      subtitle={fill(t.team.dialog.role.subtitle, {
        name: member.name,
        role: roleLabel(t, member.role),
      })}
      closeLabel={t.team.dialog.role.close}
      onClose={onClose}
    >
      <form className="flex flex-col gap-4" onSubmit={submit}>
        <Field
          label={t.team.dialog.role.role}
          hint={t.team.dialog.role.hint}
          hintId="team-role-hint"
        >
          <Select
            value={role}
            onChange={(event) => setRole(event.target.value as Role)}
            aria-describedby="team-role-hint"
            required
          >
            {ROLES.map((option) => (
              <option
                value={option}
                key={option}
                disabled={lastOwner && option !== "owner"}
              >
                {roleLabel(t, option)}
              </option>
            ))}
          </Select>
        </Field>

        {error && <FormError>{error}</FormError>}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onClose}>
            {t.team.dialog.cancel}
          </Button>
          <Button
            type="submit"
            aria-disabled={saving ? true : undefined}
            aria-busy={saving ? true : undefined}
          >
            {saving ? t.team.dialog.role.saving : t.team.dialog.role.submit}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
