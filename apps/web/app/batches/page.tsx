"use client";

import type { ProductBatch, ProductCorrection } from "@nata/api-client";
import { Search } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { StatusMessage } from "@/components/status";
import { LoadingRegion, SkeletonTable } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { daysUntil, formatDate, formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n, type Dictionary, type Locale } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";
import { CorrectionDialog } from "./correction-dialog";

type Filter = "all" | "expiring" | "expired";

type Load =
  | { status: "loading" }
  | { status: "ready"; batches: ProductBatch[] }
  | { status: "error" };

export default function BatchesPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<ProductBatch | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const fetchData = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    try {
      setLoad({ status: "ready", batches: await api.listProductBatches(token) });
    } catch {
      setLoad({ status: "error" });
    }
  }, []);

  useEffect(() => {
    if (session.status === "ready") {
      void fetchData();
    }
  }, [session.status, fetchData]);

  useEffect(() => {
    if (!notice) return;

    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const batches = load.status === "ready" ? load.batches : [];

  const counts = useMemo(() => {
    return {
      all: batches.length,
      expiring: batches.filter((row) => isExpiring(row)).length,
      expired: batches.filter((row) => isExpired(row)).length,
    };
  }, [batches]);

  const visible = useMemo(() => {
    const term = query.trim().toLowerCase();

    return batches.filter((row) => {
      if (term && !row.product_name.toLowerCase().includes(term)) return false;

      switch (filter) {
        case "expiring":
          return isExpiring(row);
        case "expired":
          return isExpired(row);
        default:
          return true;
      }
    });
  }, [batches, filter, query]);

  const summary = useMemo(() => {
    let value = 0;

    for (const row of batches) {
      value += Number(row.qty_remaining) * Number(row.unit_cost ?? 0);
    }

    return { value };
  }, [batches]);

  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canAdjust = permissions.includes("stock_adjust");

  const filterTabs: { key: Filter; label: string; short: string }[] = [
    { key: "all", label: t.batches.filters.all, short: t.batches.mobileFilters.all },
    {
      key: "expiring",
      label: t.batches.filters.expiring,
      short: t.batches.mobileFilters.expiring,
    },
    {
      key: "expired",
      label: t.batches.filters.expired,
      short: t.batches.mobileFilters.expired,
    },
  ];

  const subtitle = fill(t.batches.subtitle, {
    batches: batches.length,
    value: formatMoney(summary.value, locale),
  });

  /**
   * Roving tabindex for the filter tabs. The panel re-renders immediately, so
   * moving between tabs activates them instead of waiting for a second Enter.
   */
  function moveTab(index: number) {
    const next = (index + filterTabs.length) % filterTabs.length;
    setFilter(filterTabs[next].key);
    tabRefs.current[next]?.focus();
  }

  function onTabKeyDown(
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number,
  ) {
    switch (event.key) {
      case "ArrowLeft":
      case "ArrowUp":
        event.preventDefault();
        moveTab(index - 1);
        break;
      case "ArrowRight":
      case "ArrowDown":
        event.preventDefault();
        moveTab(index + 1);
        break;
      case "Home":
        event.preventDefault();
        moveTab(0);
        break;
      case "End":
        event.preventDefault();
        moveTab(filterTabs.length - 1);
        break;
    }
  }

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="batches">
          <div className="flex max-w-[1200px] flex-col gap-4">
            <LoadingRegion label={t.batches.loading}>
              <SkeletonTable rows={6} columns={5} />
            </LoadingRegion>
          </div>
        </AppShell>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[1200px] p-6">
          <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
            <p className="text-sm text-muted-foreground">{t.dashboard.error}</p>
            <Button
              variant="secondary"
              size="sm"
              className="self-start"
              type="button"
              onClick={() => void reloadSession()}
            >
              {t.dashboard.retry}
            </Button>
          </div>
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="batches" onSignOut={signOut}>
          <div className="flex max-w-[1200px] flex-col gap-4">
            <header>
              <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                {t.batches.title}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>
            </header>

            <StatusMessage
              message={notice}
              className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
            />

            {load.status === "loading" && (
              <LoadingRegion label={t.batches.loading}>
                <div className="rounded-xl border border-border bg-card p-5">
                  <SkeletonTable rows={6} columns={5} />
                </div>
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.batches.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.batches.retry}
                </Button>
              </div>
            )}

            {load.status === "ready" && (
              <section className="overflow-hidden rounded-xl border border-border bg-card">
                <div className="flex flex-col items-stretch gap-3 border-b border-border px-4 py-3 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between">
                  <div
                    role="tablist"
                    aria-label={t.batches.title}
                    className="flex flex-wrap gap-2 sm:flex-nowrap sm:gap-4.5"
                  >
                    {filterTabs.map((tab, index) => {
                      const active = filter === tab.key;

                      return (
                        <button
                          key={tab.key}
                          ref={(node) => {
                            tabRefs.current[index] = node;
                          }}
                          type="button"
                          role="tab"
                          id={`batch-tab-${tab.key}`}
                          aria-selected={active}
                          aria-controls="batch-panel"
                          tabIndex={active ? 0 : -1}
                          onKeyDown={(event) => onTabKeyDown(event, index)}
                          className={`flex items-center justify-center gap-1.5 border-b-2 px-2 pb-2 pt-0.5 text-sm whitespace-nowrap transition-colors duration-150 sm:flex-none sm:justify-start sm:px-0 ${
                            active
                              ? "border-brand font-bold text-ink"
                              : "border-transparent font-semibold text-ink-3 hover:border-border-strong hover:text-ink"
                          }`}
                          onClick={() => setFilter(tab.key)}
                        >
                          <span className="hidden sm:inline">{tab.label}</span>
                          <span className="sm:hidden">{tab.short}</span>
                          <span
                            className={`shrink-0 rounded-full px-1.5 py-px text-2xs font-bold ${
                              active
                                ? "bg-brand-soft text-brand"
                                : "bg-surface-alt text-ink-3"
                            }`}
                          >
                            {counts[tab.key]}
                          </span>
                        </button>
                      );
                    })}
                  </div>

                  <label className="flex w-full items-center gap-2 rounded-lg border border-border bg-surface-alt px-2.5 py-1.5 text-ink-3 transition-[border-color,box-shadow] duration-150 hover:border-ink-3 focus-within:border-brand focus-within:shadow-[0_0_0_3px_var(--brand-soft)] sm:w-[220px]">
                    <Search size={14} />
                    <input
                      type="search"
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder={t.batches.searchPlaceholder}
                      aria-label={t.batches.searchPlaceholder}
                      className="w-full cursor-text border-0 bg-transparent text-[1rem] text-ink outline-none placeholder:text-ink-3 sm:text-sm"
                    />
                  </label>
                </div>

                <div
                  role="tabpanel"
                  id="batch-panel"
                  aria-labelledby={`batch-tab-${filter}`}
                >
                  {batches.length === 0 ? (
                    <div className="flex flex-col items-center gap-2 px-5 py-11 text-center">
                      <h2 className="text-lg font-bold">
                        {t.batches.empty.title}
                      </h2>
                      <p className="text-sm text-muted-foreground">
                        {t.batches.empty.body}
                      </p>
                      <Button variant="ghost" asChild className="mt-1">
                        <Link href="/production">{t.batches.empty.cta}</Link>
                      </Button>
                    </div>
                  ) : visible.length === 0 ? (
                    <p className="px-4 py-6 text-sm text-muted-foreground">
                      {t.batches.empty.filtered}
                    </p>
                  ) : (
                    <div
                      role="table"
                      aria-label={t.batches.title}
                      className="@container"
                    >
                      {/* A container, not a breakpoint: the five fixed columns
                          need 526px, the five gaps 60px and the row inset 32px,
                          so the name column only gets room once this box passes
                          618px. Switching there would leave it 0px wide, so the
                          bar sits at 878px, which guarantees the name 260px and
                          keeps the same bar as the ingredient ledger. */}
                      <div
                        role="row"
                        className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 border-b border-border bg-surface-alt px-3.5 py-2 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[878px]:hidden"
                      >
                        <span role="columnheader">
                          {t.batches.table.product}
                        </span>
                        <span role="columnheader">
                          {t.batches.table.remaining}
                        </span>
                        <span role="columnheader">
                          {t.batches.table.correct}
                        </span>
                      </div>

                      <div
                        role="row"
                        className="hidden items-center gap-3 bg-surface-alt px-4 py-2.5 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[878px]:grid @min-[878px]:grid-cols-[minmax(0,1fr)_96px_140px_90px_104px_96px]"
                      >
                        <span role="columnheader">
                          {t.batches.table.product}
                        </span>
                        <span role="columnheader">
                          {t.batches.table.produced}
                        </span>
                        <span role="columnheader">
                          {t.batches.table.expires}
                        </span>
                        <span role="columnheader">
                          {t.batches.table.remaining}
                        </span>
                        <span role="columnheader">
                          {t.batches.table.unitCost}
                        </span>
                      </div>

                      {visible.map((row) => {
                        const status = statusOf(row, t);

                        return (
                          <div
                            role="row"
                            key={row.id}
                            className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 border-b border-border px-3.5 py-3 text-sm last:border-b-0 @min-[878px]:grid-cols-[minmax(0,1fr)_96px_140px_90px_104px_96px] @min-[878px]:gap-3 @min-[878px]:px-4"
                          >
                            <div
                              role="cell"
                              className="flex min-w-0 flex-col gap-px"
                            >
                              <span className="text-sm font-semibold text-ink">
                                {row.product_name}
                              </span>
                              {/* Expiry is its own column on a wide row; on a
                                  phone it is the only place the date exists. */}
                              <span className="truncate text-2xs text-ink-3 @min-[878px]:hidden">
                                {expiryText(row, t, locale)}
                              </span>
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-2 @min-[878px]:block"
                            >
                              {formatDate(row.produced_on, locale)}
                            </div>

                            <div
                              role="cell"
                              className="hidden flex-col items-start gap-1 @min-[878px]:flex"
                            >
                              <span className="text-xs text-ink-2">
                                {expiryText(row, t, locale)}
                              </span>
                              <Badge variant={status.tone}>{status.label}</Badge>
                            </div>

                            <div
                              role="cell"
                              className="flex items-baseline justify-self-end gap-1 @min-[878px]:justify-self-auto"
                            >
                              <strong className="text-sm font-bold tabular-nums">
                                {formatQty(row.qty_remaining, locale)}
                              </strong>
                              <span className="text-xs text-ink-3">
                                {t.batches.meta.unit}
                              </span>
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-2 tabular-nums @min-[878px]:block"
                            >
                              {row.unit_cost === null
                                ? "—"
                                : formatMoney(Number(row.unit_cost), locale)}
                            </div>

                            <div
                              role="cell"
                              className="flex justify-end @min-[878px]:justify-start"
                            >
                              {canAdjust && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  type="button"
                                  onClick={() => setSelected(row)}
                                  aria-label={fill(t.batches.table.correctNamed, {
                                    name: row.product_name,
                                  })}
                                >
                                  {t.batches.table.correct}
                                </Button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              </section>
            )}
          </div>

          {selected && (
            <CorrectionDialog
              batch={selected}
              onClose={() => setSelected(null)}
              onSaved={(correction: ProductCorrection) => {
                const name = selected.product_name;

                setSelected(null);
                setNotice(
                  fill(t.batches.result.corrected, {
                    product: name,
                    qty: formatQty(correction.batch_qty_remaining, locale),
                  }),
                );
                void fetchData();
              }}
            />
          )}
        </AppShell>
      )}
    </>
  );
}

type Tone = "danger" | "warning" | "brand" | "neutral";

function isExpired(row: ProductBatch): boolean {
  return row.expires_at !== null && daysUntil(row.expires_at) < 0;
}

function isExpiring(row: ProductBatch): boolean {
  if (row.expires_at === null) return false;

  const days = daysUntil(row.expires_at);

  return days >= 0 && days <= 7;
}

function statusOf(row: ProductBatch, t: Dictionary): { label: string; tone: Tone } {
  if (row.expires_at === null) {
    return { label: t.batches.status.noExpiry, tone: "neutral" };
  }

  const days = daysUntil(row.expires_at);

  if (days < 0) {
    return { label: t.batches.status.expired, tone: "danger" };
  }

  if (days === 0) {
    return { label: t.batches.meta.today, tone: "warning" };
  }

  if (days <= 7) {
    return {
      label: fill(t.batches.status.expiring, { days }),
      tone: "warning",
    };
  }

  return { label: t.batches.status.safe, tone: "brand" };
}

function expiryText(row: ProductBatch, t: Dictionary, locale: Locale): string {
  if (row.expires_at === null) return t.batches.status.noExpiry;

  const days = daysUntil(row.expires_at);
  const date = formatDate(row.expires_at, locale);

  if (days < 0) {
    return `${date} · ${fill(t.batches.meta.daysOverdue, { days: Math.abs(days) })}`;
  }

  if (days === 0) return `${date} · ${t.batches.meta.today}`;

  return `${date} · ${fill(t.batches.meta.daysLeft, { days })}`;
}
