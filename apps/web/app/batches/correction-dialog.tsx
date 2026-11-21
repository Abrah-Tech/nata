"use client";

import {
  ApiError,
  type ProductBatch,
  type ProductCorrection,
  type ProductCorrectionType,
} from "@nata/api-client";
import { useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { formatQty, normalizeSignedDecimal } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { batchError } from "./error-copy";

type Props = {
  batch: ProductBatch;
  onClose: () => void;
  onSaved: (correction: ProductCorrection) => void;
};

/**
 * The qty bounds are decimal strings, so the client pre-check compares them as
 * floats with a small tolerance: 0.1 + 0.2 is 0.30000000000000004, and a
 * correction that exactly refills the batch must not be rejected by a rounding
 * artefact. The server still enforces the real bounds.
 */
const FLOAT_TOLERANCE = 1e-9;

export function CorrectionDialog({ batch, onClose, onSaved }: Props) {
  const { t, locale } = useI18n();

  const [type, setType] = useState<ProductCorrectionType>("waste");
  const [qty, setQty] = useState("");
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const remaining = Number(batch.qty_remaining);
  const received = Number(batch.qty_received);
  const parsed = normalizeSignedDecimal(qty);
  const resulting = parsed === null ? remaining : remaining + Number(parsed);

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    // `aria-disabled` keeps the button focusable, so a repeat submission has to
    // be blocked here as well as in the styling.
    if (saving) return;

    setError(null);

    const quantity = normalizeSignedDecimal(qty);

    if (!quantity || Number(quantity) === 0) {
      setError(t.batches.errors.invalidQuantity);
      return;
    }

    if (type !== "adjustment" && Number(quantity) >= 0) {
      setError(t.batches.errors.invalidSign);
      return;
    }

    if (remaining + Number(quantity) < -FLOAT_TOLERANCE) {
      setError(t.batches.errors.insufficient);
      return;
    }

    if (remaining + Number(quantity) > received + FLOAT_TOLERANCE) {
      setError(t.batches.errors.exceedsReceived);
      return;
    }

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    setSaving(true);

    try {
      const correction = await api.correctProduct(token, {
        product_batch_id: batch.id,
        type,
        qty: quantity,
        note: note.trim() || null,
      });

      onSaved(correction);
    } catch (cause) {
      if (cause instanceof ApiError) {
        setError(batchError(t, cause));

        if (cause.status === 401) {
          onClose();
        }
      } else {
        setError(t.batches.errors.generic);
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      open
      title={t.batches.dialog.title}
      subtitle={fill(t.batches.dialog.subtitle, {
        product: batch.product_name,
        qty: formatQty(batch.qty_remaining, locale),
      })}
      closeLabel={t.batches.dialog.close}
      onClose={onClose}
    >
      <form className="flex flex-col gap-4" onSubmit={submit}>
        <div className="grid grid-cols-2 gap-3 rounded-xl border border-border bg-surface-alt px-3.5 py-3">
          <div className="flex flex-col gap-0.5">
            <span className="text-2xs font-semibold tracking-wide text-ink-3">
              {t.batches.dialog.remaining}
            </span>
            <strong className="text-base font-bold tabular-nums">
              {formatQty(batch.qty_remaining, locale)}{" "}
              {t.batches.meta.unit}
            </strong>
          </div>

          <div className="flex flex-col gap-0.5">
            <span className="text-2xs font-semibold tracking-wide text-ink-3">
              {t.batches.dialog.resulting}
            </span>
            <strong className="text-base font-bold tabular-nums">
              {formatQty(String(resulting), locale)} {t.batches.meta.unit}
            </strong>
          </div>
        </div>

        <Field label={t.batches.dialog.type}>
          <Select
            value={type}
            onChange={(event) =>
              setType(event.target.value as ProductCorrectionType)
            }
            required
          >
            <option value="waste">{t.batches.dialog.types.waste}</option>
            <option value="expired">{t.batches.dialog.types.expired}</option>
            <option value="adjustment">
              {t.batches.dialog.types.adjustment}
            </option>
          </Select>
        </Field>

        <Field
          label={`${t.batches.dialog.qty} (${t.batches.meta.unit})`}
          hint={t.batches.dialog.qtyHint}
          hintId="batch-correction-qty-hint"
        >
          <Input
            type="text"
            inputMode="decimal"
            autoComplete="off"
            placeholder="0"
            aria-describedby="batch-correction-qty-hint"
            value={qty}
            onChange={(event) => setQty(event.target.value)}
            required
          />
        </Field>

        <Field label={t.batches.dialog.note}>
          <Input
            type="text"
            placeholder={t.batches.dialog.notePlaceholder}
            value={note}
            onChange={(event) => setNote(event.target.value)}
          />
        </Field>

        {error && <FormError>{error}</FormError>}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onClose}>
            {t.batches.dialog.cancel}
          </Button>
          <Button
            type="submit"
            aria-disabled={saving ? true : undefined}
            aria-busy={saving ? true : undefined}
          >
            {saving ? t.batches.dialog.saving : t.batches.dialog.submit}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
