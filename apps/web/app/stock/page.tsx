"use client";

import type {
  Batch,
  Material,
  StockLevel,
  Supplier,
  Unit,
} from "@nata/api-client";
import { Ellipsis, Plus, RotateCcw, Search, X } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { StatusMessage } from "@/components/status";
import { LoadingRegion, SkeletonTable } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { daysUntil, formatDate, formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import {
  dismissEntry,
  flushQueue,
  listQueue,
  offlineQueueAvailable,
  retryEntry,
  type QueuedStockIn,
} from "@/lib/outbox";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";
import { BatchPanel } from "./batch-panel";
import { MaterialDialog } from "./material-dialog";
import { StockInDialog } from "./stock-in-dialog";

type Filter = "all" | "below_min" | "expiring" | "empty";

type Data = {
  stock: StockLevel[];
  materials: Material[];
  suppliers: Supplier[];
  units: Unit[];
  batches: Batch[];
};

type Load =
  | { status: "loading" }
  | { status: "ready"; data: Data }
  | { status: "error" };

export default function StockPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [panelId, setPanelId] = useState<string | null>(null);
  const [stockInFor, setStockInFor] = useState<string | null | undefined>();
  const [materialOpen, setMaterialOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [queue, setQueue] = useState<QueuedStockIn[]>([]);
  const [syncing, setSyncing] = useState(false);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const fetchData = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    try {
      const [stock, materials, suppliers, units, batches] = await Promise.all([
        api.listStock(token),
        api.listMaterials(token),
        api.listSuppliers(token),
        api.listUnits(token),
        api.listBatches(token),
      ]);

      setLoad({
        status: "ready",
        data: { stock, materials, suppliers, units, batches },
      });
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

  const refreshQueue = useCallback(async () => {
    if (!offlineQueueAvailable()) return;

    setQueue(await listQueue());
  }, []);

  const syncNow = useCallback(async () => {
    const token = readToken();

    if (!token || !offlineQueueAvailable()) return;

    setSyncing(true);

    try {
      const summary = await flushQueue(token);

      if (summary.synced > 0) {
        setNotice(fill(t.stock.offline.synced, { count: summary.synced }));
        void fetchData();
      }

      if (summary.failed > 0) {
        setNotice(fill(t.stock.offline.rejected, { count: summary.failed }));
      }
    } catch {
      // Still offline, or the server is unreachable: entries stay queued.
    } finally {
      await refreshQueue();
      setSyncing(false);
    }
  }, [fetchData, refreshQueue, t]);

  useEffect(() => {
    if (session.status !== "ready" || !offlineQueueAvailable()) return;

    void refreshQueue();

    if (navigator.onLine) {
      void syncNow();
    }

    const handleOnline = () => void syncNow();

    window.addEventListener("online", handleOnline);
    return () => window.removeEventListener("online", handleOnline);
  }, [session.status, refreshQueue, syncNow]);

  async function retryQueued(clientRef: string) {
    await retryEntry(clientRef);
    await refreshQueue();

    if (navigator.onLine) {
      await syncNow();
    }
  }

  async function dismissQueued(clientRef: string) {
    await dismissEntry(clientRef);
    await refreshQueue();
  }

  const data = load.status === "ready" ? load.data : null;

  // The dashboard links straight to one material; open its drawer once, then
  // leave the page alone so closing it sticks.
  const deepLinkApplied = useRef(false);

  useEffect(() => {
    if (!data || deepLinkApplied.current) return;

    deepLinkApplied.current = true;

    const id = new URLSearchParams(window.location.search).get("material");

    if (id && data.stock.some((row) => row.material_id === id)) {
      setPanelId(id);
    }
  }, [data]);

  const supplierById = useMemo(
    () => new Map((data?.suppliers ?? []).map((item) => [item.id, item])),
    [data],
  );

  const materialById = useMemo(
    () => new Map((data?.materials ?? []).map((item) => [item.id, item])),
    [data],
  );

  const batchesByMaterial = useMemo(() => {
    const map = new Map<string, Batch[]>();

    for (const batch of data?.batches ?? []) {
      const rows = map.get(batch.material_id) ?? [];
      rows.push(batch);
      map.set(batch.material_id, rows);
    }

    return map;
  }, [data]);

  const summary = useMemo(() => {
    if (!data) return null;

    let value = 0;
    let belowMin = 0;

    for (const row of data.stock) {
      value += Number(row.on_hand) * Number(row.last_cost);
      if (row.below_min) belowMin += 1;
    }

    let quantity = 0;
    let age = 0;
    let expiringBatches = 0;

    for (const batch of data.batches) {
      const remaining = Number(batch.qty_remaining);

      if (remaining <= 0) continue;

      quantity += remaining;
      age += remaining * Math.max(0, -daysUntil(batch.received_at));

      if (batch.expires_at && daysUntil(batch.expires_at) <= 7) {
        expiringBatches += 1;
      }
    }

    return {
      value,
      belowMin,
      expiringBatches,
      averageAge: quantity > 0 ? Math.round(age / quantity) : null,
    };
  }, [data]);

  const counts = useMemo(() => {
    const stock = data?.stock ?? [];

    return {
      all: stock.length,
      below_min: stock.filter((row) => row.below_min).length,
      expiring: stock.filter(
        (row) => row.nearest_expiry !== null && daysUntil(row.nearest_expiry) <= 7,
      ).length,
      empty: stock.filter((row) => row.active_batches === 0).length,
    };
  }, [data]);

  const visible = useMemo(() => {
    if (!data) return [];

    const term = query.trim().toLowerCase();

    return data.stock.filter((row) => {
      if (term && !row.name.toLowerCase().includes(term)) return false;

      switch (filter) {
        case "below_min":
          return row.below_min;
        case "expiring":
          return (
            row.nearest_expiry !== null && daysUntil(row.nearest_expiry) <= 7
          );
        case "empty":
          return row.active_batches === 0;
        default:
          return true;
      }
    });
  }, [data, filter, query]);

  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canWriteBatches = permissions.includes("batch_write");
  const canWriteMaterials = permissions.includes("material_write");

  const panelRow =
    data?.stock.find((row) => row.material_id === panelId) ?? null;

  function supplierName(id: string | null): string {
    if (!id) return t.stock.meta.noSupplier;
    return supplierById.get(id)?.name ?? t.stock.meta.noSupplier;
  }

  function expiryLabel(row: StockLevel): string {
    if (!row.nearest_expiry) return "—";

    const days = daysUntil(row.nearest_expiry);
    const date = formatDate(row.nearest_expiry, locale);

    if (days < 0) {
      return `${date} · ${fill(t.stock.meta.daysOverdue, { days: Math.abs(days) })}`;
    }

    if (days === 0) return `${date} · ${t.stock.meta.today}`;

    return `${date} · ${fill(t.stock.meta.daysLeft, { days })}`;
  }

  function statusOf(row: StockLevel): { label: string; tone: Tone } {
    if (row.below_min) {
      return { label: t.stock.status.critical, tone: "danger" };
    }

    if (row.nearest_expiry) {
      const days = daysUntil(row.nearest_expiry);

      if (days < 0) {
        return { label: t.stock.status.expired, tone: "danger" };
      }

      if (days <= 7) {
        return {
          label: fill(t.stock.status.expiring, { days }),
          tone: "warning",
        };
      }
    }

    if (row.active_batches === 0) {
      return { label: t.stock.status.empty, tone: "neutral" };
    }

    return { label: t.stock.status.safe, tone: "brand" };
  }

  const batchCount = data?.batches.length ?? 0;

  const filterTabs: { key: Filter; label: string; short: string }[] = [
    { key: "all", label: t.stock.filters.all, short: t.stock.mobileFilters.all },
    {
      key: "below_min",
      label: t.stock.filters.belowMin,
      short: t.stock.mobileFilters.belowMin,
    },
    {
      key: "expiring",
      label: t.stock.filters.expiring,
      short: t.stock.mobileFilters.expiring,
    },
    { key: "empty", label: t.stock.filters.empty, short: t.stock.mobileFilters.empty },
  ];

  const subtitle = summary
    ? fill(
        data!.stock.length === 1 && batchCount === 1
          ? t.stock.subtitleSingle
          : t.stock.subtitle,
        {
          materials: data!.stock.length,
          batches: batchCount,
          value: formatMoney(summary.value, locale),
        },
      )
    : "";

  function closeStockIn() {
    setStockInFor(undefined);
  }

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
        <AppShell active="stock">
          <div className="flex max-w-[1200px] flex-col gap-4">
            <LoadingRegion label={t.stock.loading}>
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
        <AppShell me={session.me} active="stock" onSignOut={signOut}>
          <div className="flex max-w-[1200px] flex-col gap-4">
            <header className="flex flex-col items-start gap-4 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.stock.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>
              </div>
              <div className="flex w-full flex-wrap items-center gap-2.5 sm:w-auto">
                {canWriteMaterials && (
                  <Button
                    variant="ghost"
                    className="flex-1 sm:flex-none"
                    type="button"
                    onClick={() => setMaterialOpen(true)}
                  >
                    <Plus size={15} />
                    {t.stock.newMaterial}
                  </Button>
                )}
                {canWriteBatches && (
                  <Button
                    className="flex-1 sm:flex-none"
                    type="button"
                    onClick={() => setStockInFor(null)}
                  >
                    <Plus size={15} />
                    {t.stock.stockIn}
                  </Button>
                )}
              </div>
            </header>

            {summary && (
              <section
                aria-label={t.stock.title}
                className="grid grid-cols-2 gap-y-3 rounded-xl border border-border bg-card px-2 py-2 sm:grid-cols-4 sm:py-3.5"
              >
                <Metric label={t.stock.summary.value}>
                  {formatMoney(summary.value, locale)}
                </Metric>
                <Metric
                  label={t.stock.summary.belowMin}
                  tone={summary.belowMin > 0 ? "danger" : undefined}
                >
                  {fill(t.stock.summary.materials, { count: summary.belowMin })}
                </Metric>
                <Metric
                  label={t.stock.summary.expiring}
                  tone={summary.expiringBatches > 0 ? "warning" : undefined}
                >
                  {fill(t.stock.summary.batches, {
                    count: summary.expiringBatches,
                  })}
                </Metric>
                <Metric label={t.stock.summary.avgAge}>
                  {summary.averageAge === null
                    ? t.stock.summary.empty
                    : fill(t.stock.summary.days, { count: summary.averageAge })}
                </Metric>
              </section>
            )}

            {/* The queue itself comes and goes, so the count is announced from a
                region that is always in the tree. */}
            <p role="status" className="sr-only">
              {queue.length > 0
                ? fill(t.stock.offline.title, { count: queue.length })
                : null}
            </p>

            {queue.length > 0 && (
              <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex flex-col gap-0.5">
                    <strong className="text-sm font-bold">
                      {fill(t.stock.offline.title, { count: queue.length })}
                    </strong>
                    <span className="text-xs text-muted-foreground">
                      {syncing ? t.stock.offline.syncing : t.stock.offline.body}
                    </span>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    type="button"
                    onClick={() => void syncNow()}
                    disabled={syncing || !navigator.onLine}
                  >
                    {t.stock.offline.sync}
                  </Button>
                </div>

                <ul className="flex list-none flex-col gap-1.5 p-0">
                  {queue.map((entry) => {
                    const material = data?.materials.find(
                      (item) => item.id === entry.payload.material_id,
                    );

                    return (
                      <li
                        className={`flex items-center justify-between gap-2.5 rounded-lg px-2.5 py-2 ${
                          entry.status === "failed"
                            ? "bg-danger-soft"
                            : "bg-surface-alt"
                        }`}
                        key={entry.client_ref}
                      >
                        <div className="flex min-w-0 flex-col gap-px">
                          <strong
                            className="truncate text-xs font-bold"
                            title={material?.name ?? entry.payload.material_id}
                          >
                            {material?.name ?? entry.payload.material_id}
                          </strong>
                          <span className="text-2xs text-ink-3 tabular-nums">
                            {formatQty(entry.payload.qty_received, locale)}{" "}
                            {material?.base_unit.code ?? ""} ·{" "}
                            {formatMoney(
                              Number(entry.payload.unit_cost),
                              locale,
                            )}
                          </span>
                        </div>

                        {/* gap-4 keeps the two 44px hit areas apart. */}
                        {entry.status === "failed" ? (
                          <div className="flex items-center gap-4">
                            <Badge variant="danger">
                              {offlineReason(t, entry.code)}
                            </Badge>
                            <Button
                              variant="ghost"
                              size="iconSm"
                              type="button"
                              onClick={() => void retryQueued(entry.client_ref)}
                              aria-label={fill(t.stock.offline.retryNamed, {
                                name:
                                  material?.name ?? entry.payload.material_id,
                              })}
                            >
                              <RotateCcw size={14} />
                            </Button>
                            <Button
                              variant="ghost"
                              size="iconSm"
                              type="button"
                              onClick={() =>
                                void dismissQueued(entry.client_ref)
                              }
                              aria-label={fill(t.stock.offline.dismissNamed, {
                                name:
                                  material?.name ?? entry.payload.material_id,
                              })}
                            >
                              <X size={14} />
                            </Button>
                          </div>
                        ) : (
                          <Badge variant="warning">
                            {t.stock.offline.pending}
                          </Badge>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}

            <StatusMessage
              message={notice}
              className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
            />

            {load.status === "loading" && (
              <LoadingRegion label={t.stock.loading}>
                <div className="rounded-xl border border-border bg-card p-5">
                  <SkeletonTable rows={6} columns={5} />
                </div>
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">{t.stock.error}</p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.stock.retry}
                </Button>
              </div>
            )}

            {data && (
              <section className="overflow-hidden rounded-xl border border-border bg-card">
                {/* flex-wrap lets the search drop below the tabs whenever the
                    row cannot fit; the tab labels and their count pills set a
                    minimum width that no single breakpoint can cover. */}
                <div className="flex flex-col items-stretch gap-3 border-b border-border px-4 py-3 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between">
                  <div
                    role="tablist"
                    aria-label={t.stock.title}
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
                          id={`stock-tab-${tab.key}`}
                          aria-selected={active}
                          aria-controls="stock-panel"
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
                      placeholder={t.stock.searchPlaceholder}
                      aria-label={t.stock.searchPlaceholder}
                      className="w-full cursor-text border-0 bg-transparent text-[1rem] text-ink outline-none placeholder:text-ink-3 sm:text-sm"
                    />
                  </label>
                </div>

                <div
                  role="tabpanel"
                  id="stock-panel"
                  aria-labelledby={`stock-tab-${filter}`}
                >
                  {data.stock.length === 0 ? (
                    <div className="flex flex-col items-center gap-2 px-5 py-11 text-center">
                      <h2 className="text-lg font-bold">
                        {t.stock.empty.title}
                      </h2>
                      <p className="text-sm text-muted-foreground">
                        {t.stock.empty.body}
                      </p>
                      {canWriteMaterials && (
                        <div className="mt-1 flex flex-wrap justify-center gap-2">
                          <Button
                            type="button"
                            onClick={() => setMaterialOpen(true)}
                          >
                            {t.stock.empty.cta}
                          </Button>
                          <Button variant="ghost" asChild>
                            <Link href="/onboarding">
                              {t.stock.empty.template}
                            </Link>
                          </Button>
                        </div>
                      )}
                    </div>
                  ) : visible.length === 0 ? (
                    <p className="px-4 py-6 text-sm text-muted-foreground">
                      {t.stock.empty.filtered}
                    </p>
                  ) : (
                    <div role="table" aria-label={t.stock.title} className="@container">
                      {/* A container, not a breakpoint: the seven fixed columns
                          need 612px, the seven gaps 84px and the row inset 32px,
                          so the name column only gets room once this box passes
                          728px. Switching on there leaves it 0px wide, so the
                          bar sits at 878px, which guarantees the name 150px. A
                          viewport breakpoint would clip again once the 246px
                          sidebar appears at 961px. */}
                      {/* The mobile grid shows three columns, so it needs its
                          own header row; the desktop grid keeps the full one. */}
                      <div
                        role="row"
                        className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 border-b border-border bg-surface-alt px-3.5 py-2 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[878px]:hidden"
                      >
                        <span role="columnheader">
                          {t.stock.table.material}
                        </span>
                        <span role="columnheader">{t.stock.table.stock}</span>
                        <span role="columnheader">{t.stock.table.status}</span>
                      </div>

                      <div
                        role="row"
                        className="hidden items-center gap-3 bg-surface-alt px-4 py-2.5 text-2xs font-bold tracking-wider text-ink-3 uppercase @min-[878px]:grid @min-[878px]:grid-cols-[minmax(0,1fr)_110px_70px_62px_130px_96px_104px_40px]"
                      >
                        <span role="columnheader">
                          {t.stock.table.material}
                        </span>
                        <span role="columnheader">{t.stock.table.stock}</span>
                        <span role="columnheader">{t.stock.table.min}</span>
                        <span role="columnheader">
                          {t.stock.table.batches}
                        </span>
                        <span role="columnheader">
                          {t.stock.table.nearestExpiry}
                        </span>
                        <span role="columnheader">
                          {t.stock.table.status}
                        </span>
                        <span role="columnheader">{t.stock.table.value}</span>
                      </div>

                      {visible.map((row) => {
                        const status = statusOf(row);
                        const supplier = supplierName(
                          materialById.get(row.material_id)
                            ?.default_supplier_id ?? null,
                        );

                        return (
                          <div
                            role="row"
                            key={row.material_id}
                            className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 border-b border-border px-3.5 py-3 text-sm last:border-b-0 @min-[878px]:grid-cols-[minmax(0,1fr)_110px_70px_62px_130px_96px_104px_40px] @min-[878px]:gap-3 @min-[878px]:px-4"
                          >
                            <div
                              role="cell"
                              className="flex min-w-0 flex-col gap-px"
                            >
                              <button
                                type="button"
                                className="cursor-pointer text-left text-sm font-semibold text-ink transition-colors duration-150 hover:text-brand"
                                onClick={() => setPanelId(row.material_id)}
                              >
                                {row.name}
                              </button>
                              <span
                                className="truncate text-2xs text-ink-3"
                                title={supplier}
                              >
                                {supplier}
                              </span>
                            </div>

                            <div
                              role="cell"
                              className="flex items-baseline justify-self-end gap-1 @min-[878px]:justify-self-auto"
                            >
                              <strong className="text-sm font-bold tabular-nums">
                                {formatQty(row.on_hand, locale)}
                              </strong>
                              <span className="text-xs text-ink-3">
                                {row.base_unit.code}
                              </span>
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-3 @min-[878px]:block"
                            >
                              {formatQty(row.min_stock, locale)}{" "}
                              {row.base_unit.code}
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-2 @min-[878px]:block"
                            >
                              {row.active_batches}
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs text-ink-2 @min-[878px]:block"
                            >
                              {expiryLabel(row)}
                            </div>

                            <div role="cell">
                              <Badge variant={status.tone}>
                                {status.label}
                              </Badge>
                            </div>

                            <div
                              role="cell"
                              className="hidden text-xs font-semibold tabular-nums @min-[878px]:block"
                            >
                              {formatMoney(
                                Number(row.on_hand) * Number(row.last_cost),
                                locale,
                              )}
                            </div>

                            <div
                              role="cell"
                              className="hidden justify-self-end @min-[878px]:block"
                            >
                              <Button
                                variant="ghost"
                                size="iconSm"
                                type="button"
                                onClick={() => setPanelId(row.material_id)}
                                aria-label={t.stock.panel.title}
                              >
                                <Ellipsis size={15} />
                              </Button>
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

          {panelRow && (
            <BatchPanel
              material={panelRow}
              batches={batchesByMaterial.get(panelRow.material_id) ?? []}
              suppliers={data?.suppliers ?? []}
              canStockIn={canWriteBatches}
              onClose={() => setPanelId(null)}
              onStockIn={() => {
                const id = panelRow.material_id;
                setPanelId(null);
                setStockInFor(id);
              }}
            />
          )}

          {stockInFor !== undefined && (
            <StockInDialog
              materials={data?.materials ?? []}
              suppliers={data?.suppliers ?? []}
              defaultMaterialId={stockInFor}
              onClose={closeStockIn}
              onSaved={() => {
                closeStockIn();
                setNotice(t.stock.saved);
                void fetchData();
              }}
              onQueued={() => {
                closeStockIn();
                setNotice(t.stock.offline.queued);
                void refreshQueue();
              }}
            />
          )}

          {materialOpen && (
            <MaterialDialog
              units={data?.units ?? []}
              suppliers={data?.suppliers ?? []}
              onClose={() => setMaterialOpen(false)}
              onCreated={() => {
                setMaterialOpen(false);
                setNotice(t.stock.materialCreated);
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

function Metric({
  label,
  tone,
  children,
}: {
  label: string;
  tone?: "danger" | "warning";
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-0.5 px-3 sm:border-l sm:border-border sm:px-4 sm:first:border-l-0">
      <span className="text-2xs font-semibold tracking-wide text-ink-3">
        {label}
      </span>
      <strong
        className={`text-base font-bold tabular-nums ${
          tone === "danger"
            ? "text-destructive"
            : tone === "warning"
              ? "text-warning"
              : ""
        }`}
      >
        {children}
      </strong>
    </div>
  );
}

function offlineReason(t: Dictionary, code: string | null): string {
  switch (code) {
    case "material_not_found":
      return t.stock.offline.reasons.materialMissing;
    case "invalid_quantity":
      return t.stock.offline.reasons.invalidQuantity;
    case "invalid_cost":
      return t.stock.offline.reasons.invalidCost;
    case "invalid_supplier":
      return t.stock.offline.reasons.invalidSupplier;
    case "permission_denied":
      return t.stock.offline.reasons.forbidden;
    default:
      return t.stock.offline.reasons.generic;
  }
}
