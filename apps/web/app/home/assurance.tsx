"use client";

import { useI18n } from "@/lib/i18n";

import { HOME_ICONS } from "./icons";

/**
 * Three assurances under the hero. They are the objections a first-time
 * visitor has — is my data separate, do I have to pay upfront, will I actually
 * hear about an expiry — answered without making them scroll.
 */
export function Assurance() {
  const { t } = useI18n();

  return (
    <section className="border-t border-border pt-10">
      <ul className="grid gap-6 sm:grid-cols-3 sm:gap-4">
        {t.home.assurance.items.map((item) => {
          const Icon = HOME_ICONS[item.icon];

          return (
            <li key={item.title} className="flex items-start gap-3">
              <span className="flex size-8 flex-none items-center justify-center rounded-lg bg-brand-soft text-brand">
                <Icon size={15} strokeWidth={2.2} aria-hidden="true" />
              </span>
              <span className="flex min-w-0 flex-col gap-0.5">
                <strong className="text-sm font-bold">{item.title}</strong>
                <span className="text-xs leading-[1.5] text-ink-2">
                  {item.detail}
                </span>
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
