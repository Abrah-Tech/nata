import type { Alert, AlertRule } from "@nata/api-client";
import { describe, expect, test } from "bun:test";

import {
  alertParts,
  alertTypeLabel,
  deliveryLabel,
  draftFrom,
  isRuleDirty,
  parseDays,
  ruleUpdate,
} from "../app/settings/alerts";
import { id } from "../lib/i18n/id";

const locale = "id";

function rule(overrides: Partial<AlertRule> = {}): AlertRule {
  return {
    id: "00000000-0000-0000-0000-000000000000",
    type: "expiry",
    days_before: 7,
    channel: "whatsapp",
    is_active: true,
    created_at: "2026-11-12T01:00:00Z",
    ...overrides,
  };
}

function alert(overrides: Partial<Alert> = {}): Alert {
  return {
    id: "00000000-0000-0000-0000-000000000001",
    type: "expiry",
    status: "open",
    rule_id: "00000000-0000-0000-0000-000000000000",
    material_id: null,
    material_name: null,
    batch_id: null,
    product_id: null,
    product_name: null,
    product_batch_id: null,
    qty_remaining: null,
    expires_at: null,
    days_left: null,
    on_hand: null,
    min_stock: null,
    created_at: "2026-11-12T01:00:00Z",
    resolved_at: null,
    sent_at: null,
    delivery_status: "pending",
    attempts: 0,
    ...overrides,
  };
}

describe("parseDays", () => {
  test("reads a whole number of days", () => {
    expect(parseDays("7")).toBe(7);
    expect(parseDays(" 7 ")).toBe(7);
    expect(parseDays("0")).toBe(0);
  });

  test("rejects an empty, fractional or negative value", () => {
    expect(parseDays("")).toBeNull();
    expect(parseDays("   ")).toBeNull();
    expect(parseDays("2.5")).toBeNull();
    expect(parseDays("-1")).toBeNull();
    expect(parseDays("dua")).toBeNull();
  });
});

describe("ruleUpdate", () => {
  test("reports only the fields that differ", () => {
    expect(
      ruleUpdate(rule(), { days: "5", channel: "whatsapp", is_active: true }),
    ).toEqual({ days_before: 5 });

    expect(
      ruleUpdate(rule(), { days: "7", channel: "email", is_active: false }),
    ).toEqual({ channel: "email", is_active: false });
  });

  test("returns nothing when the draft matches the stored rule", () => {
    expect(
      ruleUpdate(rule(), { days: "7", channel: "whatsapp", is_active: true }),
    ).toEqual({});
    expect(isRuleDirty(rule(), draftFrom(rule()))).toBe(false);
  });

  test("keeps a low-stock rule free of a lead time", () => {
    const lowStock = rule({ type: "low_stock", days_before: null });

    expect(ruleUpdate(lowStock, draftFrom(lowStock))).toEqual({});
    expect(
      ruleUpdate(lowStock, { days: "3", channel: "whatsapp", is_active: true }),
    ).toEqual({});
  });

  test("marks a rule dirty when any field moved", () => {
    expect(
      isRuleDirty(rule(), { days: "7", channel: "email", is_active: true }),
    ).toBe(true);
  });
});

describe("alertTypeLabel", () => {
  test("names both rule types", () => {
    expect(alertTypeLabel(id, "expiry")).toBe("Peringatan kedaluwarsa");
    expect(alertTypeLabel(id, "low_stock")).toBe("Peringatan stok menipis");
  });
});

describe("deliveryLabel", () => {
  test("names a known status and falls back to the raw value", () => {
    expect(deliveryLabel(id, "sent")).toBe("Terkirim");
    expect(deliveryLabel(id, "skipped")).toBe("Dilewati");
    expect(deliveryLabel(id, "pending")).toBe("Menunggu dikirim");
  });
});

describe("alertParts", () => {
  test("describes an expiry alert from its batch", () => {
    expect(
      alertParts(
        alert({
          type: "expiry",
          material_name: "Tepung Terigu",
          expires_at: "2026-11-20",
          days_left: 8,
          qty_remaining: "2.500",
          delivery_status: "sent",
        }),
        locale,
        id,
      ),
    ).toEqual([
      { label: "Barang", value: "Tepung Terigu" },
      { label: "Kedaluwarsa", value: "20 Nov" },
      { label: "Sisa hari", value: "8 hari" },
      { label: "Sisa", value: "2,5" },
      { label: "Pengiriman", value: "Terkirim" },
    ]);
  });

  test("describes a low-stock alert from its stock and minimum", () => {
    expect(
      alertParts(
        alert({
          type: "low_stock",
          material_name: "Gula Pasir",
          on_hand: "1.250",
          min_stock: "3.000",
          delivery_status: "failed",
        }),
        locale,
        id,
      ),
    ).toEqual([
      { label: "Barang", value: "Gula Pasir" },
      { label: "Stok tersedia", value: "1,25" },
      { label: "Stok minimum", value: "3" },
      { label: "Pengiriman", value: "Gagal" },
    ]);
  });

  test("falls back to the product name when there is no material", () => {
    const parts = alertParts(
      alert({ type: "low_stock", product_name: "Kue Coklat" }),
      locale,
      id,
    );

    expect(parts[0]).toEqual({ label: "Barang", value: "Kue Coklat" });
  });

  test("omits the numbers an alert does not carry", () => {
    expect(alertParts(alert({ type: "expiry" }), locale, id)).toEqual([
      { label: "Pengiriman", value: "Menunggu dikirim" },
    ]);
  });
});
