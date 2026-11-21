import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * One band of the landing page: eyebrow, title, optional subtitle, then the
 * band's content. The gap inside a band is 24px and the gap between bands is
 * set by the page, so a reader can see where one group ends and the next
 * begins.
 *
 * `id` is the anchor the top navigation points at. `scroll-mt-20` keeps the
 * sticky bar from covering the heading the link lands on.
 */
export function Section({
  id,
  eyebrow,
  title,
  subtitle,
  children,
  className,
}: {
  id?: string;
  eyebrow: string;
  title: string;
  subtitle?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      id={id}
      className={cn("flex scroll-mt-20 flex-col gap-6", className)}
    >
      <header className="flex max-w-[620px] flex-col gap-2">
        <p className="m-0 text-2xs font-bold tracking-[0.12em] text-brand uppercase">
          {eyebrow}
        </p>
        <h2 className="m-0 text-3xl font-bold tracking-[-0.02em]">{title}</h2>
        {subtitle && (
          <p className="m-0 text-base leading-[1.6] text-ink-2">{subtitle}</p>
        )}
      </header>
      {children}
    </section>
  );
}
