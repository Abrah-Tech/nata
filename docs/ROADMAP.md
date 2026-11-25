# Execution Roadmap — Nata

This document turns `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` into an order of work that an agent can execute. The product specification stays in that document; this one defines **what gets built first, when a task counts as done, and when it is safe to move on**.

## How to use this document

1. Read `CODEBUDDY.md` first, then this document.
2. Work on **one phase at a time**. The gate at the end of a phase must pass before the next phase starts.
3. Pick one task ID, create a branch named `feat/<task-id>-<slug>`, and open a PR. Do not bundle several large tasks into a single PR.
4. Tick the task checkbox in the same PR once the task is done.
5. Gates marked **(owner: user)** are business decisions, not agent work. Do not fill them in yourself.
6. If a task touches an item under **Open decisions**, stop and ask. Do not pick a default on your own.
7. Do not add scope outside the active phase, especially the full POS, CRM, loyalty, and multi-outlet features.

## Phase map

| Phase | Focus | Gate to advance | Status |
| --- | --- | --- | --- |
| 0 | Technical foundation + validation | 3–5 prospects willing to try and pay; Android build runs on 2 low-end phones | code done; gate pending (P0-08) |
| 1 | Stock, batches, expiry | 3 businesses use it instead of a notebook for 2 weeks | code done; gate pending |
| 2 | Recipes, production, HPP | Users trust the HPP and use it to set prices | code done; gate pending |
| 3 | Purchasing, sales, subscription | First paying customer | code done; gate pending |
| 4 | Expansion | Stable monthly retention | not started |

## Platform targets

Tauri 2 ships one UI (Next.js) and one Rust shell to desktop, Android, and iOS, but "one codebase" does not mean "no platform work". Shared: the React UI, `nata-core`, the API contract, and sync logic. Different: build toolchain, native permissions, app lifecycle, store review, and billing rules.

| Target | Delivery | Toolchain | Notes |
| --- | --- | --- | --- |
| Web browser | Next static export on a host | Bun + Next | No Tauri APIs; server behavior comes from Axum |
| Android | Tauri APK/AAB | JDK 17 + SDK/NDK, provisioned by the dev shell | First mobile target; test on low-end devices |
| Desktop | Tauri bundle | webkitgtk on Linux | Useful for development and back-office |
| iOS | Tauri app | macOS + Xcode + Apple developer account | After Android is stable; needs a Mac or macOS CI |

Rules:

- Core flows must not depend on a platform-only plugin. Native features sit behind a capability check with a web fallback.
- Billing is sold on the web; do not add in-app purchase before the store rules are decided (D11).
- The server stays the source of truth. The client may run `nata-core` for previews only.

## UI source of truth

The UI design lives in pen.dev (`nata.pen`, 25 top-level frames). Implement screens from that file; do not invent layouts. Desktop frames are 1440 wide, mobile frames are 390 wide.

| Group | Frames | Phase |
| --- | --- | --- |
| Auth | 13 · Desktop — Login WhatsApp OTP, M1 · Login OTP | 0 |
| Stock | 01 · Ringkasan, 02 · Stok Bahan, 03 · Batch & FEFO, M2 · Beranda, M3 · Stok Bahan | 1 |
| Production | 04 · Produk & HPP, 05 · Resep & BOM, 06 · Catat Produksi, M4 · Catat Produksi, M5 · Produksi Tersimpan | 2 |
| Commerce | 07 · POS & Penjualan, 08 · Rekomendasi Belanja, M6 · Kasir | 3 |
| Alerts and team | 09 · Alert WhatsApp, 10 · Tim & Langganan, 11 · Laporan, 12 · Pengaturan, M7 · Alert | 1–3 |
| System | 00 · Design System, Button, Badge, Field, Nav Item, Stat Card, POS Tile, Cover, Prototype | 0 |

Rules:

- Reuse the pen.dev components and tokens instead of restyling per screen.
- The Tauri Android build follows the mobile frames; the web build must cover both desktop and mobile layouts.
- New screens require a design update first; do not ship an undesigned screen.

### Design backlog

Screens whose backend already ships but whose frame is not built yet. They are
blocked on a pen.dev design update (`nata.pen`); the plan's usage limit has to
clear before any of them can be drawn, so keep them out of a phase gate.

| Frame | Backend ready | Still needed |
| --- | --- | --- |
| 09 · Alert WhatsApp | `alert_rules`, `/alert-rules`, `/alerts`, `/alerts/preview`, `alert_job`, `alert_delivery`, `PATCH /alert-rules/{id}` | Only the pen.dev drawing. The rules editor, the open versus resolved history, and the digest preview are built on `/settings` |
| 12 · Pengaturan (suggested) | `audit_logs`, `/audit-logs`, `nata_db::audit::record` | Only the pen.dev drawing. The owner-only audit feed is built on `/settings` |

Do not build these screens before their frame exists in the design file.

**Exception (2026-11-21):** 03 · Batch & FEFO (PR #28, `apps/web/app/batches`) and 10 · Tim & Langganan (PR #29, `apps/web/app/team`) were built from the tokens and patterns that already ship in `/stock` and `/settings`, because the backend for both is complete and the design system covers the screens. Both frames still need a pen.dev drawing; until it lands, the existing-token build stands in for it.

The acceptance half of 10 is `apps/web/app/join` (PR #31). `POST /auth/accept-invitation` already existed but had no client method and no screen, so the code `/team` produces had nowhere to go. It follows the same tokens; the invite dialog now offers a `/join?token=…` link next to the raw code.

09 · Alert WhatsApp needed API work before it could be built: `/alert-rules` was read only, so `PATCH /alert-rules/{id}` (PR #39) writes `days_before`, `channel`, and `is_active`, guarded by `CanManageTenant` like `PUT /settings`. The screen is `apps/web/app/settings/alerts-section.tsx`, on `/settings`: the rules editor plus the open versus resolved history. Same standing as 03, 10, and 12 — the frame still needs a pen.dev drawing.

The digest preview is `GET /alerts/preview`, readable with `CanReadMaterials` like the rest of the screen. It composes the message server-side through the same `alerts::message::compose` the sender uses, so a preview cannot drift from the text that actually goes out, and it selects the same pending set as `nata_db::alert_delivery::pending` — open, undelivered, and under the attempt cap — so what the owner reads is what the next run would send. A tenant with nothing pending gets `count` 0 and an empty message rather than a digest about nothing, and the screen says so in its own words. The preview is rendered as the message body with its line breaks intact, with a copy button for the owner who wants to forward it.

12 · Pengaturan's audit half is `apps/web/app/settings/audit-list.tsx` (PR #38). `audit_logs`, `GET /audit-logs`, and `nata_db::audit::record` were already complete and owner-only, so the screen reads them from `/settings` behind the `audit_read` permission. The frame still needs a pen.dev drawing; until it lands, the existing-token build stands in for it.

### UI polish backlog

Audit 2026-11-18 against `apps/web`: the component library plus the screens that
already run. Verdict **Approve** — no `HIGH`, so nothing blocks a release. Five
`MEDIUM` and three `LOW` findings are recorded here as backlog. All eight are
fixed on `fix/ui-polish-backlog` (2026-11-19); the table is the record of what
was wrong. Principles come from `better-ui`: the cursor is the cheapest affordance,
hover belongs behind `@media (hover:hover)`, name only the properties that
change, outer radius equals inner radius plus padding, and an icon's stroke
tracks the weight of the text beside it.

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `apps/web/components/ui/button.tsx:7`, `apps/web/components/ui/select.tsx:15`, `apps/web/app/stock/page.tsx:601`, `apps/web/app/login/page.tsx:335`, `:363`, `:393` | Preflight is deliberately off (`apps/web/app/globals.css`) and no hand-written reset sets `cursor`, so `<button>` and `<select>` keep the arrow. Only `apps/web/app/stock/page.tsx:694` opts in with `cursor-pointer` | `cursor-pointer` in the `buttonVariants` base, on `Select`, and on the stock tabs and the raw login buttons | A control that looks static but is clickable is the affordance bug the audit started from; the one corrected row proves the convention is the exception |
| MEDIUM | `apps/web/components/ui/button.tsx:7` | The press transition names `transform`, but Tailwind v4's `active:scale-[0.96]` writes the CSS `scale` property, so the 150ms ease-out press never runs | Name `scale` in the transition list | The pressed state snaps instead of easing, and the transition is the point of the 0.96 scale |
| MEDIUM | `apps/web/components/ui/input.tsx:9`, `apps/web/app/login/otp-input.tsx:93` | Text fields have no `cursor` and no hover state, only a `focus-visible` ring | `cursor-text`, and a hover border or background on the field | A field that shows no text cursor reads as read-only; the field is also the one component with no hover feedback at all |
| MEDIUM | `apps/web/app/stock/page.tsx:590`, `:592`, `:601`, `:603` | The tab strip has no `cursor-pointer`; the inactive tab's only hover cue is `hover:text-ink-2`, one step of a four-step scale; the strip is `overflow-x-auto` with `gap-4.5` and `shrink-0` tabs, so on a 360px phone the four mobile labels plus their count pills can push the last tab out of view | `cursor-pointer`; inactive hover `hover:text-ink` and a border-color step; let the four tabs share the strip width (`flex-1 justify-center`, `min-w-0`) instead of scrolling | The tab is the primary filter of the stock screen and the least affordant control on it; the mobile label set is what the i18n resources already define |
| MEDIUM | `apps/web/components/ui/button.tsx:7`, `apps/web/components/ui/select.tsx:15`, `apps/web/app/stock/page.tsx:601`, `apps/web/app/login/page.tsx:335`, `:363`, `apps/web/app/login/otp-input.tsx:93` | No `-webkit-tap-highlight-color: transparent`, so the WebView paints its own gray flash over the app's `active:scale-[0.96]` pressed state | Set it on the button base, `Select`, the stock tabs, the raw login buttons, and the OTP cells | Two pressed states stacked reads as a laggy double response on the cheap Android phones the shell targets |
| LOW | `apps/web/app/app-shell.tsx:265`, `:271`, `:281`, `:291`, `:299` | Nav pill `rounded-[28px]` with `p-1.5` and a 1px border around items at `rounded-[22px]` | Items at `rounded-[21px]` (28 − 6 − 1) | Outer radius must equal inner radius plus padding; the 1px step is visible where the active pill meets the bar |
| LOW | `apps/web/components/ui/dialog.tsx:39`, `apps/web/app/stock/batch-panel.tsx:117` | `overflow-y-auto` with no `overscroll-behavior` | `[overscroll-contain]` | Scrolling past the end of a sheet drags the page behind it |
| LOW | `apps/web/app/login/page.tsx:195`, `:327` | `Check` at `strokeWidth={3}` beside `font-bold` text | `strokeWidth={2.5}` | Stroke tracks text weight: 1.5 at 400, 2.5 at 700. The check is the success cue of the login flow |

**Verified from code:** Tailwind v4's `hover:` compiles inside `@media (hover:hover)`
(confirmed in the built CSS), so no hover state latches on touch; no
`transition-all`, no `will-change`, no `stroke="#…"` or `fill="#…"` in an icon;
the press scale is `0.96` with `disabled:active:scale-100` and a `motion-reduce:`
opt-out; the dialog and overlay animations have a `prefers-reduced-motion` block.
The built CSS also carries `transition-property: …, scale` on the button base,
`-webkit-tap-highlight-color: transparent` in the base reset, and
`overscroll-behavior: contain` on the dialog, batch panel, and desktop aside.

**Not verified:** the rendered cursor, the native option list, the 360px tab
strip, and the tap flash. No browser was available during the audit, so those
four are read from the source, not observed. The native picker itself is a
deliberate convention for the Android WebView and stays; only the closed
control is styled.

**Fixed 2026-11-19.** `cursor-pointer` is in the `buttonVariants` base and on
`Select`, the stock tabs, the raw login buttons, and the locale switch;
`cursor-text` and a hover border are on `Input` and the OTP cells; the stock tab
strip shares its width on mobile (`flex-1`, `min-w-0`, `truncate`, `gap-2`) and
keeps `gap-4.5` from `sm` up. The tap highlight is one global reset in
`apps/web/app/globals.css` rather than a class per control, so every current and
future control gets it. The nav items are `rounded-[21px]`; the dialog, batch
panel, and desktop aside are `overscroll-contain`; the login `Check` icons are
`strokeWidth={2.5}`; and the button transition names `scale`.

### Accessibility backlog

Audit 2026-11-19 against `apps/web`, following the UI polish pass. Verdict was
**Block** on two `HIGH` findings; both are fixed on `fix/web-accessibility`
(2026-11-19) and the pass is re-run as **Approve**. Principles come from
`better-accessibility`: native elements first, `aria-disabled` only where the
control must stay focusable, `:focus-visible` over `:focus`, validate on submit
instead of disabling the button, a stable live region rather than one that
mounts with its text, and one visible `<main>` per screen.

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| HIGH | `apps/web/app/sales/page.tsx:611` | `<input type="file" accept=".csv,text/csv" required>` inside a form with no label, `aria-label` or `aria-labelledby` | `aria-label={t.sales.import.chooseFile}` (new key), `name="file"`, `aria-invalid`/`aria-describedby` on the inline error, and the input is focused when validation fails | 4.1.2 Name, Role, Value (A): the control had no accessible name, so a screen reader announced only "file" |
| HIGH | `apps/web/app/stock/page.tsx:626`, `:634` | Search input with `outline-none border-0`; the wrapper only changed border on hover, so keyboard focus drew nothing but the caret | `focus-within:border-brand focus-within:shadow-[0_0_0_3px_var(--brand-soft)]` on the wrapper | 2.4.11 / 1.4.11 (AA) and the skill's missing-focus-indicator trigger |
| MEDIUM | `apps/web/app/app-shell.tsx:132`, `:264`, `:275` | No `<main>` landmark and no skip link; the mobile nav links had no `aria-current`; the two "coming soon" items carried `aria-disabled` on a `<span>` that was never focusable, so their `title` tooltip was pointer-only | `<main id="main" tabIndex={-1}>` around the content, a `.skip-link` as the first focusable element, `aria-current="page"` on the active mobile link, and the coming-soon items are `role="link"`, `tabIndex={0}`, `aria-disabled` with an `sr-only` reason | 2.4.1 Bypass Blocks, 4.1.2, and the skill's disabled-tooltip rule: keyboard and screen-reader users could not reach the reason |
| MEDIUM | `apps/web/app/stock/page.tsx:586`-`624` | `role="tablist"` with no `aria-controls`, no `tabpanel`, no `tabIndex` roving, and no key handler; only Tab and click worked | `id`/`aria-controls`/`role="tabpanel"` with `aria-labelledby`, roving `tabIndex`, and ArrowLeft/Right/Up/Down/Home/End with automatic activation | 2.1.1 Keyboard and the APG tabs pattern |
| MEDIUM | `apps/web/app/stock/page.tsx:554`, `apps/web/app/settings/page.tsx:291`, `apps/web/app/products/page.tsx:194`, `apps/web/app/sales/page.tsx:394`, `apps/web/app/login/page.tsx:409`, `apps/web/app/stock/page.tsx:460`, `apps/web/app/onboarding/page.tsx:191`, `:311`, `apps/web/app/sales/page.tsx:643` | `{message && <… role="status">}` mounted at the same moment as its text, so repeated announcements get dropped | A shared `components/status.tsx` renders the `role="status"` node on every paint and only swaps its text; the sales import result and the onboarding result use an always-mounted region; the offline queue is announced from a stable `sr-only` line instead of the section that appears and disappears | 4.1.3 Status Messages and the skill's stable-region rule |
| MEDIUM | `apps/web/app/login/page.tsx:313`, `:373`, `:68`, `:88` | Submit was `disabled` until the form was valid and the handlers returned silently, so an empty submit did nothing visible; two `<h1>`s rendered at `lg`; the raw Change/Resend/tenant buttons and the mailto link had no focus ring | Submit stays enabled and validates on submit with `aria-invalid`, `aria-describedby`, and focus on the failing field; the brand heading is a styled `<p>`; every raw control gets `focus-visible:outline-2 … outline-brand` | 3.3.1 Error Identification, 2.4.7 Focus Visible, and the skill's one-heading outline rule |
| MEDIUM | `apps/web/app/production/page.tsx:412`, `:613`, `apps/web/app/settings/page.tsx:395`, `:572`, `apps/web/app/stock/material-dialog.tsx:163`, `apps/web/app/stock/stock-in-dialog.tsx:266`, `apps/web/app/products/product-dialog.tsx:154`, `apps/web/app/products/recipe-dialog.tsx:361`, `apps/web/app/sales/page.tsx:572`, `:624`, `apps/web/app/login/page.tsx:313`, `:373`, `:393` | `disabled` was set while the request was in flight, which drops focus to `<body>` on the button the user just activated | `aria-disabled="true"` plus `aria-busy="true"` (styled in the `buttonVariants` base) with a guard at the top of the handler; genuinely unavailable states keep native `disabled` | The skill's submit-behaviour rule: the button keeps focus and a repeat click is blocked in code |
| MEDIUM | `apps/web/app/layout.tsx` | No `document.title` or focus handling on a client-side route change | A `RouteAnnouncer` client component sets the title from the pathname and moves focus to `#main`, skipping the first paint and locale-only changes | 2.4.3 Focus Order and the skill's SPA route-change recipe |
| MEDIUM | `apps/web/app/products/page.tsx:317`, `:328`, `apps/web/app/sales/page.tsx:537`, `apps/web/app/products/recipe-dialog.tsx:340`, `apps/web/app/stock/page.tsx:525`, `:537` | Icon-only buttons announced a generic action ("Edit", "Retry"), so two rows sounded identical | Named labels from new `*Named` keys, falling back to the generic string when the row has no selection | 4.1.2 and the skill's accessible-names rule |
| MEDIUM | `apps/web/app/stock/page.tsx:708` | The `role="table"` grid had a header only above `sm`; on mobile the three visible columns had no header, and the desktop row ended with a `role="columnheader" aria-hidden="true"` spacer | A mobile-only header row matching the three mobile columns, and the contradictory spacer removed | 1.3.1 Info and Relationships |
| MEDIUM | `apps/web/components/ui/field.tsx` | The hint sat inside the wrapping `<label>`, so it became part of the control's accessible name instead of its description | The hint moved outside the label and is referenced through a new `hintId` prop; the six call sites pass `aria-describedby` | 4.1.2 and the skill's label/hint rule |
| LOW | `apps/web/app/globals.css` | No `.skip-link` and no `forced-colors` handling; several focus rings are `box-shadow`, which forced-colors mode removes | `.skip-link` in the components layer, and an unlayered `@media (forced-colors: active)` block that restores a `Highlight` outline on everything focusable except `main` | 2.4.7 and the skill's forced-colors rule |
| LOW | `apps/web/app/login/otp-input.tsx:93` | Bare `focus:` on the cells, `one-time-code` on all six, and no `name` | `focus-visible:`; `one-time-code` and `name="otp"` on the first cell only, plus an imperative `focus(index)` handle for the form | 2.4.7 and the skill's autocomplete/name rule |
| LOW | `apps/web/app/stock/batch-panel.tsx:195` | `<p aria-busy="true">…</p>` announced "busy" with no words | `role="status"` with an `sr-only` label from the new `t.stock.panel.loadingMovements` | 4.1.3 and the skill's live-region rule |
| LOW | `apps/web/components/ui/dialog.tsx:39`, `:68` | `outline-none` with no replacement on the dialog containers | `focus-visible:outline-2 … outline-brand`; Radix moves focus to the first control inside, which draws its own ring | 2.4.7 and the skill's no-bare-outline rule |
| LOW | `apps/web/app/products/page.tsx:262` | The recipe disclosure had `aria-expanded` but no `aria-controls`, so the region it toggles was not associated | `aria-controls={`recipe-${product.id}`}` and the matching `id` on the region | 4.1.2 and the APG disclosure pattern |

**Verified from code:** `bun run --filter web typecheck` and
`bun run --filter web build` pass; `bun test apps/web/tests` is 6 pass, 0 fail.
Every icon-only control in the audited screens now carries a name, all six
`Field` hints are referenced by their control, and the only `role="status"`
nodes left are the always-mounted ones.

**Not verified:** no browser, screen reader, or forced-colors session was
available, so the live-region announcements, the `aria-disabled` styling, the
skip-link geometry, the mobile table header, and the focus move after a route
change are read from the source rather than observed.

### Color contrast backlog

Audit 2026-11-19 against `apps/web`, following the accessibility pass. Verdict was
**Block** on three `HIGH` findings; all are fixed on `fix/web-color-contrast`
(2026-11-19) and the pass is re-run as **Approve**. Principles come from
`better-colors`: measure the rendered pair rather than the page background, name
primitives by hue and semantics by role, and never leave a color literal in a
component when a token carries the same value.

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| HIGH | `--ink3` in `apps/web/app/globals.css:22` | `#8b978f` used as secondary text 107× (`text-ink-3`, `placeholder:text-ink-3`) across 18 files: `app/stock/batch-panel.tsx:96`, `app/stock/page.tsx:547`, `app/reports/page.tsx:121`, `app/products/page.tsx:269`, `app/login/page.tsx:253`, `app/production/page.tsx:343`, `app/dashboard/page.tsx:200`, `app/app-shell.tsx:145`, `app/settings/page.tsx:349`, `app/sales/page.tsx:455`, `app/purchasing/page.tsx:170`, `app/onboarding/page.tsx:235`, `components/ui/table.tsx:38`, `components/ui/select.tsx:25`, `components/ui/input.tsx:9`, `components/ui/field.tsx:32`, `components/ui/dialog.tsx:52`, `app/products/recipe-dialog.tsx:75` | `#5f6e68` | 1.4.3 AA (4.5:1): measured 3.03 on `--surface`, 2.92 on `--surface-alt`, 2.80 on `--bg`, 3.03 on the mobile nav's `bg-white/95`. The tertiary step failed on every surface, and at 9.5–13px the hints, table headers and empty states read as a smudge. Darkening it narrows the step to `--ink2`; no tertiary tone lighter than `--ink2` can hold 4.5:1 on white |
| HIGH | `--amber` in `apps/web/app/globals.css:28` | `#9a6210` on `--amber-soft` `#fbefd9` in `components/ui/badge.tsx:13`, `app/settings/page.tsx:438`, `app/settings/page.tsx:493`, `app/login/page.tsx:483` | `#8f5b0e` | 1.4.3 AA: 4.47 on the warning chip, missing the ratio by 0.03 |
| HIGH | `apps/web/app/login/page.tsx:246` | `#5f8a7a` version text on the sunken panel `#143a2f` | `--on-brand-strong` `#8fb3a6` | 1.4.3 AA: 3.22. The lightest text on the login panel was a raw literal, and the only one that failed |
| MEDIUM | `--border-strong` (`--color-input`) in `apps/web/app/globals.css:19` | `#ccd4cb` on `--surface` in `components/ui/input.tsx:9`, `components/ui/select.tsx:15`, `app/login/otp-input.tsx:108`, `app/sales/page.tsx:614`, `app/sales/page.tsx:654`, `components/ui/button.tsx:17` | `#7f8c83` | 1.4.11 AA (3:1): 1.52. A control is a white island on the page background, so its border is the only edge — and `hover:border-ink-3` (3.03) already passed while the resting border failed |
| MEDIUM | `apps/web/app/health-badge.tsx:48`, `:55`, `apps/web/app/production/page.tsx:654` | Status pill borders `#f3c3bd` (1.57) and `#b7e0d8` (1.43); selected summary card border `#c8e2d6` (1.37) | `border-destructive`, `border-brand`, `border-brand` | 1.4.11 AA: the boundary that identifies a status or a selected card sat below 3:1 |
| MEDIUM | `apps/web/app/login/page.tsx:198`, `:205`, `:212`, `:224`, `:229`, `:237`, `:242`, `:246`, `apps/web/app/health-badge.tsx:48`, `:55`, `apps/web/app/production/page.tsx:654`, `apps/web/components/ui/button.tsx:13`, `:19`, `apps/web/components/ui/skeleton.tsx:13` | Raw hex in component files: the login dark ramp, the status borders, the button hover fills, the skeleton sheen | Role tokens: `--brand-raised`, `--brand-sunken`, `--on-brand`, `--on-brand-strong`, `--on-brand-accent`, `--danger-strong`, `--secondary-hover`, `--skeleton-sheen` | The skill's notation rule: a hardcoded value does not follow when the token it derives from moves, so the dark panel could not be re-tuned without editing eight call sites |
| LOW | `apps/web/app/globals.css` | `--color-border-strong` was referenced by `app/stock/page.tsx:658` but never defined in `@theme`, so `hover:border-border-strong` compiled to nothing | `--color-border-strong: var(--border-strong)` | Use a token in its role: the utility exists now and the stock tabs' hover border is visible |

**Verified from code:** `bun run --filter web typecheck` and
`bun run --filter web build` pass; `bun test apps/web/tests` is 6 pass, 0 fail.
Every fixed pair was recomputed from the declared tokens — `--ink3` 4.94–5.36,
`--amber` on `--amber-soft` 5.03, `--border-strong` 3.24–3.51, `--on-brand-strong`
on `--brand-sunken` 5.47 — and the built stylesheet carries the new values and
the new utilities.

**Not verified:** no browser session, so the rendered backgrounds (the mobile
nav's `bg-white/95`, the login panel, the alpha fill on the purchasing header)
are computed from the declared tokens rather than sampled, and forced-colors
appearance is read from the source.

### Typography backlog

Audit 2026-11-19 against `apps/web`, following the color pass. Verdict was
**Approve** (no `HIGH`); the `MEDIUM` and `LOW` rows are fixed on
`fix/web-typography` (2026-11-19). Principles come from `better-typography`:
`16px` input text so iOS Safari does not zoom, a scale with few off-step sizes,
`rem` so the reader's font size still applies, font smoothing once on the root,
headings balanced and paragraphs pretty, and `user-select: none` only on a drag
or gesture surface. The project's dense scale (body `text-sm` at 12.5px) is a
documented convention, so the findings target deviations from it rather than
the scale itself.

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `apps/web/components/ui/input.tsx:9`, `apps/web/components/ui/select.tsx:15`, `apps/web/app/stock/page.tsx:686` | `text-base` (13px) and `text-sm` (12.5px) inside an `<input>`/`<select>` | `text-[1rem] sm:text-base` and `text-[1rem] sm:text-sm` | The skill's iOS no-zoom rule: Safari zooms the whole page when an input's text is under `16px`, and the only escape was a full-page pinch on every field |
| MEDIUM | `apps/web/app/sales/page.tsx:654` | `file:text-xs` (11px) on the import button | `file:text-sm` (12.5px) | Size floor: UI text starts at `14px` and rarely goes below `12px`; 11px is below the project's own `text-xs` step |
| MEDIUM | `apps/web/app/globals.css:105`-`124` | The whole type scale declared in `px` | `rem` (`0.59375`–`2.75rem`), with `--text-5xl: 2.75rem` added | 1.4.4 Resize Text: a `px` scale ignores the reader's browser font size, so the only way to enlarge text was zoom |
| MEDIUM | `apps/web/app/app-shell.tsx`, `apps/web/app/login/page.tsx`, `apps/web/app/onboarding/page.tsx`, `apps/web/app/page.tsx`, `apps/web/app/settings/page.tsx`, `apps/web/app/health-badge.tsx`, `apps/web/app/locale-switch.tsx`, `apps/web/app/login/otp-input.tsx` | 24 arbitrary sizes off the scale: `9`, `9.5`, `10`, `10.5`, `11`, `11.5`, `12`, `13`, `14`, `16`, `17`, `19`, `21`, `34`, `42`, `44px` | The nearest named step (`text-2xs`, `text-xs`, `text-sm`, `text-base`, `text-lg`, `text-xl`, `text-2xl`, `text-4xl`, `text-5xl`); `9px` and `8.5px` rose to the `9.5px` floor | Use a type scale with semantic names: 16 distinct sizes meant no two screens could be compared, and a `42px` headline sat between the `40px` and `48px` steps |
| MEDIUM | `apps/web/app/production/page.tsx:315` | Section `<h2>` at `text-xl` (17px) | `text-lg` (15px) | Heading hierarchy: every other section heading in the app is `text-lg`, so this one overpowered its parent section |
| LOW | `apps/web/components/ui/label.tsx:15`, `apps/web/components/ui/field.tsx:26`, `apps/web/app/login/page.tsx:403` | `select-none` on label text | Removed | `user-select: none` belongs on a drag or gesture surface; a label is content the reader may want to copy |
| LOW | `apps/web/app/globals.css` | Only `-webkit-font-smoothing: antialiased` | `-moz-osx-font-smoothing: grayscale` beside it, set once on the root | Font smoothing on the root: Firefox on macOS synthesizes a heavier face without it, so the app rendered a different weight than Safari |
| LOW | `apps/web/app/globals.css` | No wrap control | `text-wrap: balance` on `h1`–`h6` and `text-wrap: pretty` on `p`, in the base layer | A heading with a ragged last line and a paragraph ending on a lone word; a utility on a specific node still wins over the base rule |

**Verified from code:** `bun run --filter web typecheck` and
`bun run --filter web build` pass; `bun test apps/web/tests` is 6 pass, 0 fail.
No `text-[Npx]` utility remains outside the two deliberate `text-[1rem]` input
fixes. The built stylesheet carries `.text-2xs{font-size:.59375rem}`,
`.text-5xl`, `.text-\[1rem\]`, `-moz-osx-font-smoothing: grayscale`,
`text-wrap: balance` and `text-wrap: pretty`.

**Not verified:** no browser session, so iOS zoom at a 13px input, `rem`
scaling under a non-default root size, and the line breaking from `balance` and
`pretty` are read from the source rather than observed.

### Interface backlog

Audit 2026-11-19 against `apps/web`, following the typography pass. Verdict was
**Block** on three `HIGH` findings; all are fixed on `fix/web-interface`
(2026-11-19). Principles come from `better-interface`: escalation triggers are
`HIGH` on sight, and a trigger is a failure whatever the style guide says. The
one `LOW` candidate — the reports table's `min-w-[720px]` scroll edge — was
withdrawn: the clipped column is itself the cue and a horizontally scrolling
table is the standard pattern.

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| HIGH | `apps/web/app/stock/page.tsx:544`, `apps/web/app/products/page.tsx:274`, `:289`, `:301`, `:313`, `:476`, `apps/web/app/dashboard/page.tsx:232`, `:312`, `apps/web/app/login/page.tsx:384`, `apps/web/app/reports/page.tsx:228`, `apps/web/app/stock/page.tsx:788` | Truncated values with no way back: the offline queue material, the product name/price/stock/recipe, a recipe ingredient, two dashboard rows, the login contact, the waste-report material and the stock supplier | `title` on every truncated node, carrying the full value | The "truncated content with no way to reach the full value" trigger: each value was clipped by `truncate` and nothing exposed the rest |
| HIGH | `apps/web/app/stock/page.tsx:639` | Five `flex-1` mobile filter tabs plus a count pill left ~14px for a 14px label, so "Critical" rendered as "C…" | The tab row wraps on mobile (`flex-wrap`, `sm:flex-nowrap`) and each chip sizes to its content, so no label truncates | 1.4.10 Reflow at 320px, and the same trigger: the filter name was hidden with no way back |
| HIGH | `apps/web/app/settings/page.tsx:617` | The pending-payment withdraw was a neutral ghost button that hard-deletes the payment (`apps/api/src/billing.rs:247`) with no confirmation, undo or distinct treatment | A `Modal` confirmation naming the amount and plan from a new `settings.billing.withdrawConfirm` key, and the button takes `variant="destructive"` | The destructive-action trigger: one tap destroyed a payment record, and the API has no undo. A Radix dialog rather than `window.confirm`, because the shell runs in an Android WebView where a JS dialog is not guaranteed to render |
| MEDIUM | `apps/web/components/ui/button.tsx:29` | The dense tables used `size="icon"` (40px) overridden to `size-7` (28px), below the 44px touch aim | A new `iconSm` variant — 28px visual with a `before:-inset-2` ring for a 44px target — at `apps/web/app/products/page.tsx:325`, `:339`, `apps/web/app/stock/page.tsx:568`, `:580`, `:850`; the two side-by-side groups move to `gap-4` so the rings never overlap | 2.5.8 AA (24px) was already met, but the skill's 44px touch target was not; the pseudo-element widens the target without breaking the dense layout |
| MEDIUM | `apps/web/app/sales/page.tsx:728` | The sale card header was one `flex` row — datetime, channel badge and an `extrabold` total, ~320px of content in a 288px card | `flex-wrap` on the header, with `min-w-0` + `truncate` + `title` on the datetime | 1.4.10 Reflow at 320px: the total was pushed off the card |

**Verified from code:** `bun run --filter web typecheck` and
`bun run --filter web build` pass; `bun test apps/web/tests` is 6 pass, 0 fail.
The built stylesheet carries the new variant's `before:-inset-2` ring and
`before:content-['']`, and no `truncate` node in `apps/web` is left without a
`title`.

**Not verified:** no browser session, so the 320px reflow of the sale header
and the stock filter tabs, the geometry of the extended hit areas, and the
confirm dialog are read from the source rather than observed. The API-side
hard delete in `billing.rs` is unchanged; the confirmation is a client guard,
not a server one.

---

## Phase 0 — Foundation + validation

Goal: the repo builds, multi-tenant auth works, and the Android app installs on cheap phones. Runs in parallel with the validation interviews.

| ID | Task | Done when |
| --- | --- | --- |
| P0-01 | Scaffold the workspace: Cargo workspace (`apps/api`, `crates/core`, `crates/domain`, `crates/db`), Bun workspace (`apps/web`, `apps/shell`, `packages/api-client`), `.gitignore`, `rust-toolchain.toml` | `cargo build --workspace` and `bun run --filter '*' build` pass on an empty scaffold |
| P0-02 | Complete the dev shell: bun (runtime + package manager), postgresql, sqlx-cli, and a way to run Postgres locally | `nix develop` provides `cargo`, `bun`, `psql`, `sqlx`; `sqlx database create` succeeds |
| P0-03 | CI: fmt, clippy `-D warnings`, tests, web typecheck + static build, sqlx offline check | Pipeline is green on the first PR |
| P0-04 | Account migrations: `tenants`, `users`, `memberships`, `invitations` + RLS policies | An integration test proves tenant A cannot read or write tenant B rows, including through joins |
| P0-05 | API skeleton: axum, tracing, env-based config, uniform error type, `/healthz`, OpenAPI | `/healthz` returns 200; OpenAPI is generated; error responses are consistent |
| P0-06 | OTP + JWT auth: provider abstraction (dev stub), OTP request/verify, JWT carrying `user_id` + `tenant_id`, tenant selection endpoint | End-to-end login in dev; tokens without `tenant_id` are rejected; multi-tenant users can switch tenants |
| P0-07 | Bootstrap web + shell: Next static export, login page, generated TS client, Tauri Android init | `bun run --filter web build` produces `out/`; the debug APK installs and runs on 2 low-end Android phones |
| P0-08 | Validation interviews with 10–15 F&B producers **(owner: user)** | 3–5 agree to try the product and pay |
| P0-09 | RBAC matrix in `nata-domain`: `Role`, `Permission`, `can()` | Matrix covers owner/admin/production/viewer; pure, no IO, covered by golden tests |
| P0-10 | `AuthContext` extractor and `/me`: JWT → active membership → role | Suspended membership gets 403; a role change applies on the next request without re-login; `/me` returns the effective permissions |
| P0-11 | Permission-gated routes + invitation flow | Every mutating route declares a permission; removing or demoting the last owner fails inside the transaction; 403 paths have integration tests |
| P0-12 | UI i18n foundation: `id` and `en` dictionaries, locale switch, no hardcoded copy | Login and dashboard render in both locales; a missing key fails the build |

**Phase 0 gate:** P0-01 through P0-12 are done and P0-08 yields 3–5 prospects. Without this gate, phase 1 can still be built, but do not claim product validation.

**Status (2026-11-14):** P0-01 through P0-07 and P0-09 through P0-12 are built. P0-08, the interviews, is the open gate.

**Status (2026-11-25):** the interview notes are tracked at `interview.md` in the repository root — eleven findings, each checked against the code, so a finding that is already built is not mistaken for a gap. The navigation order and the cashier's reach to POS shipped in PR #43, the dashboard summary in PR #45, a per-batch stock take prefilled with the ledger's quantity in PR #46, per-supplier purchase price history in PR #47, production suggestions in PR #48, and the overhead component of the HPP in PR #49. This branch adds the profit and loss report (P3-07): revenue, cost of goods sold, and gross profit over the usage window, which needed the HPP to be complete first. Three findings are still open: the cashier role and offline POS need a product decision before any code, and export is CSV while PDF and automatic backup are still to do.

---

## Phase 1 — Stock and expiry

Goal: replace the notebook for material stock and expiry dates.

| ID | Task | Done when |
| --- | --- | --- |
| P1-01 | Master data: `units`, `unit_conversions`, `suppliers`, `materials` (CRUD + base unit validation) | Materials have a base unit, `min_stock`, and `shelf_life_days`; conversions round-trip consistently |
| P1-02 | Batch stock-in: short form → insert `material_batches` + a `purchase` movement in one transaction | `qty_remaining = qty_received`; the movement is recorded; entry takes under 15 seconds on a phone |
| P1-03 | Stock queries: on-hand per material, active batches, movement history | On-hand equals `sum(qty_remaining)`; depleted batches do not appear in the active list |
| P1-04 | FEFO allocator in `nata-core` (pure function, no IO) | Golden tests: ordered by `expires_at`, nulls last, `received_at` as tie-break, insufficient stock returns an explicit shortfall |
| P1-05 | Stock corrections: waste, expired, adjustment | Every change goes through `stock_movements`; no quantity changes without a trail |
| P1-06 | Daily alert job: `alert_rules`, fills `alerts`, uses `SKIP LOCKED` | Running it twice produces no duplicates; H-7/H-3 follow the configured rules |
| P1-07 | WhatsApp sender: provider abstraction, message templates, retry, delivery status | A message is delivered in staging; failures are recorded and can be retried |
| P1-08 | Daily stock UI: material list with status, stock-in form, batch detail, expiring filter | An admin can finish the daily stock entry without leaving the flow |
| P1-09 | Phase 1 dashboard: critical stock + expiring materials | Single screen; loads in under 2 seconds on a slow network |
| P1-10 | Onboarding: F&B material/recipe templates + Excel import (see D1) | A new tenant can reach 20 materials within 10 minutes |

**Alert job.** Run `cargo run -p nata-api --bin alert_job` daily from cron. It is idempotent and safe to re-run: tenants are claimed with `skip locked`, inserts are guarded by open-alert unique indexes, and alerts resolve when their condition stops holding. Alert creation is parallel-safe; delivery is not claimed, so do not overlap two runs while sending. The job needs a database role that can list every tenant (the migration role, or any role with `bypassrls`); the per-tenant work still runs inside a tenant-scoped transaction. Alerts store structured data, not prose; the sender composes one digest per tenant and effective channel (falling back to the owner's other contact), records every attempt, and retries failures on the next run.

**Phase 1 gate:** at least 3 businesses use the product instead of a notebook for 2 weeks.

**Status (2026-11-14):** P1-01 through P1-10 are built. What is missing is the two-week usage, not code.

---

## Phase 2 — Recipes, production, HPP

Goal: an HPP that users trust and use to set selling prices. This is the densest phase; do not start it before FEFO (P1-04) is stable.

| ID | Task | Done when |
| --- | --- | --- |
| P2-01 | Versioned recipes: `recipes` + `recipe_items`, one active version per product, old versions immutable | Editing a recipe creates a new version; past production still points at its original version |
| P2-02 | Finished products: `products` + `current_recipe_id` | A product without an active recipe cannot be produced |
| P2-03 | Material requirement in core: `qty × (1 + waste_pct) × (batch ÷ yield_qty)` | Golden tests cover rounding; `yield_qty = 0` is rejected |
| P2-04 | Transactional production order: FEFO consume, decrement batches, write `production_consumptions` + `stock_movements`, increase product stock, roll back on shortage (see D3) | No production leaves a batch negative; a failure leaves no partial data |
| P2-05 | HPP in core + snapshot into `production_orders` | Golden tests; historical HPP does not change when material prices rise |
| P2-06 | Production UI: pick product + qty, preview requirements, confirm, show HPP + margin | One flow without reloads; required versus actual usage is visible |
| P2-07 | Offline outbox (see D2): Tauri SQLite queue for stock-in, idempotent sync endpoint | Stock entries made offline sync after reconnect without duplicates; a rejected entry stays visible as failed instead of disappearing |
| P2-08 | Tenant labor rate: `tenants.labor_rate_per_hour` plus a settings endpoint; production snapshots `labor_cost` from `recipe.labor_minutes × rate` | New orders include labor in HPP; existing orders keep their material-only snapshot |
| P2-09 | Tenant overhead rate: `tenants.overhead_rate_per_hour` on the same settings endpoint; production snapshots `overhead_cost` from the same `recipe.labor_minutes` at that rate | New orders include overhead in HPP; the preview lists material, labor, and overhead separately; existing orders keep their snapshot |

**HPP scope note.** P2-05 snapshotted material cost only; P2-08 adds labor and P2-09 adds overhead, both pricing `recipe.labor_minutes` at the matching tenant rate. Overhead was an open question — a monthly amount allocated per unit was the first recommendation, and an hourly rate was chosen instead because the recipe already carries the run's duration, so one basis serves both components and a tenant can reason about what an hour of production costs. Either rate left at zero keeps that component out of the HPP, and a past order's snapshot never changes.

**Phase 2 gate:** users trust the HPP and start using it to set prices.

**Status (2026-11-17):** P2-01 through P2-08 are built, and D8 is now built on the backend: `product_batches` is the finished-goods ledger, production writes one batch per run with `expires_at = produced_on + shelf_life_days`, a sale allocates batches FEFO and snapshots the weighted cost, and reports value finished goods from the batches. The batch screen still waits for a pen.dev design update.

---

## Phase 3 — Purchasing, sales, subscription

| ID | Task | Done when |
| --- | --- | --- |
| P3-01 | Purchase recommendation: average usage × lead time + safety stock, capped by shelf life | The number can be explained from movement data; materials without history are clearly flagged |
| P3-02 | Simple sales: `sales` + `sale_items`, decrement product stock, channel | Product stock stays consistent after manual entry or import |
| P3-03 | Business dashboard: margin per product, stock value, waste | Numbers match the ledger query, not a separate calculation in the UI |
| P3-04 | Subscription + plan limits: 14-day trial, user/material/recipe/warehouse caps, checked before an invitation is created | Tenants over the limit are rejected with a clear message; a downgrade never deletes data |
| P3-05 | Billing (see D4): manual activation or gateway, subscription status, expiry | Trial expiry switches to read-only mode; data stays intact |
| P3-06 | Report export (Pro) | CSV/Excel respects the same filters as the on-screen view |
| P3-07 | Profit and loss: revenue, cost of goods sold, and gross profit over the usage window | The bottom line equals revenue minus the cost snapshotted on each sale, so a later price never rewrites it; a sold line with no cost makes the period's cost unknown rather than zero |

**Phase 3 gate:** first paying customer.

**Status (2026-11-25):** P3-07 is built. `GET /reports` also returns `profit_loss`, and the reports screen leads with it. Gross profit is revenue minus the cost recorded on each sale, the same snapshots the margin table uses. Overhead is inside the cost of goods rather than a second line, because the HPP already carries it; a sold line with no cost snapshot makes the period's cost and profit null, the rule the overview screen follows. Export is CSV only for now.

**Status (2026-11-15):** P3-01 through P3-06 are built. Report export is a Pro and Business entitlement: the report stays readable on every plan, the file is built in the browser from the payload the screen already shows, so the CSV cannot drift from the view, and the API reports `can_export` so the button can explain itself instead of disappearing. Billing is manual, per D4: the owner records a transfer, the operator checks it and runs `cargo run -p nata-api --bin billing -- verify <payment-id>`, and access runs to the paid period plus a 7-day grace before the tenant goes read-only. Reads and the billing routes stay open in read-only mode so a tenant can always pay its way back.

---

## Phase 4 — Expansion

| ID | Task | Notes |
| --- | --- | --- |
| P4-01 | Multi-warehouse | The columns and the `warehouses` table landed in `0021_warehouses.sql` (D6); this task is the API and the UI |
| P4-02 | RBAC extension: custom roles or per-tenant overrides, only if customers ask | Base roles ship in P0-09 through P0-11 |
| P4-03 | POS add-on: multi-payment, QRIS | Only after the phase 3 gate passes |
| P4-04 | Advanced reports + priority support (Business) | |
| P4-05 | iOS build | Requires a Mac and a developer account |

---

## Cross-cutting tracks

| Track | Content | When |
| --- | --- | --- |
| Security | OTP rate limiting, hashed invitation tokens, JWT rotation, audit log for sensitive actions | Starts in phase 0, reviewed every phase |
| Observability | Structured tracing, error tracking, Postgres backup + restore drill | Tracing in phase 0; backups before any real customer data |
| Testing | Core golden tests, RLS integration tests against real Postgres, e2e stock → production → HPP | Core tests from P1-04; e2e in phase 2 |
| Performance | Test on low-end phones and 3G at the end of every phase | Phases 0, 1, and 2 |
| Data | F&B seed templates, safe migrations, import/export | Phases 1 and 3 |

**Status (2026-11-18):** the audit log is built (`audit_logs`, migration `0020_audit_log.sql`). It records the actions that have no other history: membership, role, invitation, and settings changes, plus payment creation, deletion, and operator verification. Domain documents keep their own author (`sales`, `production_orders`, `recipes`) and a stock correction is already a signed `stock_movements` row, so those are not duplicated. Reading it needs `AuditRead`, which only the owner holds, because the feed also reports on billing. JWT rotation is built: `JWT_SECRET_PREV` keeps the outgoing secret usable for one token TTL while new tokens are signed by `JWT_SECRET`, so a rotation does not log everyone out, and `HASH_PEPPER` separates the OTP and invitation pepper from the signing keys so a rotation cannot invalidate a pending code. The procedure is in `deploy/vps/README.md`. Nothing remains in this track.

**Status (2026-11-18):** tracing is in place from phase 0. The backup and restore drill is now `deploy/vps/backup.sh`: it dumps, restores into a throwaway database beside the source, and compares the schema, an exact row count, and every column's value (numeric summed exactly, anything else hashed per row) before dropping the scratch copy. `--self-test` proves the comparison notices a deliberate change. Run it locally with `docker compose up -d db` or on the VPS from `/srv/nata/current/deploy/vps/`, where the script ships with each release. Remaining in this track: a real restore drill against Neon, which needs a second project or branch to restore into.

**Status (2026-11-18):** the promised end-to-end flow is now a test. `apps/api/tests/e2e_stock_to_hpp.rs` walks one tenant from two batches at different prices through FEFO consumption, the HPP snapshot (material plus labor), a sale, and the report, and checks the numbers still agree at the end: 3.375 kg × 9000 of material left, 6 × 4162.50 of finished goods, and 60,000 of revenue against 16,650 of cost. A second test writes a kilo of flour off after production and checks that the valuation and the waste report move while the order's snapshot does not.

---

## Authorization model (RBAC)

Decision: enforce in the API, define the matrix in `nata-domain`, and keep RLS for tenant isolation only. RBAC does not belong in `nata-core`: it needs database state and core must stay pure.

| Layer | Responsibility |
| --- | --- |
| Postgres RLS | Row isolation by `app.tenant_id`; never role logic |
| `nata-domain` | `Role`, `Permission`, and the `can()` matrix; pure and unit-tested |
| `nata-api` | JWT → active membership → `AuthContext`; a permission extractor guards routes |
| Service/handler | Resource invariants: last owner, invitation limits, plan caps |
| Web/Tauri UI | Renders from the permissions returned by `/me`; never the source of truth |

Rules:

- The JWT carries `user_id` + `tenant_id` only. Resolve the role per request from `memberships` so suspension and role changes apply immediately.
- `/me` returns the active tenant, role, and effective permissions. The UI does not duplicate the matrix.
- Every mutating route declares a permission. No ad-hoc role checks inside handlers.
- The declaration is part of the handler signature: `Require<CanManageMembers>` in `nata-api::auth::guard` resolves the role and rejects with `403 permission_denied`.
- A member may only manage or grant roles strictly below their own; an owner may also manage another owner so a tenant can hand over. Invitations never carry the owner role.
- Plan limits are entitlements, not permissions: check them in the service layer before creating a member, material, recipe, or warehouse.
- Role checks never live in RLS policies; RLS only answers "which tenant owns this row".

### Pre-tenant lookups

Login and invitation acceptance both happen before a tenant is known, and they are not the same case:

- **OTP login** does not assume a tenant. A user may belong to zero tenants (first signup) or several. Resolve the user by email/phone through a narrow `SECURITY DEFINER` function, set `app.user_id`, and list that user's memberships. The self-select policy on `memberships` allows this across tenants without opening other rows. Signup creates the tenant and the first owner membership afterwards.
- **Invitation acceptance** always happens inside an existing tenant, but the accepting user does not know which one yet. Resolve `tenant_id`, role, status, and expiry from `token_hash` through a second definer function, then set `app.tenant_id` and continue under normal RLS.
- Both functions return only the columns needed for that step, pin `search_path`, and are revoked from `PUBLIC`.

---

## Schema issues to fix before the first migration

The draft SQL in the product document is not ready to use as-is. Settle these in the first migration instead of deferring them.

1. **Child tables without `tenant_id`** — `recipe_items`, `sale_items`, `production_consumptions`, `unit_conversions`. RLS cannot be attached directly. Choose either an added `tenant_id` column or an `EXISTS` policy against the parent table. `EXISTS` keeps the schema smaller but is slower and easier to get wrong.
2. **`unit_conversions` has no `tenant_id`** while `units.tenant_id` is nullable. **Decided:** conversions are global and seeded with the system units (kg→g, l→ml, lusin→pcs); material-specific conversions such as "1 sack = 25 kg" are deferred until a customer needs them.
3. **No `warehouse_id`** even though Pro sells 2 warehouses. **Decided (2026-11-22):** a nullable `warehouse_id` on `material_batches`, `product_batches`, and `stock_movements`, plus the `warehouses` table that owns the names; see D6 and `0021_warehouses.sql`.
4. **`products.stock_qty` is a single number** — finished goods cannot be FEFO-allocated or expiry-tracked. **Decided (2026-11-17):** `product_batches` is the ledger and `products.stock_qty` stays as a read-only cache written in the same transaction; see D8.
5. **`stock_movements.qty`** has no sign convention or constraint. **Decided:** inbound is positive and outbound is negative, enforced by the `stock_movements_sign` check added in `0005_batches.sql`; only `adjustment` may go either way.
6. **`materials.last_cost`** is derived data that can drift. **Decided:** it is a read-only cache, written only by stock-in from the latest purchase; batches stay authoritative.
7. **`tenants.plan` is free text** — make it an enum or check constraint so plan limits can be trusted.
8. **Indexes for the daily job** — add an index on `material_batches(tenant_id, expires_at)` for active batches, plus a fast way to detect materials below `min_stock`. **Decided:** the partial FEFO index in `0005_batches.sql` serves the expiry scan; `0006_alerts.sql` adds open-alert unique indexes and a partial index for the open-alert list. Low stock is a per-material aggregate, which is fine at SME scale.
9. **`production_consumptions` has no primary key** — add an `id` or a composite PK on `(production_order_id, batch_id)`.

---

## Open decisions

Do not decide these alone. Ask the user, then record the outcome here.

| ID | Question | Initial recommendation |
| --- | --- | --- |
| D1 | Excel import: onboarding for every plan, or a Pro feature? The product document says both | Onboarding for every plan with a row cap; data entry is the number one product risk |
| D2 | Offline sync: start in phase 1 or 2, and which conflict policy? | **Decided (2026-11-11):** phase 2. The Tauri shell queues stock-in only in a local SQLite outbox; each entry carries a client-generated UUID so replay is idempotent. A queued entry the server rejects stays in the outbox marked failed and is shown to the user; no last-write-wins merge and no silent drop. The browser build stays online-only |
| D3 | Shortage during production: block or allow partial? | Block and show the shortfall list; core already returns an explicit shortfall |
| D4 | Billing: manual transfer with admin verification, or a gateway? | **Decided (2026-11-12):** manual verification now, gateway-ready by construction. A payment is a row carrying amount, method, proof, and the period it covers; one `activate()` writes the subscription and is called by the admin endpoint today and by a gateway webhook later. Gateway (Midtrans or Xendit hosted checkout: QRIS, virtual account, e-wallet) once there are ~10 paying customers or manual reconciliation becomes the bottleneck. No card auto-debit in this phase. Expiry means read-only after a 7-day grace period; data is never deleted |
| D5 | OTP channel: WhatsApp, email, or both? | WhatsApp primary, email fallback |
| D6 | Multi-warehouse: prepare now or later? | **Decided (2026-11-22):** nullable columns now, UI in phase 4. `0021_warehouses.sql` adds `warehouses` and a nullable `warehouse_id` to `material_batches`, `product_batches`, and `stock_movements`, each behind a composite foreign key to `warehouses (id, tenant_id)` so a row cannot name another tenant's location. `ON DELETE RESTRICT` keeps a location while stock still points at it, and `SubscriptionUsage::warehouses` counts real rows instead of a hardcoded `0`. P4-01 is endpoints and UI only |
| D7 | HPP rounding: how many decimals are stored and displayed? | Store 4 decimals, display whole Rupiah, round up when setting a selling price |
| D8 | Do finished goods need batches and expiry tracking? | **Decided (2026-11-17):** yes. `product_batches` becomes the ledger and `products.stock_qty` stays only as a read-only cache written in the same transaction. Expiry comes from a new `products.shelf_life_days` (`expires_at = produced_on + shelf_life_days`, null when unset). Finished-goods movements extend `stock_movements` with a nullable `product_batch_id` and the `production`/`sale` types. First PR is backend + tests; the batch screen waits for a pen.dev design update. Existing stock is backfilled into one synthetic batch per product |
| D9 | Crate and package naming: `nata-` prefix? | **Decided (2026-11-16):** the workspace uses the `nata-` prefix across crates, binaries, packages, and the Android app id. |
| D10 | RBAC: put the role in the JWT or resolve it per request? | Resolve per request from `memberships`; JWT stays `user_id` + `tenant_id` so revocation is immediate |
| D11 | Mobile monetization: web checkout or in-app purchase? | Web checkout; keep billing read-only inside the app until store rules are settled |
| D12 | UI copy language: the pen.dev frames use Indonesian, the project convention says English | **Decided:** UI copy is localized through i18n with `id` and `en`; code, API messages, and docs stay English |

---

## Definition of Done

A task is done when:

- Every acceptance criterion in its table row is met.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test --workspace` pass.
- `bun run --filter web build` still produces a static export.
- Any new migration that touches a business table has a tenant isolation test.
- The PR includes the task ID, a summary of the change, evidence (test output or screenshots), and any remaining risks.
- No scope was added outside the active phase.
