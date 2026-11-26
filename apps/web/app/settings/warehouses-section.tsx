"use client";

import { ApiError, type Warehouse } from "@nata/api-client";
import { useEffect, useState } from "react";

import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { fill, type Dictionary, type Locale } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { Intro } from "./intro";

type Props = {
  locale: Locale;
  /** A non-manager may see the locations, but not change them. */
  readOnly: boolean;
  t: Dictionary;
};

function errorCopy(t: Dictionary, cause: unknown): string {
  const copy = t.settings.warehouses;

  if (cause instanceof ApiError) {
    switch (cause.code) {
      case "name_required":
        return copy.nameRequired;
      case "warehouse_exists":
        return copy.nameTaken;
      case "plan_limit_reached":
        return fill(copy.limitReached, {
          limit: (cause.details as { limit?: number })?.limit ?? "",
        });
      case "warehouse_in_use":
        return copy.inUse;
    }
  }

  return copy.failed;
}

export function WarehousesSection({ locale, readOnly, t }: Props) {
  const copy = t.settings.warehouses;

  const [rows, setRows] = useState<Warehouse[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");

  useEffect(() => {
    const token = readToken();

    if (!token) return;

    let active = true;

    api
      .listWarehouses(token)
      .then((list) => {
        if (!active) return;
        setRows(list);
        setFailed(false);
      })
      .catch(() => {
        if (active) setFailed(true);
      });

    return () => {
      active = false;
    };
  }, [attempt]);

  useEffect(() => {
    if (!notice) return;

    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  function retry() {
    setRows(null);
    setFailed(false);
    setAttempt((value) => value + 1);
  }

  async function add(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (busy || readOnly) return;

    const token = readToken();

    if (!token) return;

    setBusy(true);
    setError(null);

    try {
      await api.createWarehouse(token, { name: name.trim() });
      setName("");
      setAttempt((value) => value + 1);
    } catch (cause) {
      setError(errorCopy(t, cause));
    } finally {
      setBusy(false);
    }
  }

  function startRename(row: Warehouse) {
    setEditingId(row.id);
    setEditingName(row.name);
    setError(null);
  }

  async function saveRename(id: string) {
    if (busy) return;

    const token = readToken();

    if (!token) return;

    setBusy(true);
    setError(null);

    try {
      await api.updateWarehouse(token, id, { name: editingName.trim() });
      setEditingId(null);
      setAttempt((value) => value + 1);
    } catch (cause) {
      setError(errorCopy(t, cause));
    } finally {
      setBusy(false);
    }
  }

  async function remove(row: Warehouse) {
    if (busy || readOnly) return;

    const token = readToken();

    if (!token) return;

    setBusy(true);
    setError(null);

    try {
      await api.removeWarehouse(token, row.id);
      setNotice(copy.deleted);
      setAttempt((value) => value + 1);
    } catch (cause) {
      setError(errorCopy(t, cause));
    } finally {
      setBusy(false);
    }
  }

  const loading = rows === null && !failed;

  return (
    <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
      <Intro title={copy.title} body={copy.body} />

      {loading && (
        <LoadingRegion label={t.settings.loading} className="flex flex-col gap-2">
          <SkeletonList rows={2} />
        </LoadingRegion>
      )}

      {failed && !loading && (
        <div className="flex flex-col gap-2">
          <p role="alert" className="text-sm text-destructive">
            {copy.failed}
          </p>
          <Button
            variant="secondary"
            size="sm"
            className="self-start"
            type="button"
            onClick={retry}
          >
            {t.settings.retry}
          </Button>
        </div>
      )}

      {!loading && !failed && rows && (
        <>
          {rows.length === 0 ? (
            <p className="text-sm text-muted-foreground">{copy.empty}</p>
          ) : (
            <ul className="flex list-none flex-col gap-2 p-0">
              {rows.map((row) =>
                editingId === row.id ? (
                  <li
                    className="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-surface-alt px-3 py-2.5"
                    key={row.id}
                  >
                    <Input
                      value={editingName}
                      onChange={(event) => setEditingName(event.target.value)}
                      required
                    />
                    <Button
                      size="sm"
                      type="button"
                      aria-disabled={busy ? true : undefined}
                      onClick={() => void saveRename(row.id)}
                    >
                      {copy.save}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      type="button"
                      onClick={() => setEditingId(null)}
                    >
                      {copy.cancel}
                    </Button>
                  </li>
                ) : (
                  <li
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-surface-alt px-3 py-2.5"
                    key={row.id}
                  >
                    <div className="flex min-w-0 flex-col gap-0.5">
                      <strong className="text-sm font-bold">{row.name}</strong>
                      <span className="text-2xs text-ink-3">
                        {fill(copy.created, {
                          date: formatDate(row.created_at, locale),
                        })}
                      </span>
                    </div>

                    {!readOnly && (
                      <div className="flex shrink-0 gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          type="button"
                          onClick={() => startRename(row)}
                        >
                          {copy.rename}
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          type="button"
                          aria-disabled={busy ? true : undefined}
                          onClick={() => void remove(row)}
                        >
                          {copy.delete}
                        </Button>
                      </div>
                    )}
                  </li>
                ),
              )}
            </ul>
          )}

          {error && (
            <p
              role="alert"
              className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
            >
              {error}
            </p>
          )}

          {notice && (
            <p
              role="status"
              className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
            >
              {notice}
            </p>
          )}

          {readOnly ? (
            <span className="text-2xs leading-snug text-ink-3">
              {copy.readOnly}
            </span>
          ) : (
            <form
              className="flex flex-wrap items-end gap-2"
              onSubmit={(event) => void add(event)}
            >
              <Field label={copy.name} className="min-w-52 flex-1">
                <Input
                  value={name}
                  placeholder={copy.namePlaceholder}
                  onChange={(event) => setName(event.target.value)}
                  required
                />
              </Field>
              <Button
                type="submit"
                aria-disabled={busy ? true : undefined}
                aria-busy={busy ? true : undefined}
              >
                {busy ? copy.adding : copy.add}
              </Button>
            </form>
          )}
        </>
      )}
    </section>
  );
}
