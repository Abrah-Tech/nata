import { cn } from "@/lib/utils";

/**
 * A native checkbox drawn as a switch.
 *
 * The input stays in the accessibility tree and in the tab order, so keyboard
 * and screen-reader support come from the platform rather than from a
 * JavaScript primitive the cheap Android WebView the shell runs on would have
 * to drive. The track and the knob are siblings of the input, because a
 * `peer-checked` style cannot reach into a child.
 *
 * Wrap it in a `<label>` with visible text to give it an accessible name.
 */
function Switch({ className, ...props }: React.ComponentProps<"input">) {
  return (
    <span className={cn("relative inline-flex h-6 w-11 shrink-0", className)}>
      <input
        type="checkbox"
        data-slot="switch"
        className="peer sr-only"
        {...props}
      />
      <span
        aria-hidden="true"
        className="absolute inset-0 rounded-full border border-border bg-surface-alt transition-colors duration-150 peer-checked:border-brand peer-checked:bg-brand peer-focus-visible:shadow-[0_0_0_3px_var(--brand-soft)] peer-disabled:opacity-55 motion-reduce:transition-none"
      />
      <span
        aria-hidden="true"
        className="relative left-0.5 h-5 w-5 rounded-full bg-card shadow-sm transition-transform duration-150 peer-checked:translate-x-5 peer-disabled:opacity-55 motion-reduce:transition-none"
      />
    </span>
  );
}

export { Switch };
