import type { BusinessReport } from "@nata/api-client";
import type { jsPDF } from "jspdf";
import type { UserOptions } from "jspdf-autotable";

import { formatMoney, formatPercent, formatQty, formatStamp } from "./format";
import type { Dictionary, Locale } from "./i18n";

type Labels = Dictionary["reports"];

export type PdfValueRow = { label: string; value: string };

export type PdfTable = {
  head: string[];
  body: string[][];
  align: ("left" | "right")[];
};

export type PdfSection = {
  heading: string;
  note?: string;
  rows?: PdfValueRow[];
  table?: PdfTable;
};

export type ReportPdf = {
  title: string;
  generated: string;
  window: string;
  sections: PdfSection[];
};

/** A value the API leaves null is printed as a gap, never as zero. */
const EMPTY = "—";

function money(value: string | null, locale: Locale): string {
  return value === null ? EMPTY : formatMoney(Number(value), locale);
}

/**
 * The business report as a document, mirroring the screen section by section.
 *
 * The values are formatted for reading (a person, not a spreadsheet), because
 * the file is the view the user just looked at: same window, same rows, same
 * order. The screen is the source of truth for layout; this only renders it.
 */
export function reportPdf(
  data: BusinessReport,
  labels: Labels,
  locale: Locale,
  generatedAt: Date,
): ReportPdf {
  const profitLoss: PdfSection = {
    heading: labels.profitLoss.title,
    rows: [
      {
        label: labels.profitLoss.revenue,
        value: money(data.profit_loss.revenue, locale),
      },
      {
        label: labels.profitLoss.costOfGoods,
        value: money(data.profit_loss.cost_of_goods, locale),
      },
      {
        label: labels.profitLoss.grossProfit,
        value: money(data.profit_loss.gross_profit, locale),
      },
    ],
  };

  if (data.profit_loss.cost_incomplete) {
    profitLoss.note = labels.profitLoss.incomplete;
  }

  const stock: PdfSection = {
    heading: labels.stock.title,
    rows: [
      { label: labels.stock.materials, value: money(data.stock.materials, locale) },
      { label: labels.stock.products, value: money(data.stock.products, locale) },
      { label: labels.export.total, value: money(data.stock.total, locale) },
    ],
  };

  const waste: PdfSection = {
    heading: labels.waste.title,
    rows: [
      { label: labels.export.writeOffs, value: money(data.waste.value, locale) },
      { label: labels.export.movements, value: String(data.waste.movements) },
    ],
  };

  if (data.waste.by_material.length > 0) {
    waste.table = {
      head: [
        labels.export.material,
        labels.export.unit,
        labels.export.qty,
        labels.export.value,
      ],
      body: data.waste.by_material.map((row) => [
        row.material_name,
        row.unit_code,
        formatQty(row.qty, locale),
        money(row.value, locale),
      ]),
      align: ["left", "left", "right", "right"],
    };
  }

  const products: PdfSection = { heading: labels.products.title };

  if (data.products.length === 0) {
    products.note = labels.products.empty;
  } else {
    const incomplete = data.products.some((product) => product.cost_incomplete);

    products.table = {
      head: [
        labels.products.product,
        labels.products.price,
        labels.products.cost,
        labels.products.unitMargin,
        labels.products.sold,
        labels.products.revenue,
        labels.products.margin,
      ],
      body: data.products.map((product) => [
        product.cost_incomplete ? `${product.name} *` : product.name,
        money(product.sell_price, locale),
        product.unit_cost === null
          ? labels.products.noCost
          : money(product.unit_cost, locale),
        product.margin_per_unit === null
          ? EMPTY
          : `${money(product.margin_per_unit, locale)}${
              product.margin_pct
                ? ` (${formatPercent(product.margin_pct, locale)})`
                : ""
            }`,
        formatQty(product.sold_qty, locale),
        money(product.revenue, locale),
        money(product.margin, locale),
      ]),
      align: ["left", "right", "right", "right", "right", "right", "right"],
    };

    products.note = incomplete
      ? `${labels.products.costNote} * ${labels.products.incomplete}`
      : labels.products.costNote;
  }

  return {
    title: labels.title,
    generated: `${labels.export.generated}: ${formatStamp(generatedAt)}`,
    window: `${labels.export.window}: ${data.window_days}`,
    sections: [profitLoss, stock, waste, products],
  };
}

// The palette is the `nata.pen` token set, as RGB triples.
const INK: [number, number, number] = [16, 28, 24];
const MUTED: [number, number, number] = [90, 106, 99];
const LINE: [number, number, number] = [227, 231, 225];
const BRAND: [number, number, number] = [18, 89, 74];

const MARGIN = 14;
const PAGE_WIDTH = 210;
const PAGE_HEIGHT = 297;

type FinalY = { lastAutoTable: { finalY: number } };
type AutoTable = (doc: jsPDF, options: UserOptions) => void;

/**
 * Draws the report onto a document.
 *
 * Split from `downloadReportPdf` so the layout can be rendered and inspected
 * without saving a file; the caller owns the `jspdf` document and the table
 * plugin it loaded.
 */
export function renderReportPdf(
  doc: jsPDF,
  report: ReportPdf,
  autoTable: AutoTable,
): void {
  let y = MARGIN;

  doc.setFont("helvetica", "bold");
  doc.setFontSize(18);
  doc.setTextColor(...INK);
  doc.text(report.title, MARGIN, y);
  y += 8;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(9);
  doc.setTextColor(...MUTED);
  doc.text(report.generated, MARGIN, y);
  y += 5;
  doc.text(report.window, MARGIN, y);
  y += 8;

  for (const section of report.sections) {
    doc.setFont("helvetica", "bold");
    doc.setFontSize(12);
    doc.setTextColor(...BRAND);
    doc.text(section.heading, MARGIN, y);

    doc.setDrawColor(...LINE);
    doc.setLineWidth(0.3);
    doc.line(MARGIN, y + 2, PAGE_WIDTH - MARGIN, y + 2);
    y += 8;

    if (section.note) {
      doc.setFont("helvetica", "normal");
      doc.setFontSize(8.5);
      doc.setTextColor(...MUTED);
      const lines = doc.splitTextToSize(
        section.note,
        PAGE_WIDTH - MARGIN * 2,
      );
      doc.text(lines, MARGIN, y);
      y += lines.length * 4 + 3;
    }

    if (section.rows) {
      autoTable(doc, {
        startY: y,
        margin: { left: MARGIN, right: MARGIN },
        theme: "plain",
        tableWidth: "wrap",
        styles: {
          fontSize: 9,
          cellPadding: 1.5,
          textColor: INK,
          lineWidth: 0,
        },
        head: [],
        body: section.rows.map((row) => [row.label, row.value]),
        columnStyles: {
          0: { cellWidth: 110 },
          1: { cellWidth: 55, halign: "right" },
        },
      });
      y = (doc as unknown as FinalY).lastAutoTable.finalY + 6;
    }

    if (section.table) {
      autoTable(doc, {
        startY: y,
        margin: { left: MARGIN, right: MARGIN },
        theme: "striped",
        tableWidth: "wrap",
        showHead: "everyPage",
        styles: {
          fontSize: 8.5,
          cellPadding: 1.5,
          textColor: INK,
          lineColor: LINE,
          lineWidth: 0.2,
          overflow: "linebreak",
        },
        headStyles: {
          fillColor: BRAND,
          textColor: 255,
          fontStyle: "bold",
        },
        head: [section.table.head],
        body: section.table.body,
        columnStyles: Object.fromEntries(
          section.table.align.map((align, index) => [
            index,
            { halign: align },
          ]),
        ),
      });
      y = (doc as unknown as FinalY).lastAutoTable.finalY + 8;
    }
  }

  // Page numbers are written last so a table that spilled onto a new page is
  // counted too.
  const pages = doc.getNumberOfPages();

  for (let page = 1; page <= pages; page += 1) {
    doc.setPage(page);
    doc.setFont("helvetica", "normal");
    doc.setFontSize(8);
    doc.setTextColor(...MUTED);
    doc.text(`${page} / ${pages}`, PAGE_WIDTH - MARGIN, PAGE_HEIGHT - 8, {
      align: "right",
    });
  }
}

/**
 * Builds and saves the report as a PDF.
 *
 * `jspdf` and its table plugin are imported only when the user asks for the
 * file, so the report screen does not pay for them on every load. The model
 * comes from `reportPdf`, which keeps the layout testable without a DOM.
 */
export async function downloadReportPdf(
  filename: string,
  report: ReportPdf,
): Promise<void> {
  const { jsPDF } = await import("jspdf");
  const { default: autoTable } = await import("jspdf-autotable");

  const doc = new jsPDF({ unit: "mm", format: "a4" });
  renderReportPdf(doc, report, autoTable);
  doc.save(filename);
}
