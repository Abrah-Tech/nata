"use client";

import { ArrowRight } from "lucide-react";
import Link from "next/link";

import { useFirstSteps, type FirstStepId } from "@/lib/first-steps";
import { useI18n } from "@/lib/i18n";

/**
 * One line under a screen header, pointing at the first step the business has
 * not finished yet.
 *
 * `at` is the step this screen owns. While the user is standing on that step
 * there is nowhere to send them, so the line stays silent and the overview
 * card carries the list instead. It disappears with the card once the
 * checklist is complete.
 */
function FirstStepsHint({ at }: { at: FirstStepId }) {
  const { t } = useI18n();
  const load = useFirstSteps();

  if (load.status !== "ready") return null;

  const next = load.steps.find((step) => !step.done);

  if (!next || next.id === at) return null;

  return (
    <Link
      href={next.href}
      className="group flex items-center gap-1.5 text-xs font-semibold text-ink-3 transition-colors duration-150 hover:text-brand focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-brand"
    >
      <span>{t.firstSteps.next}</span>
      <span className="text-ink">{t.firstSteps.steps[next.id]}</span>
      <ArrowRight
        size={13}
        aria-hidden="true"
        className="transition-transform duration-150 group-hover:translate-x-0.5"
      />
    </Link>
  );
}

export { FirstStepsHint };
