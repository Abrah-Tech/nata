/**
 * A polite live region that is always in the accessibility tree. Only its text
 * changes: a region mounted at the same moment as its text is not announced
 * reliably, and repeated announcements from a region that keeps appearing and
 * disappearing get dropped.
 *
 * The visible line is rendered separately so the announcement never depends on
 * the styling, and the screen-reader-only region stays out of the layout.
 */
function StatusMessage({
  message,
  className,
}: {
  message: string | null;
  className?: string;
}) {
  return (
    <>
      <p role="status" className="sr-only">
        {message}
      </p>
      {message && <p className={className}>{message}</p>}
    </>
  );
}

export { StatusMessage };
