"use client";

import Link from "next/link";
import { Check } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatMoney } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";

import { Section } from "./section";

/**
 * `Plan` order: trial, starter, pro, business. Pro is the recommended tier, so
 * it is the one filled button in this section.
 */
const POPULAR_INDEX = 2;

export function Pricing() {
  const { t, locale } = useI18n();
  const pricing = t.home.pricing;

  return (
    <Section
      id="pricing"
      eyebrow={pricing.eyebrow}
      title={pricing.title}
      subtitle={pricing.subtitle}
    >
      {/* `pt-3` leaves room for the badge that overlaps the popular card. */}
      <ul className="grid gap-3 pt-3 sm:grid-cols-2 lg:grid-cols-4">
        {pricing.items.map((plan, index) => {
          const popular = index === POPULAR_INDEX;

          return (
            <li
              key={plan.name}
              className={cn(
                "relative flex flex-col gap-4 rounded-2xl border bg-surface p-5",
                popular ? "border-brand" : "border-border",
              )}
            >
              {popular && (
                <span className="absolute -top-2.5 left-1/2 -translate-x-1/2 rounded-full bg-brand px-2.5 py-0.5 text-2xs font-bold whitespace-nowrap text-white">
                  {pricing.popular}
                </span>
              )}

              <div className="flex flex-col gap-1">
                <h3 className="m-0 text-lg font-bold">{plan.name}</h3>
                <p className="m-0 text-xs leading-[1.5] text-ink-3">
                  {plan.caption}
                </p>
              </div>

              <div className="flex items-baseline gap-1">
                <span className="text-3xl font-extrabold tracking-[-0.02em] tabular-nums">
                  {formatMoney(plan.price, locale)}
                </span>
                {plan.price > 0 && (
                  <span className="text-xs text-ink-3">{pricing.perMonth}</span>
                )}
              </div>

              <p className="m-0 text-xs leading-[1.5] text-ink-2">
                {plan.includes}
              </p>

              <ul className="flex flex-col gap-2 border-t border-dashed border-border pt-3">
                {plan.limits.map((limit) => (
                  <li
                    key={limit}
                    className="flex items-start gap-2 text-sm text-ink-2"
                  >
                    <Check
                      size={14}
                      strokeWidth={2.5}
                      className="mt-0.5 flex-none text-brand"
                      aria-hidden="true"
                    />
                    {limit}
                  </li>
                ))}
              </ul>

              <Button
                variant={popular ? "default" : "outline"}
                className="mt-auto w-full"
                asChild
              >
                <Link href="/login">
                  {index === 0
                    ? pricing.trialCta
                    : fill(pricing.planCta, { name: plan.name })}
                </Link>
              </Button>
            </li>
          );
        })}
      </ul>

      <p className="m-0 text-xs leading-[1.5] text-ink-3">
        {pricing.annualNote}
      </p>
    </Section>
  );
}
