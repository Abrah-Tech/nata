import type { Alert, AlertRule, AlertRuleUpdate } from "@nata/api-client";

import { formatDate, formatQty } from "@/lib/format";
import { fill, type Dictionary, type Locale } from "@/lib/i18n";

import type { Part } from "./audit-detail";

/** What the editor holds for one rule while it is being changed. */
export type RuleDraft = {
  /** Text in the number input. Empty for a low-stock rule, which has none. */
  days: string;
  channel: AlertRule["channel"];
  is_active: boolean;
};

export function draftFrom(rule: AlertRule): RuleDraft {
  return {
    days: rule.days_before === null ? "" : String(rule.days_before),
    channel: rule.channel,
    is_active: rule.is_active,
  };
}

/** A whole number of days, or null when the input is empty or not a number. */
export function parseDays(value: string): number | null {
  const text = value.trim();

  if (!/^\d+$/.test(text)) return null;

  return Number(text);
}

/**
 * The fields that differ from the stored rule. Empty when nothing changed, so
 * the screen can leave the row alone and the API receives no no-op writes.
 */
export function ruleUpdate(rule: AlertRule, draft: RuleDraft): AlertRuleUpdate {
  const update: AlertRuleUpdate = {};

  // A low-stock rule has no lead time; its shape check would reject one.
  if (rule.type === "expiry") {
    const days = parseDays(draft.days);

    if (days !== null && days !== rule.days_before) {
      update.days_before = days;
    }
  }

  if (draft.channel !== rule.channel) update.channel = draft.channel;
  if (draft.is_active !== rule.is_active) update.is_active = draft.is_active;

  return update;
}

export function isRuleDirty(rule: AlertRule, draft: RuleDraft): boolean {
  return Object.keys(ruleUpdate(rule, draft)).length > 0;
}

export function alertTypeLabel(t: Dictionary, type: Alert["type"]): string {
  return type === "expiry"
    ? t.settings.alerts.rules.expiry
    : t.settings.alerts.rules.lowStock;
}

/** The message status, falling back to the raw value for a status added later. */
export function deliveryLabel(
  t: Dictionary,
  status: Alert["delivery_status"],
): string {
  const deliveries = t.settings.alerts.history.delivery as Record<
    string,
    string | undefined
  >;

  return deliveries[status] ?? status;
}

/**
 * The detail line for one history row: what was flagged, the numbers behind it,
 * and whether the message went out.
 */
export function alertParts(alert: Alert, locale: Locale, t: Dictionary): Part[] {
  const copy = t.settings.alerts.history;
  const parts: Part[] = [];
  const name = alert.material_name ?? alert.product_name;

  if (name) parts.push({ label: copy.subject, value: name });

  if (alert.type === "expiry") {
    if (alert.expires_at) {
      parts.push({
        label: copy.expires,
        value: formatDate(alert.expires_at, locale),
      });
    }

    if (alert.days_left !== null) {
      parts.push({
        label: copy.daysLeft,
        value: fill(copy.days, { n: String(alert.days_left) }),
      });
    }

    if (alert.qty_remaining !== null) {
      parts.push({
        label: copy.remaining,
        value: formatQty(alert.qty_remaining, locale),
      });
    }
  } else {
    if (alert.on_hand !== null) {
      parts.push({
        label: copy.onHand,
        value: formatQty(alert.on_hand, locale),
      });
    }

    if (alert.min_stock !== null) {
      parts.push({
        label: copy.minStock,
        value: formatQty(alert.min_stock, locale),
      });
    }
  }

  parts.push({
    label: copy.deliveryLabel,
    value: deliveryLabel(t, alert.delivery_status),
  });

  return parts;
}
