import { cn } from "@/lib/utils";

/**
 * Label, control and hint as one unit. The label wraps the control, so clicking
 * the text focuses the control without threading ids through every form. The
 * hint sits outside the label on purpose: inside it, the hint text becomes part
 * of the control's accessible name instead of being available as its description.
 */
function Field({
  label,
  hint,
  hintId,
  className,
  children,
}: {
  label: string;
  hint?: string;
  /** Id of the hint node, referenced by the control's `aria-describedby`. */
  hintId?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label className="flex flex-col gap-1.5">
        <span className="text-xs font-semibold text-muted-foreground select-none">
          {label}
        </span>
        {children}
      </label>
      {hint && (
        <span id={hintId} className="text-2xs leading-snug text-ink-3">
          {hint}
        </span>
      )}
    </div>
  );
}

function FormError({ children }: { children: React.ReactNode }) {
  return (
    <p
      role="alert"
      className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
    >
      {children}
    </p>
  );
}

export { Field, FormError };
