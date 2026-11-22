import type { AuditEntry } from "@nata/api-client";

import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { formatDateTime } from "@/lib/format";
import type { Dictionary, Locale } from "@/lib/i18n";

import { Intro } from "./intro";
import { actionLabel, detailParts, entityLabel } from "./audit-detail";

/** The screen asks for the newest 50; the server caps a page at 200. */
const LAST_PAGE = 200;

type AuditSectionProps = {
  entries: AuditEntry[] | null;
  failed: boolean;
  loading: boolean;
  limit: number;
  locale: Locale;
  onRetry: () => void;
  onShowMore: () => void;
  t: Dictionary;
};

export function AuditSection({
  entries,
  failed,
  loading,
  limit,
  locale,
  onRetry,
  onShowMore,
  t,
}: AuditSectionProps) {
  const audit = t.settings.audit;
  // The server returns at most `limit` rows, so a full page means there may be
  // more behind it. At the cap the button would only refetch the same rows.
  const more = limit < LAST_PAGE && entries !== null && entries.length >= limit;

  return (
    <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
      <Intro title={audit.title} body={audit.body} />

      {loading && (
        <LoadingRegion label={audit.loading} className="flex flex-col gap-2">
          <SkeletonList rows={4} />
        </LoadingRegion>
      )}

      {failed && !loading && (
        <div className="flex flex-col gap-3">
          <p className="text-sm text-muted-foreground">{audit.error}</p>
          <Button
            variant="secondary"
            size="sm"
            type="button"
            className="self-start"
            onClick={onRetry}
          >
            {audit.retry}
          </Button>
        </div>
      )}

      {!loading && !failed && entries && entries.length === 0 && (
        <p className="text-sm text-muted-foreground">{audit.empty}</p>
      )}

      {!loading && !failed && entries && entries.length > 0 && (
        <>
          <ul className="flex flex-col gap-2">
            {entries.map((entry) => (
              <AuditRow key={entry.id} entry={entry} locale={locale} t={t} />
            ))}
          </ul>

          {more && (
            <div className="flex justify-end">
              <Button variant="ghost" size="sm" type="button" onClick={onShowMore}>
                {audit.showMore}
              </Button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

function AuditRow({
  entry,
  locale,
  t,
}: {
  entry: AuditEntry;
  locale: Locale;
  t: Dictionary;
}) {
  const parts = detailParts(entry, locale, t);

  return (
    <li className="flex flex-wrap items-start justify-between gap-3 rounded-lg border border-border bg-surface-alt px-3 py-2.5">
      <div className="flex min-w-0 flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <strong className="text-sm font-bold">
            {actionLabel(t, entry.action)}
          </strong>
          <Badge variant="neutral" className="px-2 py-0.5 text-2xs">
            {entityLabel(t, entry.entity_type)}
          </Badge>
        </div>

        {parts.length > 0 && (
          <p className="m-0 text-xs leading-relaxed text-ink-2">
            {parts.map((part, index) => (
              <span key={part.label}>
                {index > 0 && <span aria-hidden="true"> · </span>}
                {part.label}{" "}
                <strong className="font-semibold text-ink tabular-nums">
                  {part.value}
                </strong>
              </span>
            ))}
          </p>
        )}
      </div>

      {/* `min-w-0` and `break-words` keep a long name from forcing the row past
          the card instead of wrapping onto its own line. */}
      <div className="flex min-w-0 flex-col gap-0.5 sm:items-end">
        <span className="break-words text-xs font-semibold">
          {entry.actor_name ?? t.settings.audit.systemActor}
        </span>
        <span className="text-2xs text-ink-3">
          {formatDateTime(entry.created_at, locale)}
        </span>
      </div>
    </li>
  );
}
