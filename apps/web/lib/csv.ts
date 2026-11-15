/**
 * A CSV file for a spreadsheet, not for the API.
 *
 * Excel and Sheets both read a comma-separated file with dot decimals, so the
 * values stay exactly as the API sent them instead of being formatted for the
 * screen: "Rp 1.234.567" would arrive as text and could not be summed.
 *
 * The BOM is added on download, because Excel on Windows reads a UTF-8 file
 * without one as the system code page and mangles anything outside ASCII.
 */

/** Leading characters a spreadsheet would read as the start of a formula. */
const FORMULA = /^[=+@\t\r]/;

function escapeField(value: string): string {
  // A product name a user typed could start with "=" and run as a formula
  // when the file is opened. An apostrophe keeps it text; a negative number
  // is left alone so the column still sums.
  const safe =
    FORMULA.test(value) || (value.startsWith("-") && !Number.isFinite(Number(value)))
      ? `'${value}`
      : value;

  const quote = /[",\n\r]/.test(safe) || safe !== safe.trim();

  return quote ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/**
 * Rows to a CRLF document, which every spreadsheet accepts. An empty row
 * separates sections; the file ends with a newline.
 */
export function toCsv(rows: string[][]): string {
  return rows.map((row) => row.map(escapeField).join(",")).join("\r\n") + "\r\n";
}

/** Hands the text to the browser as a file the user keeps. */
export function downloadCsv(filename: string, text: string): void {
  const blob = new Blob(["\uFEFF", text], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");

  link.href = url;
  link.download = filename;
  // A detached link is enough for Chrome and the Android WebView; Safari only
  // follows a click on an element that is in the document.
  document.body.append(link);
  link.click();
  link.remove();
  // Revoking in the same tick can cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
