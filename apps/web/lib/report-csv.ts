import type { BusinessReport } from "@nata/api-client";

import { toCsv } from "./csv";
import { formatStamp } from "./format";
import type { Dictionary } from "./i18n";

type Labels = Dictionary["reports"];

/**
 * The business report as a spreadsheet file.
 *
 * It mirrors the screen section by section, because the file is meant to be
 * the view the user just looked at: same window, same rows, same order. Only
 * the labels are localized; the amounts stay exactly as the API sent them, so
 * a spreadsheet can add them up.
 */
export function reportCsv(
  data: BusinessReport,
  labels: Labels,
  generatedAt: Date,
): string {
  const summary = [
    [labels.title],
    [labels.export.generated, formatStamp(generatedAt)],
    [labels.export.period, `${data.range.from} to ${data.range.to}`],
  ];

  const profitLoss = [
    [labels.profitLoss.title],
    [labels.profitLoss.revenue, data.profit_loss.revenue],
    [labels.profitLoss.costOfGoods, data.profit_loss.cost_of_goods ?? ""],
    [labels.profitLoss.grossProfit, data.profit_loss.gross_profit ?? ""],
    // An unknown cost is printed as a gap, and the row says why, rather than
    // letting an empty cell read as zero.
    ...(data.profit_loss.cost_incomplete
      ? [[labels.export.costIncomplete, labels.export.yes]]
      : []),
  ];

  const stock = [
    [labels.stock.title],
    [labels.stock.materials, data.stock.materials],
    [labels.stock.products, data.stock.products],
    [labels.export.total, data.stock.total],
  ];

  const waste = [
    [labels.waste.title],
    [labels.export.writeOffs, data.waste.value],
    [labels.export.movements, String(data.waste.movements)],
  ];

  // The screen shows the biggest losses only; an empty list is left out
  // rather than printed as a header with nothing under it.
  const wasteDetail =
    data.waste.by_material.length === 0
      ? []
      : [
          [labels.export.wasteDetail],
          [
            labels.export.material,
            labels.export.unit,
            labels.export.qty,
            labels.export.value,
          ],
          ...data.waste.by_material.map((row) => [
            row.material_name,
            row.unit_code,
            row.qty,
            row.value,
          ]),
        ];

  // The deeper sections belong to Business, so a tenant without them gets a
  // file with no empty headers in it.
  const trend =
    data.trend.length === 0
      ? []
      : [
          [labels.trend.title],
          [
            labels.export.date,
            labels.trend.revenue,
            labels.trend.grossProfit,
            labels.trend.sold,
            labels.export.costIncomplete,
          ],
          ...data.trend.map((day) => [
            day.date,
            day.revenue,
            day.gross_profit ?? "",
            day.sold_qty,
            day.cost_incomplete ? labels.export.yes : labels.export.no,
          ]),
        ];

  const usage =
    data.usage.length === 0
      ? []
      : [
          [labels.usage.title],
          [
            labels.usage.material,
            labels.usage.unit,
            labels.usage.qty,
            labels.usage.unitCost,
            labels.usage.value,
            labels.usage.runs,
          ],
          ...data.usage.map((row) => [
            row.material_name,
            row.unit_code,
            row.qty,
            row.unit_cost,
            row.value,
            String(row.runs),
          ]),
        ];

  const purchasing =
    data.purchasing.length === 0
      ? []
      : [
          [labels.purchasing.title],
          [
            labels.purchasing.supplier,
            labels.purchasing.material,
            labels.purchasing.unit,
            labels.purchasing.qty,
            labels.purchasing.value,
            labels.purchasing.batches,
          ],
          ...data.purchasing.map((row) => [
            row.supplier_name ?? labels.purchasing.noSupplier,
            row.material_name,
            row.unit_code,
            row.qty,
            row.value,
            String(row.batches),
          ]),
        ];

  const products = [
    [labels.products.title],
    [
      labels.products.product,
      labels.products.price,
      labels.products.cost,
      labels.products.unitMargin,
      labels.export.marginPct,
      labels.products.sold,
      labels.products.revenue,
      labels.export.costOfGoods,
      labels.products.margin,
      labels.export.costIncomplete,
    ],
    ...data.products.map((product) => [
      product.name,
      product.sell_price,
      product.unit_cost ?? "",
      product.margin_per_unit ?? "",
      product.margin_pct ?? "",
      product.sold_qty,
      product.revenue,
      product.cost_of_goods ?? "",
      product.margin ?? "",
      product.cost_incomplete ? labels.export.yes : labels.export.no,
    ]),
  ];

  const sections: string[][][] = [
    summary,
    profitLoss,
    trend,
    stock,
    waste,
    wasteDetail,
    usage,
    purchasing,
    products,
  ]
    .filter((section) => section.length > 0);

  return toCsv(
    sections.flatMap((section, index) =>
      index === 0 ? section : [[], ...section],
    ),
  );
}
