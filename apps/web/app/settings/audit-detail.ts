import type { AuditEntry, Role } from "@nata/api-client";

import { formatDate, formatMoney } from "@/lib/format";
import { fill, type Dictionary, type Locale } from "@/lib/i18n";

import { roleLabel } from "../team/roles";
import { methodName, planName } from "./labels";

const ROLES: Role[] = ["owner", "admin", "production", "viewer"];

/** `formatDate` parses `YYYY-MM-DD` by splitting on `-`, so a timestamp must not reach it. */
const DATE_ONLY = /^\d{4}-\d{2}-\d{2}$/;

export type Part = { label: string; value: string };

export function actionLabel(t: Dictionary, action: string): string {
  const actions = t.settings.audit.actions as Record<string, string | undefined>;

  return actions[action] ?? action;
}

export function entityLabel(t: Dictionary, entity: string): string {
  const entities = t.settings.audit.entity as Record<string, string | undefined>;

  return entities[entity] ?? entity;
}

/**
 * Turns an entry's metadata into the detail line.
 *
 * Metadata is schemaless by design, so every key the API writes gets a label
 * and a value formatter here. An unrecognized key, and a value that is neither
 * a primitive nor a `{ from, to }` change, are dropped rather than shown as
 * raw JSON: the ten actions the API records only produce those two shapes.
 */
export function detailParts(
  entry: AuditEntry,
  locale: Locale,
  t: Dictionary,
): Part[] {
  const fields = t.settings.audit.fields as Record<string, string | undefined>;
  const changes = t.settings.audit.changes as Record<string, string | undefined>;
  const parts: Part[] = [];

  for (const [key, value] of Object.entries(entry.metadata)) {
    // A role or status change stores a bare `{ from, to }` rather than a named
    // field, so its label comes from the action. The pair is read once, at
    // `from`; the `to` half is skipped on its own pass.
    if (key === "from" || key === "to") {
      if (key === "to") continue;

      const label = changes[entry.action];

      if (label) {
        parts.push({
          label,
          value: `${scalar("from", value, locale, t)} → ${scalar("to", entry.metadata.to, locale, t)}`,
        });
      }

      continue;
    }

    const label = fields[key];

    if (value === null || typeof value !== "object") {
      if (label) {
        parts.push({ label, value: scalar(key, value, locale, t) });
      }

      continue;
    }

    const change = value as Record<string, unknown>;

    if (label && ("from" in change || "to" in change)) {
      parts.push({
        label,
        value: `${scalar(key, change.from, locale, t)} → ${scalar(key, change.to, locale, t)}`,
      });
    }
  }

  return parts;
}

function scalar(
  key: string,
  value: unknown,
  locale: Locale,
  t: Dictionary,
): string {
  if (value === null || value === undefined) return "";

  if (key === "amount" || key === "labor_rate_per_hour") {
    const numeric = Number(value);

    return Number.isFinite(numeric) ? formatMoney(numeric, locale) : String(value);
  }

  if (key === "period_start" || key === "period_end") {
    const text = String(value);

    return DATE_ONLY.test(text) ? formatDate(text, locale) : text;
  }

  if (key === "usage_window_days") {
    return fill(t.settings.audit.days, { n: String(value) });
  }

  if (key === "plan") return planName(t, String(value));
  if (key === "method") return methodName(t, String(value));

  // A role appears on its own, or inside the `{ from, to }` of a change.
  if (key === "role" || key === "from" || key === "to") {
    const text = String(value);

    return isRole(text) ? roleLabel(t, text) : text;
  }

  return String(value);
}

function isRole(value: string): value is Role {
  return (ROLES as string[]).includes(value);
}
