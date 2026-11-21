"use client";

import { usePathname } from "next/navigation";
import { useEffect, useRef } from "react";

import { useI18n, type Dictionary } from "@/lib/i18n";

/** The view title for a path, taken from the first segment. */
function titleFor(pathname: string, t: Dictionary): string {
  const segment = pathname.split("/")[1] ?? "";

  switch (segment) {
    case "dashboard":
      return t.dashboard.title;
    case "onboarding":
      return t.onboarding.title;
    case "products":
      return t.products.title;
    case "sales":
      return t.sales.title;
    case "reports":
      return t.reports.title;
    case "team":
      return t.team.title;
    case "settings":
      return t.settings.title;
    case "join":
      return t.join.title;
    case "purchasing":
      return t.purchasing.title;
    case "production":
      return t.production.title;
    case "stock":
      return t.stock.title;
    case "batches":
      return t.batches.title;
    case "login":
      return t.login.title;
    case "guide":
      return t.guide.title;
    default:
      return t.common.appName;
  }
}

/**
 * A client-side navigation resets nothing on its own: the document title stays
 * on the old view and focus stays on the control that was clicked. This moves
 * both to the new view. Screens without a `<main>` (the login page) only get the
 * title.
 */
export function RouteAnnouncer() {
  const pathname = usePathname();
  const { t } = useI18n();
  const previous = useRef<string | null>(null);

  useEffect(() => {
    document.title = `${titleFor(pathname, t)} · ${t.common.appName}`;

    const firstRender = previous.current === null;
    const sameView = previous.current === pathname;

    previous.current = pathname;

    // The first paint already sits in the right place, and a locale change only
    // swaps the copy, so neither one moves focus.
    if (firstRender || sameView) return;

    document.getElementById("main")?.focus();
  }, [pathname, t]);

  return null;
}
