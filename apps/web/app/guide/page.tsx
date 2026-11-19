"use client";

import {
  ArrowRight,
  BarChart3,
  BookOpen,
  Boxes,
  ChefHat,
  LayoutDashboard,
  LogIn,
  Package,
  Settings,
  ShoppingCart,
  Sparkles,
  Truck,
} from "lucide-react";
import Link from "next/link";
import type { ComponentType, ReactNode } from "react";

import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { Button } from "@/components/ui/button";
import { useI18n, type Dictionary } from "@/lib/i18n";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

const ICONS: Record<
  string,
  ComponentType<{ size?: number; strokeWidth?: number }>
> = {
  login: LogIn,
  onboarding: Sparkles,
  dashboard: LayoutDashboard,
  products: Boxes,
  stock: Package,
  production: ChefHat,
  sales: ShoppingCart,
  purchasing: Truck,
  reports: BarChart3,
  settings: Settings,
};

type Step = Dictionary["guide"]["steps"][number];

function stepTitle(t: Dictionary, id: string): string {
  return t.guide.steps.find((step) => step.id === id)?.title ?? id;
}

/** One numbered block: the label is fixed, the body is a short list. */
function Block({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5 rounded-xl border border-border bg-surface-alt px-3.5 py-3">
      <h3 className="text-2xs font-bold tracking-wider text-ink-3 uppercase">
        {label}
      </h3>
      {children}
    </div>
  );
}

function Bullets({ items }: { items: string[] }) {
  return (
    <ul className="flex flex-col gap-1 text-sm leading-relaxed text-ink-2">
      {items.map((item) => (
        <li key={item} className="flex gap-2">
          <span aria-hidden="true" className="text-brand">
            •
          </span>
          <span>{item}</span>
        </li>
      ))}
    </ul>
  );
}

function StepBody({ step, t }: { step: Step; t: Dictionary }) {
  const Icon = ICONS[step.icon] ?? BookOpen;

  return (
    <li
      id={step.id}
      className="flex flex-col gap-3 scroll-mt-4"
    >
      <div className="flex items-center gap-2.5">
        <span
          aria-hidden="true"
          className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-brand-soft text-brand"
        >
          <Icon size={15} />
        </span>
        <h2 className="text-xl font-bold tracking-tight">{step.title}</h2>
      </div>

      <p className="max-w-[62ch] text-base leading-relaxed text-ink-2">
        {step.goal}
      </p>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Block label={t.guide.labels.before}>
          <Bullets items={step.before} />
        </Block>
        <Block label={t.guide.labels.inputs}>
          <Bullets items={step.inputs} />
        </Block>
        <Block label={t.guide.labels.outputs}>
          <Bullets items={step.outputs} />
        </Block>
        <Block label={t.guide.labels.next}>
          {step.nextId ? (
            <Link
              href={`#${step.nextId}`}
              className="flex items-center gap-1.5 text-sm font-semibold text-brand no-underline hover:underline"
            >
              {stepTitle(t, step.nextId)}
              <ArrowRight size={14} aria-hidden="true" />
            </Link>
          ) : (
            <span className="text-sm text-ink-3">{t.guide.end}</span>
          )}
        </Block>
      </div>

      {step.tips.length > 0 && (
        <div className="flex flex-col gap-1.5 rounded-xl border border-border bg-card px-3.5 py-3">
          <h3 className="text-2xs font-bold tracking-wider text-ink-3 uppercase">
            {t.guide.labels.tips}
          </h3>
          <ul className="flex flex-col gap-1 text-sm leading-relaxed text-ink-2">
            {step.tips.map((tip) => (
              <li key={tip}>{tip}</li>
            ))}
          </ul>
        </div>
      )}
    </li>
  );
}

export default function GuidePage() {
  const { t } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  if (session.status === "loading") {
    return (
      <AppShell active="guide">
        <div className="flex max-w-[680px] flex-col gap-4">
          <LoadingRegion label={t.dashboard.loading} className="flex flex-col gap-4">
            <SkeletonList rows={4} />
          </LoadingRegion>
        </div>
      </AppShell>
    );
  }

  if (session.status === "error") {
    return (
      <main className="mx-auto max-w-[680px] p-6">
        <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
          <p className="text-sm text-muted-foreground">{t.dashboard.error}</p>
          <Button
            variant="secondary"
            size="sm"
            className="self-start"
            type="button"
            onClick={() => void reloadSession()}
          >
            {t.dashboard.retry}
          </Button>
        </div>
      </main>
    );
  }

  return (
    <AppShell me={session.me} active="guide" onSignOut={signOut}>
      <div className="flex max-w-[680px] flex-col gap-6">
        <header className="flex flex-col gap-1.5">
          <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
            {t.guide.title}
          </h1>
          <p className="max-w-[62ch] text-sm leading-relaxed text-muted-foreground">
            {t.guide.subtitle}
          </p>
        </header>

        <nav
          aria-label={t.guide.toc}
          className="rounded-xl border border-border bg-card px-4 py-3.5"
        >
          <p className="text-2xs font-bold tracking-wider text-ink-3 uppercase">
            {t.guide.toc}
          </p>
          <ol className="mt-2.5 grid grid-cols-1 gap-x-6 gap-y-1.5 sm:grid-cols-2">
            {t.guide.steps.map((step, index) => (
              <li key={step.id}>
                <Link
                  href={`#${step.id}`}
                  className="flex items-baseline gap-1.5 text-sm font-semibold text-ink-2 no-underline hover:text-brand hover:underline"
                >
                  <span className="text-2xs font-bold text-brand">
                    {index + 1}.
                  </span>
                  {step.title}
                </Link>
              </li>
            ))}
          </ol>
        </nav>

        <ol className="flex flex-col gap-8">
          {t.guide.steps.map((step) => (
            <StepBody key={step.id} step={step} t={t} />
          ))}
        </ol>

        <section className="flex flex-col gap-2 rounded-xl border border-border bg-surface-alt px-4 py-3.5">
          <h2 className="text-base font-bold tracking-tight">
            {t.guide.soonTitle}
          </h2>
          <p className="text-sm leading-relaxed text-ink-2">
            {t.guide.soonBody}
          </p>
        </section>
      </div>
    </AppShell>
  );
}
