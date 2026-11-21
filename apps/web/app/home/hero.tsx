"use client";

import Link from "next/link";

import { Button } from "@/components/ui/button";
import { formatDate, formatQty } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";

import { HOME_ICONS, Wheat } from "./icons";

/**
 * One chunk of the hero, entering `delay` milliseconds after the page does.
 * The animation itself lives in CSS, so it runs even if JavaScript is slow to
 * arrive — the first thing a visitor sees should not wait on a download.
 */
function Enter({
  children,
  delay,
}: {
  children: React.ReactNode;
  delay: number;
}) {
  return (
    <div className="home-enter" style={{ animationDelay: `${delay}ms` }}>
      {children}
    </div>
  );
}

/**
 * The batch ledger as it actually works: one material, several batches, and
 * the one closest to expiry marked as the one FEFO takes first. It is the
 * product's core idea in one picture, drawn with the same tokens the app uses.
 *
 * Decorative, so it is hidden from assistive technology: the value it
 * illustrates is already in the headline and the sections below.
 */
function Preview() {
  const { t, locale } = useI18n();
  const preview = t.home.hero.preview;
  const Bell = HOME_ICONS.bell;

  return (
    <div aria-hidden="true" className="flex flex-col gap-3">
      <div className="flex flex-col gap-3 rounded-2xl border border-border bg-surface p-4">
        <div className="flex items-center gap-2.5">
          <span className="flex size-8 items-center justify-center rounded-lg bg-brand-soft text-brand">
            <Wheat size={15} strokeWidth={2.2} />
          </span>
          <strong className="text-base font-bold">{preview.title}</strong>
          <span className="ml-auto rounded-full border border-border px-2.5 py-1 text-2xs font-semibold text-ink-2">
            {fill(preview.chip, { count: preview.batches.length })}
          </span>
        </div>

        <ul className="flex flex-col gap-1.5">
          {preview.batches.map((batch, index) => (
            <li
              key={batch.expiry ?? "no-expiry"}
              className={
                // The rows are ordered by expiry, so the first one is the one
                // FEFO takes.
                index === 0
                  ? "flex items-center gap-2.5 rounded-lg bg-brand-soft px-3 py-2.5"
                  : "flex items-center gap-2.5 rounded-lg px-3 py-2"
              }
            >
              <span className="text-sm text-ink-2">
                {batch.expiry
                  ? formatDate(batch.expiry, locale)
                  : preview.noExpiry}
              </span>
              {index === 0 && (
                <span className="rounded-full bg-brand px-2 py-0.5 text-2xs font-bold text-white">
                  {preview.useFirst}
                </span>
              )}
              <strong className="ml-auto text-sm font-bold tabular-nums">
                {formatQty(batch.qty, locale)}{" "}
                <span className="font-semibold text-ink-3">{preview.unit}</span>
              </strong>
            </li>
          ))}
        </ul>

        <div className="flex items-center gap-2.5 border-t border-dashed border-border pt-3">
          <span className="flex size-7 items-center justify-center rounded-md bg-warning-soft text-warning">
            <Bell size={14} strokeWidth={2.2} />
          </span>
          <span className="flex min-w-0 flex-col">
            <strong className="text-xs font-bold">{preview.alertTitle}</strong>
            <span className="text-2xs text-ink-3">{preview.alertDetail}</span>
          </span>
        </div>
      </div>
    </div>
  );
}

export function Hero() {
  const { t } = useI18n();
  const hero = t.home.hero;

  return (
    <section className="grid gap-10 py-12 sm:py-16 lg:grid-cols-[minmax(0,1fr)_minmax(0,420px)] lg:items-center lg:gap-14">
      <div className="flex flex-col gap-5">
        <Enter delay={0}>
          <p className="m-0 text-2xs font-bold tracking-[0.12em] text-brand uppercase">
            {hero.eyebrow}
          </p>
        </Enter>

        <Enter delay={100}>
          <h1 className="m-0 text-4xl leading-[1.08] font-bold tracking-[-0.03em] sm:text-5xl">
            {hero.title}
          </h1>
        </Enter>

        <Enter delay={200}>
          <p className="m-0 max-w-[54ch] text-base leading-[1.6] text-ink-2 sm:text-lg">
            {hero.subtitle}
          </p>
        </Enter>

        <Enter delay={300}>
          <div className="mt-1 flex flex-col gap-2.5 sm:flex-row sm:items-center">
            <Button size="lg" asChild className="w-full sm:w-auto">
              <Link href="/login">{hero.primaryCta}</Link>
            </Button>
            <Button
              size="lg"
              variant="outline"
              asChild
              className="w-full sm:w-auto"
            >
              <Link href="#pricing">{hero.secondaryCta}</Link>
            </Button>
          </div>
        </Enter>

        <Enter delay={400}>
          <p className="m-0 text-xs leading-[1.5] text-ink-3">{hero.note}</p>
        </Enter>
      </div>

      <Enter delay={250}>
        <Preview />
      </Enter>
    </section>
  );
}
