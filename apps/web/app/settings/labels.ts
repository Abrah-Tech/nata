import type { Dictionary } from "@/lib/i18n";

/** The plan names the billing card and the audit log both need. */
export function planName(t: Dictionary, plan: string): string {
  switch (plan) {
    case "trial":
      return t.settings.plan.trial;
    case "starter":
      return t.settings.plan.starter;
    case "pro":
      return t.settings.plan.pro;
    case "business":
      return t.settings.plan.business;
    default:
      return plan;
  }
}

/** Falls back to the raw value for a method added after the dictionary. */
export function methodName(t: Dictionary, method: string): string {
  const methods = t.settings.billing.methods as Record<string, string | undefined>;

  return methods[method] ?? method;
}
