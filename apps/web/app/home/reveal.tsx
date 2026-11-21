"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * Fades a band of the landing page in the first time it scrolls into view.
 *
 * The hidden state lives in CSS under `html.js`, which the root layout sets
 * before the body renders, so nothing flashes. Without JavaScript the content
 * is simply visible, and under `prefers-reduced-motion` the transition is not
 * applied at all — a reader is never left looking at an empty band.
 */
export function Reveal({
  children,
  className,
  delay = 0,
}: {
  children: ReactNode;
  className?: string;
  /** Milliseconds after the band enters view, used to stagger a group. */
  delay?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(false);

  useEffect(() => {
    const node = ref.current;

    if (!node || !("IntersectionObserver" in window)) {
      setShown(true);
      return;
    }

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setShown(true);
          observer.disconnect();
        }
      },
      // A sliver of the band is enough, and the bottom margin keeps a band
      // that is still mostly below the fold from firing early.
      { threshold: 0.12, rootMargin: "0px 0px -8% 0px" },
    );

    observer.observe(node);

    return () => observer.disconnect();
  }, []);

  return (
    <div
      ref={ref}
      className={cn("home-reveal", shown && "is-shown", className)}
      style={delay ? { transitionDelay: `${delay}ms` } : undefined}
    >
      {children}
    </div>
  );
}
