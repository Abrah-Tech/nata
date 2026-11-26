import type { BusinessReport } from "@nata/api-client";
import { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import { describe, expect, test } from "bun:test";

import { formatMoney, formatPercent, formatQty } from "../lib/format";
import { reportPdf, renderReportPdf } from "../lib/report-pdf";
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
  stock: { materials: "10000.00", products: "5000.00", total: "15000.00" },
  waste: {
    value: "2000.00",
    movements: 2,
    by_material: [
      {
        material_id: "m1",
        material_name: "Tepung",
        unit_code: "kg",
        qty: "1.500",
        value: "2000.00",
      },
    ],
  },
  products: [
    {
      product_id: "p1",
      name: "Roti",
      sell_price: "10000.00",
      unit_cost: "6000.00",
      margin_per_unit: "4000.00",
      margin_pct: "66.67",
      sold_qty: "4.000",
      revenue: "40000.00",
      cost_of_goods: "24000.00",
      margin: "16000.00",
      cost_incomplete: false,
    },
  ],
};

describe("reportPdf", () => {
  test("follows the screen order: profit and loss, stock, waste, products", () => {
    const pdf = reportPdf(report, id.reports, "id", new Date("2026-11-25T02:00:00Z"));

    expect(pdf.sections.map((section) => section.heading)).toEqual([
      id.reports.profitLoss.title,
      id.reports.stock.title,
      id.reports.waste.title,
      id.reports.products.title,
    ]);
    expect(pdf.generated).toContain(id.reports.export.generated);
    expect(pdf.window).toBe("Periode (hari): 30");
  });

  test("prints an unknown cost as a gap, not as zero", () => {
    const pdf = reportPdf(
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
      "id",
      new Date("2026-11-25T02:00:00Z"),
    );

    const [profitLoss] = pdf.sections;
    expect(profitLoss.rows?.map((row) => row.value).slice(1)).toEqual([
      "—",
      "—",
    ]);
    expect(profitLoss.rows?.[0].value).toBe(formatMoney(15000, "id"));
    expect(profitLoss.note).toBe(id.reports.profitLoss.incomplete);
  });

  test("lists the waste by ingredient and the products with formatted amounts", () => {
    const pdf = reportPdf(report, id.reports, "id", new Date("2026-11-25T02:00:00Z"));

    const waste = pdf.sections[2];
    expect(waste.table?.body).toEqual([
      ["Tepung", "kg", formatQty("1.500", "id"), formatMoney(2000, "id")],
    ]);

    const products = pdf.sections[3];
    expect(products.table?.body?.[0][0]).toBe("Roti");
    expect(products.table?.body?.[0][3]).toBe(
      `${formatMoney(4000, "id")} (${formatPercent("66.67", "id")})`,
    );
  });

  test("marks a product whose cost is incomplete", () => {
    const pdf = reportPdf(
      {
        ...report,
        products: [{ ...report.products[0], cost_incomplete: true }],
      },
      id.reports,
      "id",
      new Date("2026-11-25T02:00:00Z"),
    );

    const products = pdf.sections[3];
    expect(products.table?.body?.[0][0]).toBe("Roti *");
    expect(products.note).toContain(id.reports.products.incomplete);
  });

  test("says when there is no product instead of printing an empty table", () => {
    const pdf = reportPdf(
      { ...report, products: [] },
      id.reports,
      "id",
      new Date("2026-11-25T02:00:00Z"),
    );

    const products = pdf.sections[3];
    expect(products.table).toBeUndefined();
    expect(products.note).toBe(id.reports.products.empty);
  });
});

describe("renderReportPdf", () => {
  test("draws the report onto a document", () => {
    const doc = new jsPDF({ unit: "mm", format: "a4" });

    renderReportPdf(
      doc,
      reportPdf(report, id.reports, "id", new Date("2026-11-25T02:00:00Z")),
      autoTable,
    );

    const bytes = new Uint8Array(doc.output("arraybuffer"));

    expect(doc.getNumberOfPages()).toBeGreaterThan(0);
    expect(String.fromCharCode(...bytes.slice(0, 5))).toBe("%PDF-");
    expect(bytes.byteLength).toBeGreaterThan(1000);
  });
});
