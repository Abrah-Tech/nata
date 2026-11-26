import type { BusinessReport } from "@nata/api-client";
import { describe, expect, test } from "bun:test";

import { reportCsv } from "../lib/report-csv";
import { id } from "../lib/i18n/id";

const report: BusinessReport = {
  window_days: 30,
  range: { from: "2026-10-27", to: "2026-11-25", days: 30 },
  can_export: true,
  can_advanced: false,
  profit_loss: {
    revenue: "40000.00",
    cost_of_goods: "24000.00",
    gross_profit: "16000.00",
    cost_incomplete: false,
  },
  trend: [],
  stock: { materials: "0.00", products: "0.00", total: "0.00" },
  waste: { value: "0.00", movements: 0, by_material: [] },
  usage: [],
  purchasing: [],
  products: [],
};

describe("reportCsv", () => {
  test("leads with the profit and loss, ahead of the stock value", () => {
    const csv = reportCsv(report, id.reports, new Date("2026-11-25T02:00:00Z"));

    const profitIndex = csv.indexOf(id.reports.profitLoss.title);
    expect(profitIndex).toBeGreaterThan(-1);
    expect(profitIndex).toBeLessThan(csv.indexOf(id.reports.stock.title));
    expect(csv).toContain("40000.00");
    expect(csv).toContain("16000.00");
  });

  test("prints the period it covered, not the tenant's default window", () => {
    const csv = reportCsv(report, id.reports, new Date("2026-11-25T02:00:00Z"));

    const row = csv
      .split("\r\n")
      .find((line) => line.startsWith(id.reports.export.period));
    expect(row).toBe(`${id.reports.export.period},2026-10-27 to 2026-11-25`);
  });

  test("says the cost is incomplete instead of printing a blank as zero", () => {
    const csv = reportCsv(
      {
        ...report,
        profit_loss: {
          revenue: "15000.00",
          cost_of_goods: null,
          gross_profit: null,
          cost_incomplete: true,
        },
      },
      id.reports,
      new Date("2026-11-25T02:00:00Z"),
    );

    const row = csv
      .split("\r\n")
      .find((line) => line.startsWith(id.reports.export.costIncomplete));
    expect(row).toBe(`${id.reports.export.costIncomplete},${id.reports.export.yes}`);
  });

  test("leaves out the deeper sections a tenant cannot read", () => {
    const csv = reportCsv(report, id.reports, new Date("2026-11-25T02:00:00Z"));

    expect(csv).not.toContain(id.reports.trend.title);
    expect(csv).not.toContain(id.reports.usage.title);
    expect(csv).not.toContain(id.reports.purchasing.title);
  });

  test("adds the trend, usage, and purchasing tables when the plan allows", () => {
    const csv = reportCsv(
      {
        ...report,
        can_advanced: true,
        trend: [
          {
            date: "2026-11-24",
            revenue: "20000.00",
            cost_of_goods: "12000.00",
            gross_profit: "8000.00",
            sold_qty: "1.000",
            cost_incomplete: false,
          },
        ],
        usage: [
          {
            material_id: "m1",
            material_name: "Tepung",
            unit_code: "kg",
            qty: "3.000",
            unit_cost: "10000.0000",
            value: "30000.00",
            runs: 2,
          },
        ],
        purchasing: [
          {
            supplier_id: "s1",
            supplier_name: "Toko Sumber",
            material_id: "m1",
            material_name: "Tepung",
            unit_code: "kg",
            qty: "14.000",
            value: "144000.00",
            batches: 2,
          },
        ],
      },
      id.reports,
      new Date("2026-11-25T02:00:00Z"),
    );

    const trendIndex = csv.indexOf(id.reports.trend.title);
    const usageIndex = csv.indexOf(id.reports.usage.title);
    const purchasingIndex = csv.indexOf(id.reports.purchasing.title);

    // Same order as the screen: trend, then usage, then purchasing.
    expect(trendIndex).toBeGreaterThan(-1);
    expect(trendIndex).toBeLessThan(usageIndex);
    expect(usageIndex).toBeLessThan(purchasingIndex);
    expect(csv).toContain("2026-11-24,20000.00,8000.00,1.000,");
    expect(csv).toContain("Tepung,kg,3.000,10000.0000,30000.00,2");
    expect(csv).toContain("Toko Sumber,Tepung,kg,14.000,144000.00,2");
  });
});
