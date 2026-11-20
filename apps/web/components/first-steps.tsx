"use client";

import { Check, ChevronRight } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";

import { LoadingRegion } from "@/components/skeletons";
import { Skeleton } from "@/components/ui/skeleton";
import { api } from "@/lib/api";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";

type TaskId = keyof Dictionary["firstSteps"]["steps"];

type Task = {
  id: TaskId;
  done: boolean;
  href: string;
};

type Load =
  | { status: "loading" }
  | { status: "ready"; tasks: Task[] }
  | { status: "error" };

/**
 * The five things a business needs before Nata can plan anything. Each one is
 * read from the screens that own it, so the list ticks itself off and the card
 * disappears once there is nothing left to do.
 */
function FirstSteps({ materials }: { materials: number }) {
  const { t } = useI18n();
  const [load, setLoad] = useState<Load>({ status: "loading" });

  useEffect(() => {
    const token = readToken();

    if (!token) return;

    let cancelled = false;

    void (async () => {
      try {
        // Four reads, because no single screen owns all five answers. They
        // travel in parallel with the overview rather than after it.
        const [products, movements, productBatches, sales] = await Promise.all([
          api.listProducts(token),
          api.listMovements(token, { limit: 1 }),
          api.listProductBatches(token),
          api.listSales(token, 1),
        ]);

        if (cancelled) return;

        setLoad({
          status: "ready",
          tasks: [
            {
              id: "materials",
              done: materials > 0,
              href: materials > 0 ? "/stock" : "/onboarding",
            },
            {
              id: "recipe",
              done: products.some(
                (product) => product.current_recipe_id !== null,
              ),
              href: "/products",
            },
            {
              // Every batch opens with its purchase movement, so one movement
              // means one stock-in. Counting batches instead would read as
              // "nothing left" once a busy day consumes the last of them.
              id: "stockIn",
              done: movements.length > 0,
              href: "/stock",
            },
            {
              id: "production",
              done: productBatches.length > 0,
              href: "/production",
            },
            { id: "sales", done: sales.items.length > 0, href: "/sales" },
          ],
        });
      } catch {
        // The card is auxiliary. A failed read leaves it hidden instead of
        // showing a list that might be wrong.
        if (!cancelled) setLoad({ status: "error" });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [materials]);

  if (load.status === "error") return null;

  const done = load.status === "ready"
    ? load.tasks.filter((task) => task.done).length
    : 0;

  if (load.status === "ready" && done === load.tasks.length) return null;

  return (
    <section className="flex flex-col gap-2.5 rounded-xl border border-border bg-card px-4 py-3.5">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="text-base font-bold tracking-tight">
          {t.firstSteps.title}
        </h2>
        {load.status === "ready" && (
          <span className="text-2xs font-semibold text-ink-3 tabular-nums">
            {fill(t.firstSteps.counter, {
              done,
              total: load.tasks.length,
            })}
          </span>
        )}
      </div>

      {load.status === "loading" ? (
        <LoadingRegion
          label={t.firstSteps.title}
          className="flex flex-col"
        >
          <FirstStepsRows />
        </LoadingRegion>
      ) : (
        <ol className="flex list-none flex-col p-0">
          {load.tasks.map((task, index) => {
            const label = t.firstSteps.steps[task.id];

            return (
              <li
                key={task.id}
                className="border-t border-border first:border-t-0"
              >
                {task.done ? (
                  <div className="flex items-center gap-2.5 py-2">
                    <span className="flex size-5 shrink-0 items-center justify-center rounded-full bg-brand-soft text-brand">
                      <Check size={12} strokeWidth={3} aria-hidden="true" />
                    </span>
                    <span className="text-sm text-ink-3">{label}</span>
                  </div>
                ) : (
                  <Link
                    href={task.href}
                    className="group flex items-center gap-2.5 py-2 text-sm font-medium text-ink no-underline transition-colors duration-150 hover:text-brand focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-brand"
                  >
                    <span
                      aria-hidden="true"
                      className="flex size-5 shrink-0 items-center justify-center rounded-full border border-border text-xs font-bold text-ink-3"
                    >
                      {index + 1}
                    </span>
                    <span className="flex-1">{label}</span>
                    <ChevronRight
                      size={14}
                      aria-hidden="true"
                      className="text-ink-3 transition-transform duration-150 group-hover:translate-x-0.5"
                    />
                  </Link>
                )}
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}

/** Five rows of the right height, so the card does not jump when the list lands. */
function FirstStepsRows() {
  return (
    <div aria-hidden="true" className="flex flex-col">
      {[0, 1, 2, 3, 4].map((row) => (
        <div
          key={row}
          className="flex items-center gap-2.5 border-t border-border py-2 first:border-t-0"
        >
          <Skeleton className="size-5 rounded-full" />
          <Skeleton className="h-3 flex-1" />
        </div>
      ))}
    </div>
  );
}

export { FirstSteps };
