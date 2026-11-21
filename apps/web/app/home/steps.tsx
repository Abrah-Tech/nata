"use client";

import { useI18n } from "@/lib/i18n";

import { Reveal } from "./reveal";
import { Section } from "./section";

/** The three habits the product asks for, in the order they happen. */
export function Steps() {
  const { t } = useI18n();

  return (
    <Reveal>
      <Section
        id="how"
        eyebrow={t.home.steps.eyebrow}
        title={t.home.steps.title}
        subtitle={t.home.steps.subtitle}
      >
        <ol className="grid gap-3 sm:grid-cols-3">
          {t.home.steps.items.map((item, index) => (
            <li
              key={item.title}
              className="flex flex-col gap-2.5 rounded-xl border border-border bg-surface p-5"
            >
              <span className="flex size-8 items-center justify-center rounded-full bg-brand text-sm font-bold text-white">
                {index + 1}
              </span>
              <h3 className="m-0 text-lg font-bold">{item.title}</h3>
              <p className="m-0 text-sm leading-[1.55] text-ink-2">
                {item.detail}
              </p>
            </li>
          ))}
        </ol>
      </Section>
    </Reveal>
  );
}
