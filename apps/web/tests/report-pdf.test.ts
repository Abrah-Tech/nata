import type { BusinessReport } from "@nata/api-client";
import { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import { describe, expect, test } from "bun:test";

import {
  formatDate,
  formatMoney,
  formatPercent,
  formatQty,
} from "../lib/format";
import { reportPdf, renderReportPdf } from "../lib/report-pdf";
import { id } from "../lib/i18n/id";

const report: BusinessReport = {
  window_days: 30,
  range: { from: "2026-10-27", to: "2026-11-25", days: 30 },
  can_export: true,
  can_advanced: true,
  profit_loss: {
    revenue: "40000.00",
    cost_of_goods: "24000.00",
    gross_profit: "16000.00",
    cost_incomplete: false,
  },
  trend: [
    {
      date: "2026-11-24",
      revenue: "20000.00",
      cost_of_goods: "12000.00",
      gross_profit: "8000.00",
      sold_qty: "1.000",
      cost_incomplete: false,
    },
    {
      date: "2026-11-25",
      revenue: "20000.00",
      cost_of_goods: null,
      gross_profit: null,
      sold_qty: "1.000",
      cost_incomplete: true,
    },
  ],
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
  test("follows the screen order, with the deeper sections in place", () => {
    const pdf = reportPdf(report, id.reports, "id", new Date("2026-11-25T02:00:00Z"));

    expect(pdf.sections.map((section) => section.heading)).toEqual([
      id.reports.profitLoss.title,
      id.reports.trend.title,
      id.reports.stock.title,
      id.reports.waste.title,
      id.reports.usage.title,
      id.reports.purchasing.title,
      id.reports.products.title,
    ]);
    expect(pdf.generated).toContain(id.reports.export.generated);
    expect(pdf.period).toContain(id.reports.export.period);
    expect(pdf.period).toContain("2026");
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

    const waste = pdf.sections[3];
    expect(waste.table?.body).toEqual([
      ["Tepung", "kg", formatQty("1.500", "id"), formatMoney(2000, "id")],
    ]);

    const products = pdf.sections[6];
    expect(products.table?.body?.[0][0]).toBe("Roti");
    expect(products.table?.body?.[0][3]).toBe(
      `${formatMoney(4000, "id")} (${formatPercent("66.67", "id")})`,
    );
  });

  test("prints the trend, usage, and purchasing tables", () => {
    const pdf = reportPdf(report, id.reports, "id", new Date("2026-11-25T02:00:00Z"));

    const [trend, usage, purchasing] = [pdf.sections[1], pdf.sections[4], pdf.sections[5]];

    // The second day has no cost, so its profit is a gap and the note says why.
    expect(trend.table?.body?.[0]).toEqual([
      formatDate("2026-11-24", "id"),
      formatMoney(20000, "id"),
      formatMoney(8000, "id"),
      formatQty("1.000", "id"),
    ]);
    expect(trend.table?.body?.[1].slice(2, 4)).toEqual(["—", formatQty("1.000", "id")]);
    expect(trend.note).toContain(id.reports.trend.incomplete);

    expect(usage.table?.body).toEqual([
      [
        "Tepung",
        "kg",
        formatQty("3.000", "id"),
        formatMoney(10000, "id"),
        formatMoney(30000, "id"),
        "2",
      ],
    ]);

    expect(purchasing.table?.body).toEqual([
      [
        "Toko Sumber",
        "Tepung",
        "kg",
        formatQty("14.000", "id"),
        formatMoney(144000, "id"),
        "2",
      ],
    ]);
  });

  test("prints a batch with no supplier under its own label", () => {
    const pdf = reportPdf(
      {
        ...report,
        purchasing: [
          {
            ...report.purchasing[0],
            supplier_id: null,
            supplier_name: null,
          },
        ],
      },
      id.reports,
      "id",
      new Date("2026-11-25T02:00:00Z"),
    );

    expect(pdf.sections[5].table?.body?.[0][0]).toBe(id.reports.purchasing.noSupplier);
  });

  test("says when a deeper section is empty instead of printing a blank table", () => {
    const pdf = reportPdf(
      { ...report, trend: [], usage: [], purchasing: [] },
      id.reports,
      "id",
      new Date("2026-11-25T02:00:00Z"),
    );

    expect(pdf.sections[1].table).toBeUndefined();
    expect(pdf.sections[1].note).toBe(id.reports.trend.empty);
    expect(pdf.sections[4].note).toBe(id.reports.usage.empty);
    expect(pdf.sections[5].note).toBe(id.reports.purchasing.empty);
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

    const products = pdf.sections[6];
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

    const products = pdf.sections[6];
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
