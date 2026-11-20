"use client";

import { useEffect, useState } from "react";

import { api } from "@/lib/api";
import type { Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";

export type FirstStepId = keyof Dictionary["firstSteps"]["steps"];

export type FirstStep = {
  id: FirstStepId;
  done: boolean;
  href: string;
};

export type FirstStepLoad =
  | { status: "loading" }
  | { status: "ready"; steps: FirstStep[] }
  | { status: "error" };

/**
 * Reads the five answers the checklist is made of, each from the screen that
 * owns it. The array order is the order the product needs them in: a sale
 * needs stock, stock needs a batch, a batch needs a recipe, a recipe needs
 * materials.
 *
 * `materialsCount` lets a screen that already holds the number skip the
 * materials read. Everything else always comes from the API, so a step the
 * user just finished is never still shown as outstanding.
 */
export function useFirstSteps(materialsCount?: number | null): FirstStepLoad {
  const [load, setLoad] = useState<FirstStepLoad>({ status: "loading" });

  useEffect(() => {
    const token = readToken();

    if (!token) return;

    let cancelled = false;

    void (async () => {
      try {
        // Five reads, because no single screen owns all five answers. They
        // travel in parallel with the screen rather than after it.
        const [materials, products, movements, productBatches, sales] =
          await Promise.all([
            materialsCount == null
              ? api.listMaterials(token)
              : Promise.resolve(null),
            api.listProducts(token),
            api.listMovements(token, { limit: 1 }),
            api.listProductBatches(token),
            api.listSales(token, 1),
          ]);

        if (cancelled) return;

        const materialTotal = materialsCount ?? materials?.length ?? 0;

        setLoad({
          status: "ready",
          steps: [
            {
              id: "materials",
              done: materialTotal > 0,
              href: materialTotal > 0 ? "/stock" : "/onboarding",
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
        // The checklist is auxiliary: a failed read leaves it hidden instead
        // of showing a list that might be wrong.
        if (!cancelled) setLoad({ status: "error" });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [materialsCount]);

  return load;
}
