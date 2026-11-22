"use client";

import { ApiError, type Batch, type StockCountResult, type StockLevel } from "@nata/api-client";
import { useMemo, useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate, formatQty, normalizeDecimal } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { stockError } from "./error-copy";

type Props = {
  materials: StockLevel[];
  batches: Batch[];
  onClose: () => void;
  onApplied: (result: StockCountResult) => void;
};

type Group = {
  material: StockLevel;
  rows: Batch[];
};

/** Quantities are `numeric(18,3)`, so a difference is rounded to the third. */
function difference(counted: number, system: number): number {
  return Math.round((counted - system) * 1000) / 1000;
}

/** The ledger holds "10.000"; a count the owner types reads better as "10". */
function plain(value: string): string {
  return value.includes(".") ? value.replace(/0+$/, "").replace(/\.$/, "") : value;
}

/**
 * A count sheet, prefilled with what the ledger already believes.
 *
 * Only the rows the owner actually edits are sent, and a row that still agrees
 * writes nothing at all. The server applies the whole sheet in one
 * transaction, so a count is recorded completely or not at all.
 */
export function CountSheet({ materials, batches, onClose, onApplied }: Props) {
  const { t, locale } = useI18n();

  const groups: Group[] = useMemo(
    () =>
      materials.flatMap((material) => {
        const rows = batches.filter(
          (batch) =>
            batch.material_id === material.material_id &&
            Number(batch.qty_remaining) > 0,
        );

        return rows.length === 0 ? [] : [{ material, rows }];
      }),
    [materials, batches],
  );

  const [counts, setCounts] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      groups.flatMap((group) =>
        group.rows.map((row) => [row.id, plain(row.qty_remaining)]),
      ),
    ),
  );
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** Null when a row is not a number yet, which blocks the submit. */
  const countedValues = useMemo(
    () =>
      Object.fromEntries(
        Object.entries(counts).map(([id, value]) => {
          const counted = normalizeDecimal(value);

          return [id, counted === null ? null : Number(counted)];
        }),
      ),
    [counts],
  );

  const rows = groups.flatMap((group) =>
    group.rows.map((row) => {
      const counted = normalizeDecimal(counts[row.id] ?? "");
      const countedValue = countedValues[row.id] ?? null;

      return {
        row,
        counted,
        difference:
          countedValue === null
            ? null
            : difference(countedValue, Number(row.qty_remaining)),
      };
    }),
  );

  const byId = useMemo(
    () => new Map(rows.map((entry) => [entry.row.id, entry])),
    [rows],
  );

  const invalid = rows.some((entry) => entry.counted === null);
  const changed = rows.filter(
    (entry): entry is { row: Batch; counted: string; difference: number } =>
      entry.counted !== null && entry.difference !== 0,
  );
  const incomplete = changed.reduce(
    (total, entry) => total + entry.difference,
    0,
  );

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (saving || invalid || changed.length === 0) return;

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    setError(null);
    setSaving(true);

    try {
      const result = await api.applyStockCount(token, {
        note: note.trim() || null,
        counts: changed.map((entry) => ({
          batch_id: entry.row.id,
          qty: entry.counted,
        })),
      });

      onApplied(result);
    } catch (cause) {
      if (cause instanceof ApiError && cause.status === 401) {
        onClose();
      } else {
        setError(stockError(t, cause));
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      open
      wide
      title={t.stock.opname.title}
      subtitle={t.stock.opname.subtitle}
      closeLabel={t.stock.panel.close}
      onClose={onClose}
    >
      {groups.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t.stock.opname.empty}</p>
      ) : (
        <form
          className="flex flex-col gap-4"
          onSubmit={submit}
          aria-busy={saving ? true : undefined}
        >
          <Field label={t.stock.opname.note} hint={t.stock.opname.noteHint}>
            <Input
              type="text"
              value={note}
              placeholder={t.stock.opname.notePlaceholder}
              onChange={(event) => setNote(event.target.value)}
            />
          </Field>

          <div className="flex flex-col gap-4">
            {groups.map((group) => {
              const system = group.rows.reduce(
                (total, row) => total + Number(row.qty_remaining),
                0,
              );
              // An unfinished row falls back to the ledger so the total is
              // never misleading while the owner is still typing.
              const counted = group.rows.reduce(
                (total, row) =>
                  total +
                  (countedValues[row.id] ?? Number(row.qty_remaining)),
                0,
              );

              return (
                <section
                  key={group.material.material_id}
                  className="flex flex-col gap-1.5"
                  aria-label={group.material.name}
                >
                  <header className="flex items-baseline justify-between gap-3">
                    <h3 className="text-sm font-bold tracking-tight">
                      {group.material.name}
                    </h3>
                    <span className="text-2xs whitespace-nowrap text-ink-3 tabular-nums">
                      {fill(t.stock.opname.materialTotal, {
                        system: formatQty(String(system), locale),
                        counted: formatQty(String(counted), locale),
                        unit: group.material.base_unit.code,
                      })}
                    </span>
                  </header>

                  {group.rows.map((row) => {
                    const value = counts[row.id] ?? plain(row.qty_remaining);
                    const diff = byId.get(row.id)?.difference ?? 0;

                    return (
                      <div
                        key={row.id}
                        className="flex items-center gap-2 border-t border-border pt-2"
                      >
                        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                          <span className="truncate text-xs font-semibold">
                            {formatDate(row.received_at, locale)}
                          </span>
                          <span className="text-2xs text-ink-3 tabular-nums">
                            {fill(t.stock.opname.system, {
                              qty: formatQty(row.qty_remaining, locale),
                              unit: group.material.base_unit.code,
                            })}
                          </span>
                        </div>

                        <Input
                          type="text"
                          inputMode="decimal"
                          autoComplete="off"
                          aria-label={fill(t.stock.opname.countLabel, {
                            batch: formatDate(row.received_at, locale),
                            unit: group.material.base_unit.code,
                          })}
                          className="w-20 shrink-0 text-right tabular-nums"
                          value={value}
                          onChange={(event) =>
                            setCounts((current) => ({
                              ...current,
                              [row.id]: event.target.value,
                            }))
                          }
                        />

                        <span
                          className={`w-16 shrink-0 text-right text-2xs font-semibold tabular-nums ${
                            diff === 0
                              ? "text-ink-3"
                              : diff > 0
                                ? "text-brand"
                                : "text-destructive"
                          }`}
                        >
                          {diff > 0 ? "+" : ""}
                          {formatQty(String(diff), locale)}
                        </span>
                      </div>
                    );
                  })}
                </section>
              );
            })}
          </div>

          {error && <FormError>{error}</FormError>}

          <div className="-mx-5 -mb-5 sticky bottom-0 flex flex-col gap-1.5 border-t border-border bg-card px-5 pt-3 pb-5">
            <p className="text-2xs text-ink-3 tabular-nums">
              {invalid
                ? t.stock.errors.invalidNumber
                : changed.length === 0
                  ? t.stock.opname.noChange
                  : fill(t.stock.opname.summary, {
                    count: changed.length,
                    difference: formatQty(String(incomplete), locale),
                  })}
            </p>

            <div className="flex justify-end gap-2">
              <Button variant="ghost" type="button" onClick={onClose}>
                {t.stock.form.cancel}
              </Button>
              <Button type="submit" disabled={invalid || changed.length === 0}>
                {saving
                  ? t.stock.form.saving
                  : fill(t.stock.opname.submit, { count: changed.length })}
              </Button>
            </div>
          </div>
        </form>
      )}
    </Modal>
  );
}
