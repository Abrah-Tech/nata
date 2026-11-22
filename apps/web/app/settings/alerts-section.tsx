"use client";

import type { Alert, AlertPreview, AlertRule } from "@nata/api-client";
import { ApiError } from "@nata/api-client";
import { useEffect, useState } from "react";

import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api";
import { formatDateTime } from "@/lib/format";
import type { Dictionary, Locale } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import {
  alertParts,
  alertTypeLabel,
  draftFrom,
  isRuleDirty,
  parseDays,
  ruleUpdate,
  type RuleDraft,
} from "./alerts";
import { Intro } from "./intro";

/** The history asks for the newest 50; the server caps a page at 200. */
const FIRST_PAGE = 50;
const LAST_PAGE = 200;

type StatusFilter = "all" | "open" | "resolved";

type AlertsSectionProps = {
  locale: Locale;
  /** A non-owner may read the rules and the history, but not change either. */
  readOnly: boolean;
  t: Dictionary;
};

export function AlertsSection({ locale, readOnly, t }: AlertsSectionProps) {
  const copy = t.settings.alerts;

  const [rules, setRules] = useState<AlertRule[] | null>(null);
  const [drafts, setDrafts] = useState<Record<string, RuleDraft>>({});
  const [alerts, setAlerts] = useState<Alert[] | null>(null);
  const [preview, setPreview] = useState<AlertPreview | null>(null);
  const [copied, setCopied] = useState(false);
  const [failed, setFailed] = useState(false);
  const [status, setStatus] = useState<StatusFilter>("all");
  const [limit, setLimit] = useState(FIRST_PAGE);
  // A retry remounts the fetch without another state change to key on.
  const [attempt, setAttempt] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    const token = readToken();

    if (!token) return;

    let active = true;

    Promise.all([
      api.listAlertRules(token),
      api.listAlerts(token, {
        status: status === "all" ? undefined : status,
        limit,
      }),
      api.previewAlerts(token),
    ])
      .then(([ruleList, alertList, digest]) => {
        if (!active) return;

        setRules(ruleList);
        setDrafts(
          Object.fromEntries(ruleList.map((rule) => [rule.id, draftFrom(rule)])),
        );
        setAlerts(alertList);
        setPreview(digest);
        setFailed(false);
      })
      .catch(() => {
        if (active) setFailed(true);
      });

    return () => {
      active = false;
    };
  }, [status, limit, attempt]);

  useEffect(() => {
    if (!notice) return;

    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    if (!copied) return;

    const timer = window.setTimeout(() => setCopied(false), 2_000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const loading = rules === null && !failed;
  const dirty =
    rules !== null &&
    rules.some((rule) => isRuleDirty(rule, drafts[rule.id] ?? draftFrom(rule)));
  const more = limit < LAST_PAGE && alerts !== null && alerts.length >= limit;

  function retry() {
    setRules(null);
    setAlerts(null);
    setPreview(null);
    setFailed(false);
    setAttempt((value) => value + 1);
  }

  /** Copies the subject and the body together, the way they are sent. */
  async function copyPreview() {
    if (!preview || preview.body.length === 0) return;

    const value = `${preview.subject}\n\n${preview.body}`;

    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      return;
    } catch {
      // A plain HTTP origin is not a secure context, so the async clipboard API
      // is missing on the Android WebView. The textarea path still works.
    }

    const area = document.createElement("textarea");
    area.value = value;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    document.body.removeChild(area);
    setCopied(true);
  }

  function edit(rule: AlertRule, patch: Partial<RuleDraft>) {
    setDrafts((current) => {
      const draft = current[rule.id] ?? draftFrom(rule);

      return { ...current, [rule.id]: { ...draft, ...patch } };
    });
  }

  async function save() {
    const token = readToken();

    if (!token || !rules || saving) return;

    // Checked here so a bad number never becomes a request the server rejects.
    const invalid = rules.some(
      (rule) =>
        rule.type === "expiry" && parseDays(drafts[rule.id].days) === null,
    );

    if (invalid) {
      setError(copy.errors.invalidDays);
      return;
    }

    if (!dirty) return;

    setSaving(true);
    setError(null);

    const next = rules.map((rule) => ({ ...rule }));
    let failure: string | null = null;

    try {
      for (const rule of next) {
        const update = ruleUpdate(rule, drafts[rule.id]);

        if (Object.keys(update).length === 0) continue;

        Object.assign(rule, await api.updateAlertRule(token, rule.id, update));
      }
    } catch (cause) {
      if (cause instanceof ApiError) {
        if (cause.code === "duplicate_rule_days") {
          failure = copy.errors.duplicate;
        } else if (cause.code === "permission_denied") {
          failure = copy.errors.forbidden;
        }
      }

      failure ??= copy.errors.generic;
    } finally {
      setSaving(false);
    }

    // Whatever landed stays on screen: a rule saved before a later failure is
    // not reported as still pending.
    setRules(next);
    setDrafts(
      Object.fromEntries(next.map((rule) => [rule.id, draftFrom(rule)])),
    );

    if (failure) setError(failure);
    else setNotice(copy.saved);
  }

  return (
    <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
      <Intro title={copy.title} body={copy.body} />

      {loading && (
        <LoadingRegion
          label={copy.history.loading}
          className="flex flex-col gap-2"
        >
          <SkeletonList rows={3} />
        </LoadingRegion>
      )}

      {failed && !loading && (
        <div className="flex flex-col gap-3">
          <p className="text-sm text-muted-foreground">{copy.error}</p>
          <Button
            variant="secondary"
            size="sm"
            type="button"
            className="self-start"
            onClick={retry}
          >
            {copy.retry}
          </Button>
        </div>
      )}

      {!loading && !failed && rules && (
        <>
          <ul className="flex flex-col gap-2">
            {rules.map((rule) => (
              <RuleRow
                key={rule.id}
                rule={rule}
                draft={drafts[rule.id] ?? draftFrom(rule)}
                readOnly={readOnly}
                t={t}
                onEdit={edit}
              />
            ))}
          </ul>

          {error && (
            <p
              role="alert"
              className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
            >
              {error}
            </p>
          )}

          {notice && (
            <p
              role="status"
              className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
            >
              {notice}
            </p>
          )}

          {readOnly ? (
            <span className="text-2xs leading-snug text-ink-3">
              {copy.readOnly}
            </span>
          ) : (
            <div className="flex justify-end">
              <Button
                type="button"
                aria-disabled={!dirty || saving}
                aria-busy={saving}
                onClick={() => void save()}
              >
                {saving ? copy.saving : copy.save}
              </Button>
            </div>
          )}

          <h3 className="mt-2 text-sm font-bold">{copy.preview.title}</h3>
          <p className="m-0 text-sm leading-relaxed text-muted-foreground">
            {copy.preview.body}
          </p>

          {preview === null ? (
            <LoadingRegion
              label={copy.preview.loading}
              className="flex flex-col gap-2"
            >
              <SkeletonList rows={2} />
            </LoadingRegion>
          ) : (
            <DigestPreview
              preview={preview}
              copied={copied}
              onCopy={() => void copyPreview()}
              t={t}
            />
          )}

          <h3 className="mt-2 text-sm font-bold">{copy.history.title}</h3>
          <p className="m-0 text-sm leading-relaxed text-muted-foreground">
            {copy.history.body}
          </p>

          <label className="flex items-center gap-2">
            <span className="text-xs font-semibold text-muted-foreground">
              {copy.history.filter}
            </span>
            <Select
              value={status}
              onChange={(event) =>
                setStatus(event.target.value as StatusFilter)
              }
              className="w-36"
            >
              <option value="all">{copy.history.all}</option>
              <option value="open">{copy.history.open}</option>
              <option value="resolved">{copy.history.resolved}</option>
            </Select>
          </label>

          {alerts === null && !failed && (
            <LoadingRegion
              label={copy.history.loading}
              className="flex flex-col gap-2"
            >
              <SkeletonList rows={4} />
            </LoadingRegion>
          )}

          {alerts !== null && alerts.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {copy.history.empty}
            </p>
          )}

          {alerts !== null && alerts.length > 0 && (
            <>
              <ul className="flex flex-col gap-2">
                {alerts.map((alert) => (
                  <AlertRow
                    key={alert.id}
                    alert={alert}
                    locale={locale}
                    t={t}
                  />
                ))}
              </ul>

              {more && (
                <div className="flex justify-end">
                  <Button
                    variant="ghost"
                    size="sm"
                    type="button"
                    onClick={() => setLimit(LAST_PAGE)}
                  >
                    {copy.history.showMore}
                  </Button>
                </div>
              )}
            </>
          )}
        </>
      )}
    </section>
  );
}

function RuleRow({
  rule,
  draft,
  readOnly,
  t,
  onEdit,
}: {
  rule: AlertRule;
  draft: RuleDraft;
  readOnly: boolean;
  t: Dictionary;
  onEdit: (rule: AlertRule, patch: Partial<RuleDraft>) => void;
}) {
  const copy = t.settings.alerts;

  return (
    <li className="flex flex-col gap-3 rounded-lg border border-border bg-surface-alt px-3 py-2.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <strong className="text-sm font-bold">
          {alertTypeLabel(t, rule.type)}
        </strong>

        <div className="flex flex-wrap items-center gap-3">
          <Field label={copy.channel} className="w-36">
            <Select
              value={draft.channel}
              disabled={readOnly}
              onChange={(event) =>
                onEdit(rule, {
                  channel: event.target.value as AlertRule["channel"],
                })
              }
            >
              <option value="whatsapp">{copy.channels.whatsapp}</option>
              <option value="email">{copy.channels.email}</option>
            </Select>
          </Field>

          <label className="flex items-center gap-2">
            <span className="text-xs font-semibold text-muted-foreground">
              {copy.active}
            </span>
            <Switch
              aria-label={copy.active}
              checked={draft.is_active}
              disabled={readOnly}
              onChange={(event) => onEdit(rule, { is_active: event.target.checked })}
            />
          </label>
        </div>
      </div>

      {rule.type === "expiry" ? (
        <Field
          label={copy.daysBefore}
          hint={copy.daysHint}
          hintId={`alerts-days-hint-${rule.id}`}
          className="max-w-[200px]"
        >
          <Input
            id={`alerts-days-${rule.id}`}
            type="number"
            min={0}
            step="1"
            value={draft.days}
            disabled={readOnly}
            aria-describedby={`alerts-days-hint-${rule.id}`}
            onChange={(event) => onEdit(rule, { days: event.target.value })}
          />
        </Field>
      ) : (
        <span className="text-2xs leading-snug text-ink-3">
          {copy.lowStockHint}
        </span>
      )}
    </li>
  );
}

/**
 * The digest itself, exactly as the server composed it.
 *
 * A waiting set of zero is a state with nothing to show, so it gets its own
 * line rather than an empty bubble.
 */
function DigestPreview({
  preview,
  copied,
  onCopy,
  t,
}: {
  preview: AlertPreview;
  copied: boolean;
  onCopy: () => void;
  t: Dictionary;
}) {
  const copy = t.settings.alerts.preview;

  if (preview.count === 0) {
    return <p className="text-sm text-muted-foreground">{copy.empty}</p>;
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-surface-alt px-3 py-2.5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <strong className="text-xs font-semibold text-ink-2">
          {preview.subject}
        </strong>
        <Button variant="ghost" size="sm" type="button" onClick={onCopy}>
          {copied ? copy.copied : copy.copy}
        </Button>
      </div>
      <p className="m-0 whitespace-pre-wrap break-words text-xs leading-relaxed text-ink-2">
        {preview.body}
      </p>
    </div>
  );
}

function AlertRow({
  alert,
  locale,
  t,
}: {
  alert: Alert;
  locale: Locale;
  t: Dictionary;
}) {
  const copy = t.settings.alerts.history;
  const parts = alertParts(alert, locale, t);
  const name = alert.material_name ?? alert.product_name;

  return (
    <li className="flex flex-wrap items-start justify-between gap-3 rounded-lg border border-border bg-surface-alt px-3 py-2.5">
      <div className="flex min-w-0 flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <strong className="text-sm font-bold">
            {name ?? alertTypeLabel(t, alert.type)}
          </strong>
          <Badge variant={alert.status === "open" ? "warning" : "neutral"}>
            {copy[alert.status]}
          </Badge>
        </div>

        {parts.length > 0 && (
          <p className="m-0 text-xs leading-relaxed text-ink-2">
            {parts.map((part, index) => (
              <span key={part.label}>
                {index > 0 && <span aria-hidden="true"> · </span>}
                {part.label}{" "}
                <strong className="font-semibold text-ink tabular-nums">
                  {part.value}
                </strong>
              </span>
            ))}
          </p>
        )}
      </div>

      <span className="text-2xs text-ink-3">
        {formatDateTime(alert.created_at, locale)}
      </span>
    </li>
  );
}
