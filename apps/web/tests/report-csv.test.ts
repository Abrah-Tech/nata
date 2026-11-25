import type { BusinessReport } from "@nata/api-client";
import { describe, expect, test } from "bun:test";

import { reportCsv } from "../lib/report-csv";
import { id } from "../lib/i18n/id";

const report: BusinessReport = {
  window_days: 30,
  can_export: true,
  profit_loss: {
    revenue: "40000.00",
    cost_of_goods: "24000.00",
    gross_profit: "16000.00",
    cost_incomplete: false,
  },
  stock: { materials: "0.00", products: "0.00", total: "0.00" },
  waste: { value: "0.00", movements: 0, by_material: [] },
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
});
