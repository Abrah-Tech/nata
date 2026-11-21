"use client";

import Link from "next/link";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";

/** The closing argument, on the dark brand surface the sign-in panel uses. */
export function Cta() {
  const { t } = useI18n();
  const cta = t.home.cta;

  return (
    <section className="rounded-2xl bg-brand-dark px-5 py-12 sm:px-12 sm:py-16">
      <div className="mx-auto flex max-w-[560px] flex-col gap-4 text-center">
        <h2 className="m-0 text-3xl font-bold tracking-[-0.02em] text-white">
          {cta.title}
        </h2>
        <p className="m-0 text-base leading-[1.6] text-on-brand">
          {cta.subtitle}
        </p>

        <div className="mt-2 flex flex-col justify-center gap-2.5 sm:flex-row">
          <Button
            size="lg"
            asChild
            className="w-full bg-white text-brand-dark hover:bg-brand-soft sm:w-auto"
          >
            <Link href="/login">{cta.primary}</Link>
          </Button>
          <Button
            size="lg"
            variant="ghost"
            asChild
            className="w-full border-transparent text-white hover:bg-brand-raised sm:w-auto"
          >
            <Link href="#pricing">{cta.secondary}</Link>
          </Button>
        </div>
      </div>
    </section>
  );
}
