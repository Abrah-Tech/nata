"use client";

import { useI18n } from "@/lib/i18n";

import { HOME_ICONS } from "./icons";
import { Section } from "./section";

/**
 * Six capabilities, each with the problem it removes. The cards are content,
 * not controls, so they carry no hover treatment: nothing here should look
 * clickable when it is not.
 */
export function Features() {
  const { t } = useI18n();

  return (
    <Section
      id="features"
      eyebrow={t.home.features.eyebrow}
      title={t.home.features.title}
      subtitle={t.home.features.subtitle}
    >
      <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {t.home.features.items.map((item) => {
          const Icon = HOME_ICONS[item.icon];

          return (
            <li
              key={item.title}
              className="flex flex-col gap-2.5 rounded-xl border border-border bg-surface p-5"
            >
              <span className="flex size-9 items-center justify-center rounded-lg bg-brand-soft text-brand">
                <Icon size={17} strokeWidth={2.2} aria-hidden="true" />
              </span>
              <h3 className="m-0 text-lg font-bold">{item.title}</h3>
              <p className="m-0 text-sm leading-[1.55] text-ink-2">
                {item.detail}
              </p>
            </li>
          );
        })}
      </ul>
    </Section>
  );
}
