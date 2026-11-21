/**
 * The public origin that shared links point at, or null when it is not known.
 *
 * `window.location.origin` is the wrong answer inside the Tauri shell: it
 * serves this same export from a WebView origin a browser cannot open, so an
 * invitation link built from it would be undeliverable. Configuration is the
 * only place the public address exists, which is why a link is offered only
 * when `NEXT_PUBLIC_APP_URL` is set.
 */
export function siteOrigin(): string | null {
  const configured = process.env.NEXT_PUBLIC_APP_URL?.trim();

  return configured ? configured.replace(/\/+$/, "") : null;
}
