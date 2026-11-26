"use client";

import { ApiError, type BusinessReport } from "@nata/api-client";
import { BarChart3, Download, Lock } from "lucide-react";
import { useEffect, useState } from "react";

import {
  LoadingRegion,
  SkeletonList,
  SkeletonStats,
} from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import { downloadCsv } from "@/lib/csv";
import {
  formatDate,
  formatDateYear,
  formatMoney,
  formatPercent,
  formatQty,
  todayIso,
} from "@/lib/format";
import { reportCsv } from "@/lib/report-csv";
import { downloadReportPdf, reportPdf } from "@/lib/report-pdf";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load =
  | { status: "loading" }
  | { status: "ready"; data: BusinessReport }
  | { status: "error" };

/** The period the picker sends, as `YYYY-MM-DD` strings. */
type ChosenRange = { from: string; to: string };

/** Matches `MAX_RANGE_DAYS` on the server, so a refusal is rare. */
const MAX_RANGE_DAYS = 366;

type RangeError = "invalid" | "tooLong";

/** Client-side only: the server still refuses a bad range on its own. */
function validateRange(from: string, to: string): RangeError | null {
  if (!from || !to || from > to) return "invalid";

  const days =
    Math.round((Date.parse(to) - Date.parse(from)) / 86_400_000) + 1;

  return days > MAX_RANGE_DAYS ? "tooLong" : null;
}

export default function ReportsPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [range, setRange] = useState<ChosenRange | null>(null);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [rangeError, setRangeError] = useState<RangeError | null>(null);

  useEffect(() => {
    if (session.status !== "ready") return;

    const token = readToken();

    if (!token) return;

    let active = true;

    api
      .getBusinessReport(
        token,
        range ? { from: range.from, to: range.to } : undefined,
      )
      .then((data) => {
        if (active) {
          setRangeError(null);
          setLoad({ status: "ready", data });
        }
      })
      .catch((cause: unknown) => {
        if (!active) return;

        // A range the server will not serve falls back to the tenant's own
        // window, so the screen keeps its numbers and the picker says why.
        if (
          range &&
          cause instanceof ApiError &&
          (cause.code === "invalid_range" || cause.code === "range_too_long")
        ) {
          setRangeError(
            cause.code === "range_too_long" ? "tooLong" : "invalid",
          );
          setRange(null);
          setFrom("");
          setTo("");
          return;
        }

        setLoad({ status: "error" });
      });

    return () => {
      active = false;
    };
  }, [session.status, range]);

  const data = load.status === "ready" ? load.data : null;

  const maxRevenue = data
    ? Math.max(0, ...data.trend.map((day) => Number(day.revenue)))
    : 0;

  function applyRange() {
    const problem = validateRange(from, to);

    if (problem) {
      setRangeError(problem);
      return;
    }

    setRangeError(null);
    setRange({ from, to });
  }

  function resetRange() {
    setRange(null);
    setFrom("");
    setTo("");
    setRangeError(null);
  }

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="reports">
          <div className="flex max-w-[1100px] flex-col gap-4">
            <LoadingRegion
              label={t.reports.loading}
              className="flex flex-col gap-4"
            >
              <SkeletonStats count={3} />
              <SkeletonList rows={3} />
            </LoadingRegion>
          </div>
        </AppShell>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[1100px] p-6">
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
        <AppShell me={session.me} active="reports" onSignOut={signOut}>
          <div className="flex max-w-[1100px] flex-col gap-4">
            <header className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.reports.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  {data
                    ? fill(t.reports.subtitle, {
                        from: formatDateYear(data.range.from, locale),
                        to: formatDateYear(data.range.to, locale),
                      })
                    : t.reports.loading}
                </p>

                {data && !data.can_export && (
                  <p className="mt-1 text-xs text-ink-3">
                    {t.reports.export.locked}
                  </p>
                )}
              </div>

              {data && (
                <div className="flex flex-wrap gap-2 self-start">
                  <Button
                    variant="outline"
                    size="sm"
                    type="button"
                    disabled={!data.can_export}
                    title={data.can_export ? undefined : t.reports.export.locked}
                    onClick={() =>
                      downloadCsv(
                        `nata-report-${todayIso()}.csv`,
                        reportCsv(data, t.reports, new Date()),
                      )
                    }
                  >
                    <Download size={14} />
                    {t.reports.export.button}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    type="button"
                    disabled={!data.can_export}
                    title={data.can_export ? undefined : t.reports.export.locked}
                    onClick={() =>
                      void downloadReportPdf(
                        `nata-report-${todayIso()}.pdf`,
                        reportPdf(data, t.reports, locale, new Date()),
                      )
                    }
                  >
                    <Download size={14} />
                    {t.reports.export.pdfButton}
                  </Button>
                </div>
              )}
            </header>

            <div className="flex flex-wrap items-end gap-2 rounded-xl border border-border bg-card px-4 py-3">
              <div className="flex flex-col gap-1">
                <Label
                  htmlFor="report-from"
                  className="text-2xs font-semibold text-ink-3"
                >
                  {t.reports.range.from}
                </Label>
                <Input
                  id="report-from"
                  type="date"
                  className="w-[160px]"
                  value={from}
                  max={to || undefined}
                  onChange={(event) => setFrom(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1">
                <Label
                  htmlFor="report-to"
                  className="text-2xs font-semibold text-ink-3"
                >
                  {t.reports.range.to}
                </Label>
                <Input
                  id="report-to"
                  type="date"
                  className="w-[160px]"
                  value={to}
                  min={from || undefined}
                  onChange={(event) => setTo(event.target.value)}
                />
              </div>

              <Button
                size="sm"
                type="button"
                onClick={applyRange}
                disabled={!from || !to}
              >
                {t.reports.range.apply}
              </Button>

              {range && (
                <Button
                  variant="ghost"
                  size="sm"
                  type="button"
                  onClick={resetRange}
                >
                  {fill(t.reports.range.reset, {
                    days: data?.window_days ?? 30,
                  })}
                </Button>
              )}

              {rangeError && (
                <p className="basis-full text-2xs text-warning">
                  {rangeError === "tooLong"
                    ? fill(t.reports.range.tooLong, { days: MAX_RANGE_DAYS })
                    : t.reports.range.invalid}
                </p>
              )}
            </div>

            {load.status === "loading" && (
              <LoadingRegion
                label={t.reports.loading}
                className="flex flex-col gap-4"
              >
                <SkeletonStats count={3} />
                <SkeletonList rows={3} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.reports.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.reports.retry}
                </Button>
              </div>
            )}

            {data && (
              <>
                <section className="flex flex-col gap-2.5">
                  <div>
                    <h2 className="text-base font-bold">
                      {t.reports.profitLoss.title}
                    </h2>
                    <p className="mt-0.5 max-w-[72ch] text-xs leading-relaxed text-ink-3">
                      {t.reports.profitLoss.note}
                    </p>
                  </div>

                  <article className="flex flex-col gap-1.5 rounded-xl border border-border bg-card px-4.5 py-4">
                    <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                      {t.reports.profitLoss.grossProfit}
                    </span>
                    <strong
                      className={`text-2xl font-extrabold tracking-tight tabular-nums ${
                        data.profit_loss.gross_profit !== null &&
                        Number(data.profit_loss.gross_profit) < 0
                          ? "text-destructive"
                          : ""
                      }`}
                    >
                      {data.profit_loss.gross_profit === null
                        ? "—"
                        : formatMoney(
                            Number(data.profit_loss.gross_profit),
                            locale,
                          )}
                    </strong>

                    <dl className="mt-1.5 flex flex-col gap-1.5">
                      <div className="flex items-baseline justify-between gap-3 border-t border-border pt-1.5">
                        <dt className="text-2xs text-ink-3">
                          {t.reports.profitLoss.revenue}
                        </dt>
                        <dd className="m-0 text-base tabular-nums">
                          {formatMoney(Number(data.profit_loss.revenue), locale)}
                        </dd>
                      </div>
                      <div className="flex items-baseline justify-between gap-3">
                        <dt className="text-2xs text-ink-3">
                          {t.reports.profitLoss.costOfGoods}
                        </dt>
                        <dd className="m-0 text-base tabular-nums">
                          {data.profit_loss.cost_of_goods === null
                            ? "—"
                            : formatMoney(
                                Number(data.profit_loss.cost_of_goods),
                                locale,
                              )}
                        </dd>
                      </div>
                    </dl>

                    {data.profit_loss.cost_incomplete && (
                      <p className="text-2xs leading-relaxed text-warning">
                        {t.reports.profitLoss.incomplete}
                      </p>
                    )}
                  </article>
                </section>

                {data.can_advanced ? (
                  <section className="flex flex-col gap-2.5">
                    <div>
                      <h2 className="text-base font-bold">
                        {t.reports.trend.title}
                      </h2>
                      <p className="mt-0.5 max-w-[72ch] text-xs leading-relaxed text-ink-3">
                        {t.reports.trend.note}
                      </p>
                    </div>

                    {data.trend.length === 0 ? (
                      <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center text-ink-3">
                        <BarChart3 size={24} />
                        <p className="text-sm">{t.reports.trend.empty}</p>
                      </div>
                    ) : (
                      <div className="rounded-xl border border-border bg-card px-4 py-4">
                        <div
                          className="flex h-36 min-w-[560px] items-end gap-[2px]"
                          role="img"
                          aria-label={t.reports.trend.title}
                        >
                          {data.trend.map((day) => {
                            const revenue = Number(day.revenue);
                            const height = maxRevenue
                              ? (revenue / maxRevenue) * 100
                              : 0;

                            return (
                              <div
                                key={day.date}
                                className="min-w-[6px] flex-1 rounded-t-[2px] bg-brand"
                                style={{ height: `${height}%` }}
                                title={`${formatDate(day.date, locale)} · ${formatMoney(revenue, locale)}`}
                              />
                            );
                          })}
                        </div>

                        <div className="mt-2 flex items-baseline justify-between gap-3 text-2xs text-ink-3">
                          <span>{formatDate(data.range.from, locale)}</span>
                          <span>{formatDate(data.range.to, locale)}</span>
                        </div>
                      </div>
                    )}
                  </section>
                ) : null}

                <section className="grid gap-3 lg:grid-cols-2">
                  <article className="flex flex-col gap-1.5 rounded-xl border border-border bg-card px-4.5 py-4">
                    <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                      {t.reports.stock.title}
                    </span>
                    <strong className="text-2xl font-extrabold tracking-tight tabular-nums">
                      {formatMoney(Number(data.stock.total), locale)}
                    </strong>

                    <dl className="mt-1.5 flex gap-5.5">
                      <div className="flex flex-col gap-px">
                        <dt className="text-2xs text-ink-3">
                          {t.reports.stock.materials}
                        </dt>
                        <dd className="m-0 text-base tabular-nums">
                          {formatMoney(Number(data.stock.materials), locale)}
                        </dd>
                      </div>
                      <div className="flex flex-col gap-px">
                        <dt className="text-2xs text-ink-3">
                          {t.reports.stock.products}
                        </dt>
                        <dd className="m-0 text-base tabular-nums">
                          {formatMoney(Number(data.stock.products), locale)}
                        </dd>
                      </div>
                    </dl>
                  </article>

                  <article className="flex flex-col gap-1.5 rounded-xl border border-border bg-card px-4.5 py-4">
                    <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                      {t.reports.waste.title}
                    </span>
                    <strong className="text-2xl font-extrabold tracking-tight tabular-nums">
                      {formatMoney(Number(data.waste.value), locale)}
                    </strong>
                    <p className="text-xs text-ink-3">
                      {fill(t.reports.waste.movements, {
                        count: data.waste.movements,
                      })}
                    </p>

                    {data.waste.by_material.length > 0 ? (
                      <ul className="mt-1.5 flex list-none flex-col gap-1 p-0">
                        {data.waste.by_material.map((row) => (
                          <li
                            key={row.material_id}
                            className="grid grid-cols-[minmax(0,1fr)_auto_90px] items-baseline gap-2.5 border-t border-border pt-1 text-sm"
                          >
                            <span
                              className="truncate"
                              title={row.material_name}
                            >
                              {row.material_name}
                            </span>
                            <span className="text-xs text-ink-3 tabular-nums">
                              {formatQty(row.qty, locale)} {row.unit_code}
                            </span>
                            <strong className="text-right tabular-nums">
                              {formatMoney(Number(row.value), locale)}
                            </strong>
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="text-xs text-ink-3">
                        {t.reports.waste.empty}
                      </p>
                    )}
                  </article>
                </section>

                {!data.can_advanced && (
                  <div className="flex flex-col items-start gap-2.5 rounded-xl border border-dashed border-border bg-card px-5 py-4">
                    <span className="inline-flex items-center gap-2 text-sm font-semibold">
                      <Lock size={14} className="text-ink-3" />
                      {t.reports.advanced.locked}
                    </span>
                    <div className="flex flex-wrap gap-1.5">
                      <Badge variant="outline">
                        {t.reports.trend.title}
                      </Badge>
                      <Badge variant="outline">
                        {t.reports.usage.title}
                      </Badge>
                      <Badge variant="outline">
                        {t.reports.purchasing.title}
                      </Badge>
                    </div>
                  </div>
                )}

                {data.can_advanced && (
                  <section className="flex flex-col gap-2.5">
                    <div>
                      <h2 className="text-base font-bold">
                        {t.reports.usage.title}
                      </h2>
                      <p className="mt-0.5 max-w-[72ch] text-xs leading-relaxed text-ink-3">
                        {t.reports.usage.note}
                      </p>
                    </div>

                    {data.usage.length === 0 ? (
                      <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center text-ink-3">
                        <BarChart3 size={24} />
                        <p className="text-sm">{t.reports.usage.empty}</p>
                      </div>
                    ) : (
                      <div className="overflow-hidden rounded-xl border border-border bg-card">
                        <Table className="min-w-[640px]">
                          <TableHeader>
                            <TableRow className="border-border">
                              <TableHead>{t.reports.usage.material}</TableHead>
                              <TableHead className="text-right">
                                {t.reports.usage.qty}
                              </TableHead>
                              <TableHead className="text-right">
                                {t.reports.usage.unitCost}
                              </TableHead>
                              <TableHead className="text-right">
                                {t.reports.usage.value}
                              </TableHead>
                              <TableHead className="text-right">
                                {t.reports.usage.runs}
                              </TableHead>
                            </TableRow>
                          </TableHeader>
                          <TableBody>
                            {data.usage.map((row) => (
                              <TableRow
                                key={row.material_id}
                                className="border-border"
                              >
                                <TableCell className="whitespace-normal">
                                  <strong className="text-sm font-semibold">
                                    {row.material_name}
                                  </strong>
                                  <span className="ml-2 text-2xs text-ink-3">
                                    {row.unit_code}
                                  </span>
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {formatQty(row.qty, locale)}
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {formatMoney(Number(row.unit_cost), locale)}
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {formatMoney(Number(row.value), locale)}
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {row.runs}
                                </TableCell>
                              </TableRow>
                            ))}
                          </TableBody>
                        </Table>
                      </div>
                    )}
                  </section>
                )}

                {data.can_advanced && (
                  <section className="flex flex-col gap-2.5">
                    <div>
                      <h2 className="text-base font-bold">
                        {t.reports.purchasing.title}
                      </h2>
                      <p className="mt-0.5 max-w-[72ch] text-xs leading-relaxed text-ink-3">
                        {t.reports.purchasing.note}
                      </p>
                    </div>

                    {data.purchasing.length === 0 ? (
                      <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center text-ink-3">
                        <BarChart3 size={24} />
                        <p className="text-sm">
                          {t.reports.purchasing.empty}
                        </p>
                      </div>
                    ) : (
                      <div className="overflow-hidden rounded-xl border border-border bg-card">
                        <Table className="min-w-[720px]">
                          <TableHeader>
                            <TableRow className="border-border">
                              <TableHead>
                                {t.reports.purchasing.supplier}
                              </TableHead>
                              <TableHead>
                                {t.reports.purchasing.material}
                              </TableHead>
                              <TableHead className="text-right">
                                {t.reports.purchasing.qty}
                              </TableHead>
                              <TableHead className="text-right">
                                {t.reports.purchasing.value}
                              </TableHead>
                              <TableHead className="text-right">
                                {t.reports.purchasing.batches}
                              </TableHead>
                            </TableRow>
                          </TableHeader>
                          <TableBody>
                            {data.purchasing.map((row) => (
                              <TableRow
                                key={`${row.supplier_id ?? "none"}-${row.material_id}`}
                                className="border-border"
                              >
                                <TableCell className="whitespace-normal">
                                  <strong className="text-sm font-semibold">
                                    {row.supplier_name ??
                                      t.reports.purchasing.noSupplier}
                                  </strong>
                                </TableCell>
                                <TableCell className="whitespace-normal">
                                  <span className="text-sm">
                                    {row.material_name}
                                  </span>
                                  <span className="ml-2 text-2xs text-ink-3">
                                    {row.unit_code}
                                  </span>
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {formatQty(row.qty, locale)}
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {formatMoney(Number(row.value), locale)}
                                </TableCell>
                                <TableCell className="text-right tabular-nums">
                                  {row.batches}
                                </TableCell>
                              </TableRow>
                            ))}
                          </TableBody>
                        </Table>
                      </div>
                    )}
                  </section>
                )}

                <section className="flex flex-col gap-2.5">
                  <div>
                    <h2 className="text-base font-bold">
                      {t.reports.products.title}
                    </h2>
                    <p className="mt-0.5 max-w-[72ch] text-xs leading-relaxed text-ink-3">
                      {t.reports.products.costNote}
                    </p>
                  </div>

                  {data.products.length === 0 ? (
                    <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center text-ink-3">
                      <BarChart3 size={24} />
                      <p className="text-sm">{t.reports.products.empty}</p>
                    </div>
                  ) : (
                    <div className="overflow-hidden rounded-xl border border-border bg-card">
                      <Table className="min-w-[720px]">
                        <TableHeader>
                          <TableRow className="border-border">
                            <TableHead>{t.reports.products.product}</TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.price}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.cost}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.unitMargin}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.sold}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.revenue}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.margin}
                            </TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {data.products.map((product) => (
                            <TableRow key={product.product_id}>
                              <TableCell className="whitespace-normal">
                                <span className="inline-flex flex-wrap items-center gap-2">
                                  <strong className="text-sm font-semibold">
                                    {product.name}
                                  </strong>
                                  {product.cost_incomplete && (
                                    <Badge variant="warning">
                                      {t.reports.products.incomplete}
                                    </Badge>
                                  )}
                                </span>
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {formatMoney(Number(product.sell_price), locale)}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {product.unit_cost ? (
                                  formatMoney(Number(product.unit_cost), locale)
                                ) : (
                                  <Badge>
                                    {t.reports.products.noCost}
                                  </Badge>
                                )}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {product.margin_per_unit ? (
                                  <span className="inline-flex items-baseline justify-end gap-1.5">
                                    {formatMoney(
                                      Number(product.margin_per_unit),
                                      locale,
                                    )}
                                    {product.margin_pct && (
                                      <span className="text-2xs text-ink-3">
                                        {formatPercent(
                                          product.margin_pct,
                                          locale,
                                        )}
                                      </span>
                                    )}
                                  </span>
                                ) : (
                                  "—"
                                )}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {formatQty(product.sold_qty, locale)}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {formatMoney(Number(product.revenue), locale)}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {product.margin
                                  ? formatMoney(Number(product.margin), locale)
                                  : "—"}
                              </TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </div>
                  )}
                </section>
              </>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}
