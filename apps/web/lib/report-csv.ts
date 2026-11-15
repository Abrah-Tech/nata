import type { BusinessReport } from "@takar/api-client";

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
    [labels.export.window, String(data.window_days)],
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

  const sections: string[][][] = [summary, stock, waste, wasteDetail, products]
    .filter((section) => section.length > 0);

  return toCsv(
    sections.flatMap((section, index) =>
      index === 0 ? section : [[], ...section],
    ),
  );
}
