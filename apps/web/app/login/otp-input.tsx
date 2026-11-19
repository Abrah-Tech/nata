"use client";

import { forwardRef, useImperativeHandle, useRef } from "react";
import type { ChangeEvent, ClipboardEvent, KeyboardEvent } from "react";

export const OTP_LENGTH = 6;

/** Lets the form put focus on the first unfinished cell. */
export type OtpInputHandle = { focus: (index: number) => void };

type Props = {
  value: string[];
  onChange: (value: string[]) => void;
  label: string;
  disabled?: boolean;
  /** Id of the inline error, so it is announced together with the group. */
  describedBy?: string;
};

export const OtpInput = forwardRef<OtpInputHandle, Props>(function OtpInput(
  { value, onChange, label, disabled, describedBy },
  ref,
) {
  const refs = useRef<Array<HTMLInputElement | null>>([]);

  function focus(index: number) {
    const input = refs.current[Math.min(Math.max(index, 0), OTP_LENGTH - 1)];
    input?.focus();
    input?.select();
  }

  useImperativeHandle(ref, () => ({ focus }), []);

  function setCell(index: number, digit: string) {
    const next = [...value];
    next[index] = digit;
    onChange(next);
  }

  function handleChange(index: number, event: ChangeEvent<HTMLInputElement>) {
    const digits = event.target.value.replace(/\D/g, "");

    if (digits.length === 0) {
      setCell(index, "");
      return;
    }

    setCell(index, digits[digits.length - 1]);
    focus(index + 1);
  }

  function handleKeyDown(index: number, event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Backspace") {
      if (value[index]) {
        setCell(index, "");
        return;
      }

      event.preventDefault();
      if (index > 0) {
        setCell(index - 1, "");
        focus(index - 1);
      }
      return;
    }

    if (event.key === "ArrowLeft") {
      event.preventDefault();
      focus(index - 1);
    }

    if (event.key === "ArrowRight") {
      event.preventDefault();
      focus(index + 1);
    }
  }

  function handlePaste(event: ClipboardEvent<HTMLInputElement>) {
    const digits = event.clipboardData
      .getData("text")
      .replace(/\D/g, "")
      .slice(0, OTP_LENGTH);

    if (digits.length === 0) {
      return;
    }

    event.preventDefault();
    onChange(
      Array.from({ length: OTP_LENGTH }, (_, index) => digits[index] ?? ""),
    );
    focus(digits.length);
  }

  return (
    <div
      className="grid grid-cols-6 gap-[7px]"
      role="group"
      aria-label={label}
      aria-describedby={describedBy}
    >
      {value.map((digit, index) => (
        <input
          key={index}
          ref={(element) => {
            refs.current[index] = element;
          }}
          className={`h-12 w-full cursor-text rounded-lg border bg-card text-center text-[19px] font-bold text-foreground outline-none transition-[border-color,box-shadow] focus-visible:border-primary focus-visible:shadow-[0_0_0_3px_var(--brand-soft)] disabled:opacity-55 ${digit ? "border-primary" : "border-input hover:border-ink-3"}`}
          value={digit}
          onChange={(event) => handleChange(index, event)}
          onKeyDown={(event) => handleKeyDown(index, event)}
          onPaste={handlePaste}
          onFocus={(event) => event.target.select()}
          inputMode="numeric"
          autoComplete={index === 0 ? "one-time-code" : "off"}
          name={index === 0 ? "otp" : undefined}
          maxLength={1}
          disabled={disabled}
          aria-label={`${label} ${index + 1}`}
        />
      ))}
    </div>
  );
});
