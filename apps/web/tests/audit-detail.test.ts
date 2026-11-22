import { describe, expect, test } from "bun:test";

import { detailParts } from "../app/settings/audit-detail";
import { id } from "../lib/i18n/id";

const locale = "id";

/** `Intl` separates the rupiah prefix from the amount with a non-breaking space. */
const nbsp = String.fromCharCode(0xa0);
const money = (value: string) => `Rp${nbsp}${value}`;

/** A minimal entry: the detail line reads only the action and the metadata. */
function entry(
  metadata: Record<string, unknown>,
  action = "member.role_changed",
) {
  return {
    id: "00000000-0000-0000-0000-000000000000",
    actor_user_id: null,
    actor_name: null,
    action,
    entity_type: "member",
    entity_id: null,
    metadata,
    created_at: "2026-11-22T03:00:00Z",
  };
}

describe("detailParts", () => {
  test("labels a bare role change from the action", () => {
    expect(detailParts(entry({ from: "admin", to: "production" }), locale, id))
      .toEqual([{ label: "Peran", value: "Admin → Produksi" }]);
  });

  test("labels a status change from the action", () => {
    expect(
      detailParts(
        entry({ from: "active", to: "suspended" }, "member.status_changed"),
        locale,
        id,
      ),
    ).toEqual([{ label: "Status", value: "active → suspended" }]);
  });

  test("formats a named change as old to new", () => {
    expect(
      detailParts(
        entry(
          {
            labor_rate_per_hour: { from: "60000", to: "75000" },
            usage_window_days: { from: 30, to: 45 },
          },
          "settings.updated",
        ),
        locale,
        id,
      ),
    ).toEqual([
      {
        label: "Upah per jam",
        value: `${money("60.000")} → ${money("75.000")}`,
      },
      { label: "Jendela pemakaian", value: "30 hari → 45 hari" },
    ]);
  });

  test("formats a payment from its amount, method, plan and period", () => {
    expect(
      detailParts(
        entry(
          {
            amount: "199000.0000",
            method: "transfer",
            plan: "starter",
            period_start: "2026-11-01",
            period_end: "2026-11-30",
          },
          "billing.payment_created",
        ),
        locale,
        id,
      ),
    ).toEqual([
      { label: "Jumlah", value: money("199.000") },
      { label: "Metode", value: "Transfer bank" },
      { label: "Paket", value: "Starter" },
      { label: "Mulai", value: "01 Nov" },
      { label: "Sampai", value: "30 Nov" },
    ]);
  });

  test("formats a role on its own", () => {
    expect(
      detailParts(entry({ role: "production" }, "member.invited"), locale, id),
    ).toEqual([{ label: "Peran", value: "Produksi" }]);
  });

  test("formats an alert rule change", () => {
    expect(
      detailParts(
        entry(
          {
            days_before: { from: 3, to: 5 },
            channel: { from: "whatsapp", to: "email" },
            is_active: { from: true, to: false },
          },
          "alert_rules.updated",
        ),
        locale,
        id,
      ),
    ).toEqual([
      { label: "Hari sebelum kedaluwarsa", value: "3 hari → 5 hari" },
      { label: "Kanal", value: "WhatsApp → Email" },
      { label: "Diaktifkan", value: "Nyala → Mati" },
    ]);
  });

  test("drops a key with no label and an object that is not a change", () => {
    expect(
      detailParts(
        entry({ invitation_id: "6f2b", untouched: { deep: true } }),
        locale,
        id,
      ),
    ).toEqual([]);
  });
});
