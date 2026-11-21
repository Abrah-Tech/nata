"use client";

import Link from "next/link";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";

import { LocaleSwitch } from "../locale-switch";
import { Wheat } from "./icons";

const ANCHORS = [
  { id: "features", key: "features" },
  { id: "how", key: "how" },
  { id: "pricing", key: "pricing"},
] as const;

/**
 * The one piece of chrome on the landing page. It stays in view because the
 * page is long and the sign-in action is the point of every section, so a
 * visitor should never have to scroll back to find it.
 */
export function HomeNav() {
  const { t } = useI18n();
  const labels = t.home.nav;

  const linkClass =
    "rounded-md px-2.5 py-1.5 text-sm font-semibold text-ink-2 no-underline transition-colors outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand";

  return (
    <header className="sticky top-0 z-30 border-b border-border bg-bg/85 backdrop-blur">
      <div className="mx-auto flex h-14 w-full max-w-[1080px] items-center gap-1.5 px-4 sm:gap-2 sm:px-8">
        <Link
          href="/"
          className="flex items-center gap-2 rounded-md text-ink no-underline outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
        >
          <span className="flex size-8 items-center justify-center rounded-lg bg-brand text-white">
            <Wheat size={16} strokeWidth={2.2} aria-hidden="true" />
          </span>
          <span className="text-lg font-extrabold tracking-[-0.01em]">
            {t.common.appName}
          </span>
        </Link>

        <nav className="ml-4 hidden items-center gap-0.5 md:flex">
          {ANCHORS.map((anchor) => (
            <a key={anchor.id} href={`#${anchor.id}`} className={linkClass}>
              {labels[anchor.key]}
            </a>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-1.5">
          <LocaleSwitch />
          <Link
            href="/login"
            className={`${linkClass} hidden sm:block`}
          >
            {labels.signIn}
          </Link>
          <Button
            size="sm"
            asChild
            className="h-8 px-3 text-xs sm:h-11 sm:px-4 sm:text-sm"
          >
            <Link href="/login">{labels.cta}</Link>
          </Button>
        </div>
      </div>
    </header>
  );
}
