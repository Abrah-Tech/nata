"use client";

import type { PriceHistory, PurchaseRecommendation } from "@nata/api-client";
import { X } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogSheet,
  DialogTitle,
} from "@/components/ui/dialog";
import { LoadingRegion } from "@/components/skeletons";
import { api } from "@/lib/api";
import { formatDate, formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

type Props = {
  /** The recommendation the sheet was opened from, so the header can name
   *  the ingredient and repeat the quantity it suggests buying. */
  recommendation: PurchaseRecommendation;
  onClose: () => void;
};

/**
 * What each supplier actually charged, read from the batch ledger.
 *
 * A batch is the receipt, so its `unit_cost` is the price paid. No separate
 * price table means the history cannot drift from the stock it describes.
 */
export function PriceHistoryPanel({ recommendation, onClose }: Props) {
  const { t, locale } = useI18n();
  const [history, setHistory] = useState<PriceHistory | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const token = readToken();

    if (!token) return;

    let cancelled = false;

    api
      .priceHistory(token, recommendation.material_id)
      .then((data) => {
        if (!cancelled) setHistory(data);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, [recommendation.material_id]);

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogSheet>
        <header className="flex items-start justify-between gap-3 border-b border-border px-5 pt-5 pb-3.5">
          <div className="min-w-0">
            <p className="text-2xs font-bold tracking-widest text-ink-3 uppercase">
              {t.purchasing.priceHistory.title}
            </p>
            <DialogTitle className="mt-0.5 text-xl font-extrabold">
              {recommendation.material_name}
            </DialogTitle>
            <p className="mt-1 text-xs text-ink-2 tabular-nums">
              {fill(t.purchasing.priceHistory.suggest, {
                qty: formatQty(recommendation.recommended_qty, locale),
                unit: recommendation.unit_code,
              })}
            </p>
          </div>
          <DialogClose
            aria-label={t.purchasing.priceHistory.close}
            className="-mt-1 shrink-0 rounded-md p-1 text-ink-3 transition-colors duration-150 hover:bg-surface-alt hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
          >
            <X size={16} />
          </DialogClose>
        </header>

        <div className="flex flex-1 flex-col gap-5 overflow-y-auto overscroll-contain px-5 pt-4 pb-6">
          {failed && (
            <div className="flex flex-col gap-2">
              <p className="text-sm text-muted-foreground">
                {t.purchasing.priceHistory.error}
              </p>
              <Button
                variant="secondary"
                size="sm"
                className="self-start"
                type="button"
                onClick={() => window.location.reload()}
              >
                {t.purchasing.retry}
              </Button>
            </div>
          )}

          {!failed && !history && (
            <LoadingRegion label={t.purchasing.priceHistory.loading}>
              <div className="flex flex-col gap-2">
                <div className="h-16 rounded-lg bg-surface-alt" />
                <div className="h-16 rounded-lg bg-surface-alt" />
              </div>
            </LoadingRegion>
          )}

          {history && history.suppliers.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {t.purchasing.priceHistory.empty}
            </p>
          )}

          {history &&
            history.suppliers.map((supplier) => (
              <section
                className="flex flex-col gap-2"
                key={supplier.supplier_id ?? "none"}
              >
                <header className="flex items-baseline justify-between gap-3">
                  <h3 className="min-w-0 truncate text-sm font-bold">
                    {supplier.supplier_name ??
                      t.purchasing.priceHistory.noSupplier}
                  </h3>
                  <span className="shrink-0 text-2xs text-ink-3 tabular-nums">
                    {fill(t.purchasing.priceHistory.receipts, {
                      count: supplier.purchases.length,
                    })}
                  </span>
                </header>

                <dl className="grid grid-cols-2 gap-1.5">
                  <Fact
                    label={t.purchasing.priceHistory.last}
                    value={formatMoney(Number(supplier.last_price), locale)}
                  />
                  <Fact
                    label={t.purchasing.priceHistory.total}
                    value={`${formatQty(supplier.total_qty, locale)} ${history.unit_code}`}
                  />
                  <Fact
                    label={t.purchasing.priceHistory.lowest}
                    value={formatMoney(Number(supplier.lowest_price), locale)}
                  />
                  <Fact
                    label={t.purchasing.priceHistory.highest}
                    value={formatMoney(Number(supplier.highest_price), locale)}
                  />
                </dl>

                <ul className="flex list-none flex-col gap-1.5 p-0">
                  {supplier.purchases.map((purchase) => (
                    <li
                      className="flex flex-col gap-0.5 border-b border-border pb-1.5 last:border-b-0"
                      key={purchase.id}
                    >
                      <div className="flex items-baseline justify-between gap-3">
                        <span className="min-w-0 truncate text-xs font-semibold">
                          {formatDate(purchase.received_at, locale)}
                        </span>
                        <strong className="shrink-0 text-sm font-bold tabular-nums">
                          {formatMoney(Number(purchase.total), locale)}
                        </strong>
                      </div>
                      <span className="text-2xs text-ink-3 tabular-nums">
                        {fill(t.purchasing.priceHistory.unitLine, {
                          qty: formatQty(purchase.qty, locale),
                          unit: history.unit_code,
                          price: formatMoney(Number(purchase.unit_cost), locale),
                        })}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
        </div>
      </DialogSheet>
    </Dialog>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5 rounded-md bg-surface-alt px-2.5 py-2">
      <dt className="text-2xs tracking-wide text-ink-3 uppercase">{label}</dt>
      <dd className="text-sm font-bold tabular-nums">{value}</dd>
    </div>
  );
}
