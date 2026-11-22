"use client";

import type { MeResponse } from "@nata/api-client";
import {
  BarChart3,
  BookOpen,
  Boxes,
  CalendarClock,
  ChefHat,
  LayoutDashboard,
  Menu,
  Package,
  Settings,
  ShoppingCart,
  Truck,
  Users,
} from "lucide-react";
import Link from "next/link";
import { useState, type ComponentType } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";

import { LocaleSwitch } from "./locale-switch";

type Active =
  | "overview"
  | "stock"
  | "batches"
  | "production"
  | "products"
  | "pos"
  | "purchasing"
  | "reports"
  | "team"
  | "settings"
  | "guide";

type Item = {
  key: string;
  label: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  href: string;
};

type Props = {
  /**
   * Null while /me is still in flight: the chrome renders immediately with
   * placeholders for the tenant and the user, so a reload never shows the page
   * without its sidebar.
   */
  me?: MeResponse | null;
  active: Active;
  onSignOut?: () => void;
  children: React.ReactNode;
};

function initials(name: string): string {
  return name
    .split(" ")
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

/**
 * One destination in the phone's bottom bar. A five-slot bar is still one tap
 * each, and the active pill keeps the same treatment as the desktop nav.
 */
function MobileTab({
  href,
  icon: Icon,
  label,
  active,
}: {
  href: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  label: string;
  active?: boolean;
}) {
  return (
    <Link
      href={href}
      className={cn(
        "flex flex-col items-center justify-center gap-0.5 rounded-[21px] text-2xs font-semibold text-ink-3 no-underline transition-colors hover:bg-surface-alt",
        active && "bg-brand-soft font-bold text-brand hover:bg-brand-soft",
      )}
      aria-current={active ? "page" : undefined}
    >
      <Icon size={18} />
      <span>{label}</span>
    </Link>
  );
}

export function AppShell({ me, active, onSignOut, children }: Props) {
  const { t } = useI18n();
  const [moreOpen, setMoreOpen] = useState(false);

  // Buying happens between counting stock and running production, so it sits
  // between the two on the way down the sidebar.
  const groups: { label: string; items: Item[] }[] = [
    {
      label: t.nav.operations,
      items: [
        {
          key: "overview",
          label: t.nav.overview,
          icon: LayoutDashboard,
          href: "/dashboard",
        },
        { key: "stock", label: t.nav.stock, icon: Package, href: "/stock" },
        {
          key: "batches",
          label: t.nav.batches,
          icon: CalendarClock,
          href: "/batches",
        },
        {
          key: "purchasing",
          label: t.nav.purchasing,
          icon: Truck,
          href: "/purchasing",
        },
        {
          key: "production",
          label: t.nav.production,
          icon: ChefHat,
          href: "/production",
        },
        {
          key: "products",
          label: t.nav.products,
          icon: Boxes,
          href: "/products",
        },
      ],
    },
    {
      label: t.nav.sales,
      items: [
        {
          key: "pos",
          label: t.nav.pos,
          icon: ShoppingCart,
          href: "/sales",
        },
        {
          key: "reports",
          label: t.nav.reports,
          icon: BarChart3,
          href: "/reports",
        },
      ],
    },
    {
      label: t.nav.other,
      items: [
        {
          key: "team",
          label: t.nav.team,
          icon: Users,
          href: "/team",
        },
        {
          key: "settings",
          label: t.nav.settings,
          icon: Settings,
          href: "/settings",
        },
        {
          key: "guide",
          label: t.nav.guide,
          icon: BookOpen,
          href: "/guide",
        },
      ],
    },
  ];

  // Everything a phone cannot fit in the bar. Without this list those screens
  // had no link from anywhere on a phone, because the desktop sidebar is the
  // only other place they were listed.
  const moreItems: Item[] = [
    {
      key: "reports",
      label: t.nav.reports,
      icon: BarChart3,
      href: "/reports",
    },
    {
      key: "purchasing",
      label: t.nav.purchasing,
      icon: Truck,
      href: "/purchasing",
    },
    {
      key: "batches",
      label: t.nav.batches,
      icon: CalendarClock,
      href: "/batches",
    },
    { key: "products", label: t.nav.products, icon: Boxes, href: "/products" },
    { key: "team", label: t.nav.team, icon: Users, href: "/team" },
    {
      key: "settings",
      label: t.nav.settings,
      icon: Settings,
      href: "/settings",
    },
    { key: "guide", label: t.nav.guide, icon: BookOpen, href: "/guide" },
  ];

  return (
    <div className="min-h-dvh min-[961px]:grid min-[961px]:grid-cols-[246px_minmax(0,1fr)]">
      <a href="#main" className="skip-link">
        {t.common.skipToContent}
      </a>

      <aside className="hidden min-[961px]:sticky min-[961px]:top-0 min-[961px]:flex min-[961px]:h-dvh min-[961px]:flex-col min-[961px]:gap-4 min-[961px]:overflow-y-auto min-[961px]:overscroll-contain min-[961px]:border-r min-[961px]:border-border min-[961px]:bg-surface min-[961px]:p-[18px]">
        <div className="flex items-center gap-2.5">
          <span className="flex size-8 items-center justify-center rounded-[9px] bg-brand text-white">
            <ChefHat size={17} />
          </span>
          <span className="flex flex-col leading-[1.2]">
            <strong className="text-base font-bold">Nata</strong>
            <span className="text-2xs font-semibold tracking-[0.08em] text-ink-3">
              PPIC + POS
            </span>
          </span>
        </div>

        <div className="flex justify-between rounded-lg border border-border bg-surface-alt px-3 py-2.5">
          {me ? (
            <div>
              <strong className="block text-xs font-bold">{me.tenant.name}</strong>
              <span className="text-2xs text-ink-3">
                {t.dashboard.roles[me.role]}
              </span>
            </div>
          ) : (
            <div className="flex flex-col gap-1.5" aria-hidden="true">
              <Skeleton className="h-3 w-28" />
              <Skeleton className="h-2.5 w-16" />
            </div>
          )}
        </div>

        <nav className="flex flex-col gap-3" aria-label={t.nav.main}>
          {groups.map((group) => (
            <div className="flex flex-col gap-0.5" key={group.label}>
              <p className="px-2.5 py-1 text-2xs font-bold tracking-[0.09em] text-ink-3">
                {group.label}
              </p>
              {group.items.map((item) => (
                <Link
                  key={item.key}
                  href={item.href}
                  className={cn(
                    "flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-xs font-semibold text-ink-2 no-underline transition-colors hover:bg-surface-alt",
                    item.key === active &&
                      "bg-brand-soft font-bold text-brand hover:bg-brand-soft",
                  )}
                  aria-current={item.key === active ? "page" : undefined}
                >
                  <item.icon size={16} />
                  <span>{item.label}</span>
                </Link>
              ))}
            </div>
          ))}
        </nav>

        <div className="mt-auto flex flex-col gap-2.5 border-t border-border pt-3.5">
          <div className="flex items-center gap-2.5">
            {me ? (
              <>
                <span className="flex size-7 items-center justify-center rounded-full bg-ink text-2xs font-bold text-white">
                  {initials(me.user.name)}
                </span>
                <div>
                  <strong className="block text-xs font-bold">
                    {me.user.name}
                  </strong>
                  <span className="text-2xs text-ink-3">
                    {t.dashboard.roles[me.role]}
                  </span>
                </div>
              </>
            ) : (
              <div className="flex items-center gap-2.5" aria-hidden="true">
                <Skeleton className="size-7 rounded-full" />
                <div className="flex flex-col gap-1.5">
                  <Skeleton className="h-2.5 w-24" />
                  <Skeleton className="h-2 w-14" />
                </div>
              </div>
            )}
          </div>
          <div className="flex items-center justify-between gap-2">
            <LocaleSwitch />
            <Button
              variant="ghost"
              size="sm"
              type="button"
              onClick={onSignOut}
              disabled={!onSignOut}
            >
              {t.nav.signOut}
            </Button>
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-col">
        <header className="hidden h-[60px] items-center justify-between border-b border-border bg-surface px-6 min-[961px]:flex">
          {me ? (
            <div className="flex flex-col leading-[1.3]">
              <span className="text-2xs font-bold tracking-[0.08em] text-ink-3 uppercase">
                {t.dashboard.tenant}
              </span>
              <strong className="text-sm font-bold">{me.tenant.name}</strong>
            </div>
          ) : (
            <div className="flex flex-col gap-1.5" aria-hidden="true">
              <Skeleton className="h-2 w-12" />
              <Skeleton className="h-3 w-32" />
            </div>
          )}
          <LocaleSwitch />
        </header>

        <main
          id="main"
          tabIndex={-1}
          className="px-4 pt-4 pb-24 min-[961px]:px-7 min-[961px]:pt-6 min-[961px]:pb-10"
        >
          {children}
        </main>
      </div>

      <nav
        className="fixed inset-x-4 bottom-3 z-20 grid h-14 grid-cols-5 gap-1 rounded-[28px] border border-border bg-white/95 p-1.5 backdrop-blur min-[961px]:hidden"
        aria-label={t.nav.main}
      >
        <MobileTab
          href="/dashboard"
          icon={LayoutDashboard}
          label={t.nav.mobileHome}
          active={active === "overview"}
        />
        <MobileTab
          href="/stock"
          icon={Package}
          label={t.nav.mobileStock}
          active={active === "stock"}
        />
        {/* The cashier's screen earns a slot of its own. It used to have no link
            anywhere on a phone, so a shop that only works from one could not
            take a sale at all. */}
        <MobileTab
          href="/sales"
          icon={ShoppingCart}
          label={t.nav.mobileCashier}
          active={active === "pos"}
        />
        <MobileTab
          href="/production"
          icon={ChefHat}
          label={t.nav.mobileProduction}
          active={active === "production"}
        />
        <button
          type="button"
          onClick={() => setMoreOpen(true)}
          className="flex flex-col items-center justify-center gap-0.5 rounded-[21px] text-2xs font-semibold text-ink-3 no-underline transition-colors hover:bg-surface-alt"
        >
          <Menu size={18} />
          <span>{t.nav.mobileMore}</span>
        </button>
      </nav>

      <Modal
        open={moreOpen}
        title={t.nav.more.title}
        closeLabel={t.nav.more.close}
        onClose={() => setMoreOpen(false)}
      >
        <div className="flex flex-col gap-0.5">
          {moreItems.map((item) => (
            <Link
              key={item.key}
              href={item.href}
              onClick={() => setMoreOpen(false)}
              className="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-xs font-semibold text-ink-2 no-underline transition-colors hover:bg-surface-alt"
            >
              <item.icon size={16} />
              <span>{item.label}</span>
            </Link>
          ))}
          {onSignOut && (
            <Button
              variant="ghost"
              size="sm"
              type="button"
              className="justify-start"
              onClick={onSignOut}
            >
              {t.nav.signOut}
            </Button>
          )}
        </div>
      </Modal>
    </div>
  );
}
