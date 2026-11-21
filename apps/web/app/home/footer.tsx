"use client";

import Link from "next/link";

import { useI18n } from "@/lib/i18n";

import { Wheat } from "./icons";

export function HomeFooter() {
  const { t } = useI18n();
  const { footer, nav } = t.home;

  const links = [
    { href: "#features", label: nav.features },
    { href: "#how", label: nav.how },
    { href: "#pricing", label: nav.pricing },
    { href: "/guide", label: nav.guide },
    { href: "/login", label: nav.signIn },
  ];

  const linkClass =
    "rounded-md text-sm text-ink-2 no-underline transition-colors outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand";

  return (
    <footer className="border-t border-border">
      <div className="mx-auto flex w-full max-w-[1080px] flex-col gap-8 px-4 py-10 sm:px-8">
        <div className="flex flex-col gap-6 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex flex-col gap-2">
            <Link
              href="/"
              className="flex w-fit items-center gap-2 rounded-md text-ink no-underline outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
            >
              <span className="flex size-8 items-center justify-center rounded-lg bg-brand text-white">
                <Wheat size={16} strokeWidth={2.2} aria-hidden="true" />
              </span>
              <span className="text-lg font-extrabold tracking-[-0.01em]">
                {t.common.appName}
              </span>
            </Link>
            <p className="m-0 max-w-[38ch] text-sm leading-[1.55] text-ink-3">
              {footer.tagline}
            </p>
          </div>

          <nav aria-label={footer.product}>
            <ul className="grid grid-cols-2 gap-x-10 gap-y-2">
              {links.map((link) => (
                <li key={link.href}>
                  {link.href.startsWith("#") ? (
                    <a href={link.href} className={linkClass}>
                      {link.label}
                    </a>
                  ) : (
                    <Link href={link.href} className={linkClass}>
                      {link.label}
                    </Link>
                  )}
                </li>
              ))}
            </ul>
          </nav>
        </div>

        <div className="flex flex-col gap-2 border-t border-border pt-4 text-2xs text-ink-3 sm:flex-row sm:items-center sm:justify-between">
          <span>{footer.rights}</span>
          <a
            href={`mailto:${footer.contact}`}
            className="rounded-md font-semibold text-ink-2 no-underline transition-colors outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
          >
            {footer.contact}
          </a>
        </div>
      </div>
    </footer>
  );
}
