"use client";

import type {
  Invitation,
  InvitationCreated,
  Member,
  Subscription,
} from "@nata/api-client";
import { Plus } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import { StatusMessage } from "@/components/status";
import { LoadingRegion, SkeletonList, SkeletonTable } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { formatTimestampDate } from "@/lib/format";
import { fill, useI18n, type Dictionary, type Locale } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";
import { cn } from "@/lib/utils";

import { AppShell } from "../app-shell";
import { ConfirmDialog, type ConfirmAction } from "./confirm-dialog";
import { InviteDialog } from "./invite-dialog";
import { RoleDialog } from "./role-dialog";
import {
  canManage,
  contactOf,
  grantableRoles,
  roleLabel,
  roleTone,
  statusTone,
} from "./roles";

type Load =
  | { status: "loading" }
  | { status: "ready"; members: Member[]; invitations: Invitation[] }
  | { status: "error" };

export default function TeamPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [plan, setPlan] = useState<Subscription | null>(null);
  const [inviteOpen, setInviteOpen] = useState(false);
  const [roleTarget, setRoleTarget] = useState<Member | null>(null);
  const [confirm, setConfirm] = useState<ConfirmAction | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    try {
      // A failure to read the plan must not take the team down with it: the
      // seat count is a hint, and the server still enforces the limit.
      const [members, invitations, subscription] = await Promise.all([
        api.listMembers(token),
        api.listInvitations(token),
        api.getSubscription(token).catch(() => null),
      ]);

      setLoad({ status: "ready", members, invitations });

      if (subscription) setPlan(subscription);
    } catch {
      // A refetch after a successful change already happened, so the list
      // keeps what it has rather than blanking over a stale read.
      setLoad((current) =>
        current.status === "ready" ? current : { status: "error" },
      );
    }
  }, []);

  useEffect(() => {
    if (session.status === "ready") {
      void fetchData();
    }
  }, [session.status, fetchData]);

  useEffect(() => {
    if (!notice) return;

    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const me = session.status === "ready" ? session.me : null;
  const permissions = me?.permissions ?? [];
  const myRole = me?.role ?? null;

  const canInvite = permissions.includes("member_invite");
  const canChangeRoles = permissions.includes("member_role_change");
  const canManageMembers = permissions.includes("member_manage");

  const members = load.status === "ready" ? load.members : [];
  const invitations = load.status === "ready" ? load.invitations : [];
  const ownerCount = members.filter(
    (member) => member.role === "owner" && member.status === "active",
  ).length;

  function onRoleSaved(member: Member) {
    setRoleTarget(null);
    setNotice(
      fill(t.team.result.roleChanged, {
        name: member.name,
        role: roleLabel(t, member.role),
      }),
    );
    void fetchData();
    void reloadSession();
  }

  function onInviteCreated(invitation: InvitationCreated) {
    setNotice(
      fill(t.team.result.invited, {
        contact: contactOf(invitation),
      }),
    );
    void fetchData();
  }

  function onConfirmDone(action: ConfirmAction) {
    setConfirm(null);

    const message = confirmMessage(t, action);
    setNotice(message);
    void fetchData();
    void reloadSession();
  }

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="team">
          <div className="flex max-w-[1200px] flex-col gap-4">
            <LoadingRegion label={t.team.loading}>
              <SkeletonTable rows={5} columns={4} />
            </LoadingRegion>
          </div>
        </AppShell>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[1200px] p-6">
          <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
            <p className="text-sm text-muted-foreground">{t.dashboard.error}</p>
            <Button
              variant="secondary"
              size="sm"
              className="self-start"
              type="button"
              onClick={() => void reloadSession()}
            >
              {t.dashboard.retry}
            </Button>
          </div>
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="team" onSignOut={signOut}>
          <div className="flex max-w-[1200px] flex-col gap-4">
            <header className="flex flex-col items-start gap-4 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.team.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  {fill(t.team.subtitle, {
                    members: members.length,
                    invitations: invitations.length,
                  })}
                </p>
              </div>
              {canInvite && (
                <Button type="button" onClick={() => setInviteOpen(true)}>
                  <Plus size={15} />
                  {t.team.invite}
                </Button>
              )}
            </header>

            <StatusMessage
              message={notice}
              className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
            />

            {load.status === "loading" && (
              <LoadingRegion label={t.team.loading}>
                <div className="flex flex-col gap-4">
                  <div className="rounded-xl border border-border bg-card p-5">
                    <SkeletonTable rows={5} columns={4} />
                  </div>
                  <SkeletonList rows={1} />
                </div>
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">{t.team.error}</p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.team.retry}
                </Button>
              </div>
            )}

            {load.status === "ready" && (
              <>
                <section className="overflow-hidden rounded-xl border border-border bg-card">
                  <div className="border-b border-border px-4 py-3.5 sm:px-5">
                    <h2 className="text-base font-bold">{t.team.members.title}</h2>
                  </div>

                  <div
                    role="table"
                    aria-label={t.team.members.title}
                    className="@container"
                  >
                    <div
                      role="row"
                      className="border-b border-border bg-surface-alt px-3.5 py-2 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[878px]:hidden"
                    >
                      <span role="columnheader">
                        {t.team.members.column.member}
                      </span>
                    </div>

                    <div
                      role="row"
                      className="hidden items-center gap-3 bg-surface-alt px-4 py-2.5 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[878px]:grid @min-[878px]:grid-cols-[minmax(0,1fr)_120px_104px_110px_auto]"
                    >
                      <span role="columnheader">
                        {t.team.members.column.member}
                      </span>
                      <span role="columnheader">
                        {t.team.members.column.role}
                      </span>
                      <span role="columnheader">
                        {t.team.members.column.status}
                      </span>
                      <span role="columnheader">
                        {t.team.members.column.joined}
                      </span>
                      <span role="columnheader" className="text-right">
                        {t.team.members.column.actions}
                      </span>
                    </div>

                    {members.length === 0 ? (
                      <div className="flex flex-col items-center gap-2 px-5 py-11 text-center">
                        <h3 className="text-lg font-bold">
                          {t.team.members.empty.title}
                        </h3>
                        <p className="text-sm text-muted-foreground">
                          {t.team.members.empty.body}
                        </p>
                      </div>
                    ) : (
                      members.map((member) => {
                        const isYou = member.user_id === me?.user.id;
                        const manageable =
                          !isYou &&
                          canManageMembers &&
                          myRole !== null &&
                          canManage(myRole, member.role);

                        return (
                          <div
                            role="row"
                            key={member.user_id}
                            className="flex flex-col gap-2 border-b border-border px-3.5 py-3 text-sm last:border-b-0 @min-[878px]:grid @min-[878px]:grid-cols-[minmax(0,1fr)_120px_104px_110px_auto] @min-[878px]:items-center @min-[878px]:gap-3 @min-[878px]:px-4"
                          >
                            <div
                              role="cell"
                              className="flex min-w-0 flex-col gap-1"
                            >
                              <span className="flex flex-wrap items-center gap-2">
                                <span className="text-sm font-semibold text-ink">
                                  {member.name}
                                </span>
                                {isYou && (
                                  <Badge variant="neutral">
                                    {t.team.members.you}
                                  </Badge>
                                )}
                              </span>
                              <span className="truncate text-2xs text-ink-3">
                                {contactOf(member)}
                              </span>
                              <span className="flex flex-wrap gap-1.5 @min-[878px]:hidden">
                                <Badge variant={roleTone(member.role)}>
                                  {roleLabel(t, member.role)}
                                </Badge>
                                <Badge variant={statusTone(member.status)}>
                                  {memberStatusLabel(t, member.status)}
                                </Badge>
                              </span>
                            </div>

                            <div
                              role="cell"
                              className="hidden @min-[878px]:block"
                            >
                              <Badge variant={roleTone(member.role)}>
                                {roleLabel(t, member.role)}
                              </Badge>
                            </div>

                            <div
                              role="cell"
                              className="hidden @min-[878px]:block"
                            >
                              <Badge variant={statusTone(member.status)}>
                                {memberStatusLabel(t, member.status)}
                              </Badge>
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-2 tabular-nums @min-[878px]:block"
                            >
                              {formatTimestampDate(member.joined_at, locale)}
                            </div>

                            <div
                              role="cell"
                              className="flex flex-wrap gap-1.5 @min-[878px]:justify-end"
                            >
                              {manageable && canChangeRoles && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  type="button"
                                  onClick={() => setRoleTarget(member)}
                                  aria-label={fill(
                                    t.team.members.changeRoleNamed,
                                    { name: member.name },
                                  )}
                                >
                                  {t.team.members.changeRole}
                                </Button>
                              )}
                              {manageable && member.status === "active" && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  type="button"
                                  onClick={() =>
                                    setConfirm({
                                      kind: "suspend",
                                      member,
                                    })
                                  }
                                  aria-label={fill(
                                    t.team.members.suspendNamed,
                                    { name: member.name },
                                  )}
                                >
                                  {t.team.members.suspend}
                                </Button>
                              )}
                              {manageable && member.status !== "active" && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  type="button"
                                  onClick={() =>
                                    setConfirm({
                                      kind: "activate",
                                      member,
                                    })
                                  }
                                  aria-label={fill(
                                    t.team.members.activateNamed,
                                    { name: member.name },
                                  )}
                                >
                                  {t.team.members.activate}
                                </Button>
                              )}
                              {manageable && (
                                <Button
                                  variant="destructive"
                                  size="sm"
                                  type="button"
                                  onClick={() =>
                                    setConfirm({ kind: "remove", member })
                                  }
                                  aria-label={fill(
                                    t.team.members.removeNamed,
                                    { name: member.name },
                                  )}
                                >
                                  {t.team.members.remove}
                                </Button>
                              )}
                            </div>
                          </div>
                        );
                      })
                    )}
                  </div>
                </section>

                <section className="overflow-hidden rounded-xl border border-border bg-card">
                  <div className="border-b border-border px-4 py-3.5 sm:px-5">
                    <h2 className="text-base font-bold">
                      {t.team.invitations.title}
                    </h2>
                  </div>

                  {invitations.length === 0 ? (
                    <p className="px-4 py-6 text-sm text-muted-foreground sm:px-5">
                      {t.team.invitations.empty}
                    </p>
                  ) : (
                    <div
                      role="table"
                      aria-label={t.team.invitations.title}
                      className="@container"
                    >
                      <div
                        role="row"
                        className="border-b border-border bg-surface-alt px-3.5 py-2 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[640px]:hidden"
                      >
                        <span role="columnheader">
                          {t.team.invitations.column.contact}
                        </span>
                      </div>

                      <div
                        role="row"
                        className="hidden items-center gap-3 bg-surface-alt px-4 py-2.5 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[640px]:grid @min-[640px]:grid-cols-[minmax(0,1fr)_120px_120px_auto]"
                      >
                        <span role="columnheader">
                          {t.team.invitations.column.contact}
                        </span>
                        <span role="columnheader">
                          {t.team.invitations.column.role}
                        </span>
                        <span role="columnheader">
                          {t.team.invitations.column.expires}
                        </span>
                        <span role="columnheader" className="text-right">
                          {t.team.invitations.column.actions}
                        </span>
                      </div>

                      {invitations.map((invitation) => {
                        const contact = contactOf(invitation);

                        return (
                          <div
                            role="row"
                            key={invitation.id}
                            className="flex flex-col gap-2 border-b border-border px-3.5 py-3 text-sm last:border-b-0 @min-[640px]:grid @min-[640px]:grid-cols-[minmax(0,1fr)_120px_120px_auto] @min-[640px]:items-center @min-[640px]:gap-3 @min-[640px]:px-4"
                          >
                            <div
                              role="cell"
                              className="flex min-w-0 flex-col gap-1"
                            >
                              <span className="truncate text-sm font-semibold text-ink">
                                {contact}
                              </span>
                              <span className="flex flex-wrap gap-1.5 text-2xs text-ink-3 @min-[640px]:hidden">
                                <Badge variant={roleTone(invitation.role)}>
                                  {roleLabel(t, invitation.role)}
                                </Badge>
                                <span>
                                  {fill(t.team.invitations.expiresNamed, {
                                    date: formatTimestampDate(
                                      invitation.expires_at,
                                      locale,
                                    ),
                                  })}
                                </span>
                              </span>
                            </div>

                            <div
                              role="cell"
                              className="hidden @min-[640px]:block"
                            >
                              <Badge variant={roleTone(invitation.role)}>
                                {roleLabel(t, invitation.role)}
                              </Badge>
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-2 tabular-nums @min-[640px]:block"
                            >
                              {formatTimestampDate(invitation.expires_at, locale)}
                            </div>

                            <div
                              role="cell"
                              className="flex flex-wrap gap-1.5 @min-[640px]:justify-end"
                            >
                              {canInvite && (
                                <Button
                                  variant="destructive"
                                  size="sm"
                                  type="button"
                                  onClick={() =>
                                    setConfirm({
                                      kind: "revoke",
                                      invitation,
                                    })
                                  }
                                  aria-label={fill(
                                    t.team.invitations.revokeNamed,
                                    { contact },
                                  )}
                                >
                                  {t.team.invitations.revoke}
                                </Button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </section>

                {plan ? (
                  <section className="flex max-w-[520px] flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
                    <div>
                      <h2 className="text-base font-bold">
                        {t.team.plan.title}
                      </h2>
                      <p className="mt-1.5 max-w-[60ch] text-sm leading-relaxed text-muted-foreground">
                        {t.team.plan.body}
                      </p>
                    </div>

                    <div className="flex flex-wrap items-center gap-2.5">
                      <Badge variant="brand" className="px-2.5 py-1 text-xs">
                        {planName(t, plan.plan)}
                      </Badge>
                      <Badge
                        variant={accessTone(plan.access)}
                        className="px-2.5 py-1 text-xs"
                      >
                        {t.team.plan.access[plan.access]}
                      </Badge>
                    </div>

                    {planDateLine(t, locale, plan) && (
                      <p className="text-xs text-ink-3">
                        {planDateLine(t, locale, plan)}
                      </p>
                    )}

                    {plan.access === "read_only" && (
                      <p className="rounded-lg bg-warning-soft px-3 py-2.5 text-xs text-warning">
                        {t.team.plan.readOnlyHint}
                      </p>
                    )}

                    <SeatUsage
                      used={plan.usage.users}
                      limit={plan.limits.users}
                      label={t.team.plan.seatsLabel}
                      of={t.team.plan.seats}
                      unlimited={t.team.plan.unlimited}
                    />

                    <div>
                      <Button variant="ghost" size="sm" asChild>
                        <Link href="/settings">{t.team.plan.billing}</Link>
                      </Button>
                    </div>
                  </section>
                ) : (
                  <SkeletonList rows={1} />
                )}
              </>
            )}
          </div>

          {inviteOpen && myRole !== null && (
            <InviteDialog
              seats={{
                used: plan?.usage.users ?? 0,
                limit: plan?.limits.users ?? null,
              }}
              roles={grantableRoles(myRole)}
              onClose={() => setInviteOpen(false)}
              onCreated={onInviteCreated}
            />
          )}

          {roleTarget && (
            <RoleDialog
              member={roleTarget}
              ownerCount={ownerCount}
              onClose={() => setRoleTarget(null)}
              onSaved={onRoleSaved}
            />
          )}

          {confirm && (
            <ConfirmDialog
              action={confirm}
              onClose={() => setConfirm(null)}
              onDone={onConfirmDone}
            />
          )}
        </AppShell>
      )}
    </>
  );
}

function memberStatusLabel(t: Dictionary, status: string): string {
  const labels = t.team.members.status as Record<string, string | undefined>;

  return labels[status] ?? status;
}

function confirmMessage(t: Dictionary, action: ConfirmAction): string {
  switch (action.kind) {
    case "remove":
      return fill(t.team.result.removed, { name: action.member.name });
    case "suspend":
      return fill(t.team.result.suspended, { name: action.member.name });
    case "activate":
      return fill(t.team.result.activated, { name: action.member.name });
    case "revoke":
      return t.team.result.invitationRevoked;
  }
}

function planName(t: Dictionary, plan: string): string {
  switch (plan) {
    case "trial":
      return t.settings.plan.trial;
    case "starter":
      return t.settings.plan.starter;
    case "pro":
      return t.settings.plan.pro;
    case "business":
      return t.settings.plan.business;
    default:
      return plan;
  }
}

function accessTone(access: Subscription["access"]): "brand" | "warning" | "danger" {
  if (access === "active") return "brand";
  if (access === "grace") return "warning";

  return "danger";
}

function isPast(value: string): boolean {
  return new Date(value).getTime() < Date.now();
}

function planDateLine(
  t: Dictionary,
  locale: Locale,
  plan: Subscription,
): string | null {
  if (plan.paid_until) {
    return fill(t.team.plan.paidUntil, {
      date: formatTimestampDate(plan.paid_until, locale),
    });
  }

  if (plan.trial_ends_at) {
    return isPast(plan.trial_ends_at)
      ? fill(t.team.plan.trialEnded, {
          date: formatTimestampDate(plan.trial_ends_at, locale),
        })
      : fill(t.team.plan.trialEnds, {
          date: formatTimestampDate(plan.trial_ends_at, locale),
        });
  }

  return null;
}

function SeatUsage({
  used,
  limit,
  label,
  of,
  unlimited,
}: {
  used: number;
  limit: number | null;
  label: string;
  of: string;
  unlimited: string;
}) {
  const full = limit !== null && used >= limit;

  return (
    <div className="grid grid-cols-[110px_minmax(0,1fr)] items-center gap-2.5">
      <span className="text-xs text-ink-2">{label}</span>
      <span className="flex items-center gap-2.5">
        <span className="min-w-[72px] text-xs tabular-nums">
          {limit === null ? fill(unlimited, { used }) : fill(of, { used, limit })}
        </span>
        {limit !== null && (
          <span
            className="block h-1.5 flex-1 overflow-hidden rounded-full bg-surface-alt"
            aria-hidden="true"
          >
            <span
              className={cn("block h-full", full ? "bg-warning" : "bg-brand")}
              style={{
                width: `${Math.min(
                  100,
                  Math.round((used / Math.max(limit, 1)) * 100),
                )}%`,
              }}
            />
          </span>
        )}
      </span>
    </div>
  );
}
