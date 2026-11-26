# CODEBUDDY.md

This file provides guidance to CodeBuddy Code when working with code in this repository.

## Repository state

Greenfield. `main` has no commits yet; the only artifacts are `flake.nix` and the product plan. Do not assume code exists — verify before referencing modules.

Two documents are the source of truth:

- `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` — product spec, schema draft, pricing, stack decisions.
- `docs/ROADMAP.md` — execution order, task IDs, gates, open decisions.

Read both before starting work. Implement only inside the current roadmap phase; do not pull deferred scope (full POS, CRM, multi-outlet) forward.

## Commands

Everything runs inside the Nix dev shell. The flake currently provisions only the Rust toolchain; Bun, Postgres, and sqlx-cli are added by roadmap task P0-02. Use Bun for everything JavaScript/TypeScript — no npm, pnpm, or yarn.

```bash
nix develop          # enter dev shell
direnv allow         # optional, .envrc already uses flake
```

Rust:

```bash
cargo build --workspace
cargo test --workspace
cargo test -p nata-core fefo              # single module
cargo test -p nata-core -- --nocapture
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Local sign-in: the API loads `.env` at startup. With `DEV_OTP_CODE` set (default `000000`), debug builds use that fixed code for any contact instead of a random one, so authenticated routes are reachable without reading the log. Release builds always randomize.

OTP delivery is picked from the environment, never from code: `EMAIL_PROVIDER=brevo` with `BREVO_API_KEY`, `EMAIL_FROM`, `EMAIL_FROM_NAME`, and `WHATSAPP_PROVIDER=fonnte` with `FONNTE_TOKEN`. With none set, debug builds write the message to the log and release builds answer `otp_delivery_failed`. Transports live in `apps/api/src/notify/` and are reused for alerts in P1-07; adding a provider is one module, not a change to the OTP path. A contact may request five codes per ten minutes.

A code inside its TTL is not replaced: `POST /auth/otp/request` answers `202` with `{ "expires_in": <remaining>, "pending": true }` and sends nothing, so a second request cannot rotate the code out from under a message already on its way. A code that has burned its five attempts is dead and does not block a fresh one — otherwise the "request a new code" the `too_many_attempts` error suggests would be a dead end until the TTL runs out. On the web, a pending answer lands on the code screen with a notice, and the resend countdown runs to the code's expiry because a resend before then cannot produce a new code.

`/login` bounces a visitor who already holds a valid session to `/dashboard`. The token is checked with `/me` rather than trusted, so an expired one is cleared there instead of looping between the two screens.

Database:

```bash
sqlx database create
sqlx migrate run
sqlx migrate add <name>                    # forward-only, numbered
cargo sqlx prepare --workspace             # refresh .sqlx after query changes
```

Local Postgres for development and RLS tests:

```bash
docker compose up -d db
docker compose down                    # stop, keeps the volume

DATABASE_URL=postgres://nata:nata@127.0.0.1:5432/nata cargo test -p nata-db
```

Daily jobs (run from cron in deployment):

```bash
cargo run -p nata-api --bin alert_job     # fills, resolves, and sends alerts; safe to re-run
```

Manual billing, run by the operator rather than from cron:

```bash
cargo run -p nata-api --bin billing -- list
cargo run -p nata-api --bin billing -- verify <payment-id> --by <label>
```

The job claims tenants with `skip locked`, so it needs a database role that can see every tenant — the migration role or a role with `bypassrls`. Per-tenant work still runs in a tenant-scoped transaction.

Delivery goes through the same `notify::Router` as OTP. Each tenant gets one digest per channel, not one message per alert; every attempt is recorded (`delivery_status`, `attempts`, `last_error`), a failure is retried on the next run up to three attempts, and a tenant without an owner contact is marked `skipped` instead of retrying forever. If the rule's channel has no contact but the owner has the other one, the digest uses that channel; a tenant with no owner at all is the only case that is skipped. Links in the message point at `APP_URL`.

`#[sqlx::test]` creates a throwaway database per test, so the URL must point at a server where the user can create databases. The compose user is the cluster superuser, which satisfies that requirement for development only.

Web and shell (after `apps/web` and `apps/shell` exist):

```bash
bun install
bun run --filter web dev
bun run --filter web build                 # must stay a static export
bun run --filter web lint
bun run --filter web typecheck
bun test apps/web/tests                     # the browser-side units, via Bun's runner
bun run --filter shell tauri android dev
```

## Architecture

Three runtimes, one calculation core:

- `apps/web` — Next.js static export; the UI for both browser and Tauri WebView.
- `apps/shell` — Tauri 2 wrapper. Android first, iOS after Android is stable.
- `apps/api` — Axum. The only place with server logic (auth, jobs, HTTP).
- `crates/core` — pure functions: FEFO allocation, HPP, unit conversion, reorder planning. No IO, no sqlx, no async. Used by `api` and `shell` so client and server produce identical numbers.
- `crates/domain` — domain types and the API contract (utoipa/specta). Generates `packages/api-client`.
- `crates/db` — sqlx queries and data access.
- `migrations/` — Postgres DDL, including RLS policies.

Data flow: UI → generated TypeScript client → Axum → `nata-db`/`nata-core` → Postgres. Core may also run inside Tauri for local previews, but the server stays the source of truth.

### Web UI

Tailwind v4 runs through PostCSS and is loaded by `app/globals.css`, which is the only style entry: the nata.pen tokens, their `@theme` mapping, the layer order `theme, base, components, utilities`, and the global resets (`box-sizing`, border width, margins, body background and font) in `base`. Preflight is deliberately not loaded, so the reset is hand-written in `base`: box-sizing, the `html`/`body` margins, the body background and font, and the margins a browser gives headings and lists. Those are the ones that fight the gap-based layouts every screen uses — an `<h1>` with its default `0.67em` and an `<ol>` with its `40px` indent make a card look broken. Paragraphs keep their browser margin, because a screen that stacks two of them without a gap expects the default. A utility still wins over `base`.

Components live in `components/ui` and are copied shadcn source, not a dependency: `button`, `input`, `label`, `select` (native, for the Android WebView), `field`, `card`, `badge`, `dialog` (Radix, plus `DialogSheet` for the batch panel), `table`, `skeleton`. Do not add `@radix-ui/themes`, which ships its own look. `components/modal.tsx` is the thin wrapper every form dialog uses, so focus trap, Escape and scroll lock come from Radix rather than from each form. Style with the mapped tokens (`bg-card`, `text-ink-3`, `border-border`), never a raw palette class, or the design system drifts from the .pen file. Controls are 10px, surfaces 14px, and `text-sm` is the 12.5px body size.

Loading uses skeletons, never a spinner or a "Memuat…" line: `components/skeletons.tsx` exports `LoadingRegion` (the `role="status"` wrapper carrying the translated label for screen readers), `SkeletonStats`, `SkeletonTable`, `SkeletonList` and `SkeletonForm`. A page composes the shape it is about to render. `useMe` keeps the last `/me` answer for the tab, so navigating between pages does not blank the shell.

`AppShell` takes `me` as optional. While `/me` is in flight a page renders `<AppShell active="…">` with its skeleton inside, so the sidebar, topbar and tab bar are present from the first paint; the tenant and user blocks show placeholders and sign-out stays disabled until the session resolves. Only the error state renders outside the shell, because there is no session to hang the chrome on.

### Stock model

Stock is a batch ledger, not a scalar. `material_batches.qty_remaining` is on-hand per batch; `stock_movements` is the audit trail. Never add a single `stock` column to `materials`. Finished goods are a batch ledger too: `product_batches.qty_remaining` is on-hand per batch and `products.stock_qty` is only a read-only cache written in the same transaction. `product_batches.unit_cost` is null when the cost cannot be known (stock migrated from the scalar column with no production order), and a sale that takes such a batch is reported as cost incomplete instead of valued at a made-up number.

### Recipes

Recipes are versioned, never edited in place. Creating a recipe version deactivates the previous one, and every version keeps its own `recipe_items`; production will point at the exact version it consumed so historical HPP never changes. A partial unique index enforces one active version per product, and the database rejects a recipe whose product belongs to another tenant. `products.current_recipe_id` follows the newest version and is null when the product has no recipe. Reading a product is its own `product_read` permission rather than a side effect of `recipe_read`: the register needs the catalogue, while the recipe behind it carries quantities and waste a cashier has no reason to see. Writing a product still needs `recipe_write`.

### Production

A production order is the only writer of `production_use` movements. It runs in one transaction: load the product's current recipe version, compute each material requirement, allocate batches FEFO with `nata_core::allocate`, and reject the whole run with `409 insufficient_stock` plus the shortage list (D3) before writing anything. On success it decrements `material_batches.qty_remaining`, appends a negative movement and a `production_consumptions` row per batch, then writes one `product_batches` row for the run. Its expiry is `produced_on + products.shelf_life_days`, or null when the product has no shelf life. A `production` movement records the inbound finished goods, and `products.stock_qty` is updated as a cache.

`POST /production-orders/preview` runs the same plan read-only and returns each material's requirement, the batches FEFO would take, and the HPP with margin. It returns null cost fields when any material is short, because a partial allocation would understate the HPP.

The order stores the `recipe_id` it consumed; creating a newer recipe version never rewrites it. Requirements round up to three decimals to match the batch ledger.

Cost is snapshotted in the same transaction: `material_cost` is the sum of `production_consumptions.qty × unit_cost`, and `unit_cost` is the total divided by `actual_qty` (four decimals, D7), so a later purchase at a different price never changes a past HPP.

Labor and overhead are priced the same way, from `tenants.labor_rate_per_hour` and `tenants.overhead_rate_per_hour` (both set through `PUT /settings`): a recipe's `labor_minutes` covers one batch, so a run of `qty` units costs `rate × minutes / 60 × (qty ÷ yield_qty)` for each component. `nata_core::hourly_cost` is the one function that turns those minutes into an amount, so the two components cannot drift onto different bases. Both rates are read once per plan and stored in the order, so changing either never rewrites history, and a zero rate leaves that component out of the HPP.

`/settings` is readable by any member because it explains the HPP and the shopping list they see; `PUT /settings` requires `tenant_manage` (owner, admin). Fields are optional: an omitted field keeps its stored value.

### Sales

A sale is the only writer that removes finished goods. `POST /sales` runs in one transaction: it locks the products it sells (`select … for update`), allocates each line's `product_batches` FEFO with `nata_core::allocate`, rejects the whole document with `409 insufficient_stock` plus every shortage when any line is short, then inserts `sales` and `sale_items`, decrements `product_batches.qty_remaining`, appends a `sale` movement per batch, and updates the `products.stock_qty` cache. Nothing is written before the check passes, so stock never drifts from the recorded sales. `sale_items.unit_cost` is the quantity-weighted cost of the batches the line took (`nata_core::sold_unit_cost`), null when any of them has no known cost.

`sale_items.price` is snapshotted at sale time. Changing `products.sell_price` later never rewrites past revenue, the same way a new purchase never rewrites a past HPP. `sale_items.unit_cost` snapshots the product's latest production cost at the same moment, so a later run at a different cost never rewrites an earned margin; it is null when the product had never been produced.

`client_ref` (uuid) is the idempotency key for imports and retries: replaying the same document returns the first sale with `200` and moves stock once. `sale_items` carries `tenant_id` so the composite foreign key proves the product belongs to the same tenant; RLS isolates it directly rather than through an `EXISTS` policy.

Channels are validated in the API (`toko`, `reseller`, `online`, `lainnya`) and stored as text, so adding one is a code change rather than a migration. `GET /sales?limit=` returns newest first with their lines. Reads need `sales_read` (every role); writes need `sales_write` (owner, admin, cashier).

`POST /sales/import?format=csv&tz_offset=` reads a CSV as raw bytes and groups rows into sales by the user's own `sale_ref`. That reference is hashed into the sale's `client_ref` (UUID v5, scoped to the tenant), so re-importing the same file reports `duplicate` instead of subtracting the stock twice; a row without a reference is a new sale every time, because the server has nothing to recognise it by. Each sale is its own transaction, so one bad row never rolls back the rest, and rows are reported with their spreadsheet line number. `tz_offset` is minutes east of UTC for timestamps written without an offset (Jakarta is 420). Only CSV is accepted: an xlsx date cell arrives as a serial number. The template ships at `apps/web/public/templates/sales-import.csv`.

Both entry points call `sales::write_sale`, so a typed sale and an imported one obey the same price snapshot, stock check, and duplicate rule.

### Plans

Every tenant starts on `trial` with `trial_ends_at` set to 14 days after creation. The limits live in `nata-domain::plan` (`Plan::limits`) and are entitlements, not permissions: the role says what a member may do, the plan says how much of it the tenant may have. They are checked in the service layer before a create, never in RLS.

`GET /subscription` returns the plan, the trial end, the limits, and current usage. Seats count active memberships plus pending invitations, because an invitation reserves a seat. Materials and recipes count only active rows: archiving a material frees a slot, and a new recipe version does not consume one. Warehouses have no table until P4-01, so that counter stays at zero.

A create over the limit is refused with `403 plan_limit_reached` and `details { resource, plan, limit, current }`. Only creates are checked — a downgrade never deletes or hides rows, it just blocks the next one.

### Members & invitations

`memberships` is the join table: `(tenant_id, user_id, role, status, invited_by, joined_at)`. `Role::rank` is owner 3, admin 2, production 1, cashier 1, viewer 0, and `can_manage(mine, target)` is `rank(mine) > rank(target)`, or an owner managing another owner. Cashier sits beside production rather than below it — the register and the kitchen are separate desks, and neither manages the other. It holds only `product_read`, `sales_read`, and `sales_write`, so no route that carries a cost or a margin is reachable from it; `apps/web/lib/landing.ts` sends it to `/sales` for the same reason.

The guards live in `apps/api/src/auth/members.rs`:

| Route | Guard |
| --- | --- |
| `GET /members`, `GET /members/invitations` | `CanReadMembers` |
| `POST /members/invitations`, `DELETE /members/invitations/{id}` | `CanInviteMembers` |
| `PATCH /members/{id}/role` | `CanChangeRoles` (owner only) |
| `PATCH /members/{id}/status`, `DELETE /members/{id}` | `CanManageMembers` plus `can_manage` |

`change_role` carries no `can_manage` check of its own because the guard is already owner-only; it does refuse to leave a business without an owner. The web client still gates the control on `can_manage`, for consistency with suspend and remove.

An invitation is a 64-character token from `secret::random_token()`. `POST /members/invitations` returns it once and stores only `secret::hash(&hash_pepper, token)` — the same construction an OTP code uses. It expires after `INVITATION_TTL_DAYS = 7`, and `ensure_capacity(Resource::Users)` runs on create, so a pending invitation reserves a seat until it is accepted, revoked, or expires.

`POST /auth/accept-invitation` takes `UserIdentity` rather than `AuthContext`, because an invitee may hold a tenant-selection token with no membership at all. It matches the token hash, requires the invitation to be pending and unexpired, requires the signed-in contact to match the invited one, then inserts the membership and marks the invitation accepted in one tenant-scoped transaction. It returns a fresh session token for the joined tenant, so the client replaces whatever it stored. Capacity is not re-checked on accept.

On the web, `/team` creates the invitation and shows the code, and `/join` accepts it. A signed-out visitor is sent to `/login` with the code parked in `sessionStorage` (`nata.invite`), which the login screen hands back after the OTP step. An invitation-only account's tenant-selection token is parked there too, never in `nata.token`, because `/me` requires a session token and `useMe` would clear it and bounce the visitor.

The invite dialog builds a `/join?token=…` link from `NEXT_PUBLIC_APP_URL` (`apps/web/lib/site.ts`). It is deliberately not derived from `window.location.origin`: the Tauri shell serves this same export from a WebView origin a browser cannot open. When the variable is unset the dialog offers only the raw code.

### Billing

Access is derived, never stored: `nata_domain::billing::access` reads `tenants.trial_ends_at` and `tenants.paid_until` and returns `active`, `grace` or `read_only`. It runs to the later of the two dates plus seven days of grace, so a support extension or an early payment can only lengthen access. A tenant with neither date stays active — locking out an account we cannot date would be a bug, not a policy.

Read-only is enforced once, in `AuthContext`: any non-GET request is refused with `403 subscription_expired` and `details { access }`. Reads stay open, and so do the `/billing` routes, because recording a payment is the way back. Nothing is deleted; a verified payment restores the account exactly as it was.

`payments` is a manual transfer the operator verifies (D4). The owner records the plan, method, months and a reference; the server derives the period — today, or the day after the current paid period when paying early — and prefills the amount from `Plan::monthly_price`. The row stays `pending` until the operator runs the `billing` bin, which calls `activate()` and marks it verified. `activate()` is the only function that writes a subscription, so a gateway webhook later replaces the CLI without touching anything else, and it only ever moves `paid_until` forward.

The bin needs the same role as `alert_job`: the pending list spans every tenant, so row level security must not hide it. The write itself still runs in a tenant-scoped transaction.

### Reports

`GET /reports` returns profit and loss, stock value, waste, and margin per product over the tenant's usage window.

Stock value is `sum(qty_remaining × unit_cost)` over material batches plus the same sum over `product_batches` for finished goods. It never uses `materials.last_cost`: a new purchase at a different price must not revalue stock already on the shelf.

Waste is the sum of `waste` and `expired` movements in the window, valued at each movement's own `unit_cost`.

Margin per unit is `sell_price − latest unit_cost`; a product that has never been produced has no cost and comes back flagged rather than estimated from its recipe. Historical margin is `revenue − cost_of_goods` from `sale_items.unit_cost`, so a sale written before that column existed makes the product's totals null and sets `cost_incomplete` instead of valuing the past at today's cost.

Profit and loss (`profit_loss` on the same response) is revenue minus cost of goods over the window, from the same `sale_items` snapshots, so it is the period's gross profit. Overhead is not a second line: the HPP already carries `production_orders.overhead_cost` into `product_batches.unit_cost` and then `sale_items.unit_cost`, so subtracting the period overhead again would charge it twice. Nata records no other operating cost, so gross profit is also net. A sold line with no cost snapshot makes the period's `cost_of_goods` and `gross_profit` null and sets `cost_incomplete`, the rule the overview screen follows.

Export is an entitlement, not a permission: `Plan::allows_export` is true for Pro and Business, and `GET /reports` returns `can_export` so the screen can say why the button is disabled instead of hiding it. Both files are built in the browser from the payload the screen is already showing, which is what makes "same filters as the view" true by construction. `apps/web/lib/report-csv.ts` is the spreadsheet: `apps/web/lib/csv.ts` owns quoting, CRLF, the formula guard, and the UTF-8 BOM Excel needs; the amounts stay raw so a spreadsheet can sum them. `apps/web/lib/report-pdf.ts` is the document: `reportPdf` turns the same payload into a pure model (sections, value rows, tables) and `renderReportPdf` draws it with `jspdf` and `jspdf-autotable`, which `downloadReportPdf` lazy-imports so the screen does not pay for them on load. The PDF uses the formatted values the screen shows, prints a null cost as a gap rather than zero, and marks a product whose cost is incomplete with `*` and a footnote. Both files keep their headers in the same i18n block as the screen.

### Purchasing

`GET /purchasing/recommendations` turns the movement ledger into a shopping list. Usage is the sum of `production_use`, `waste`, and `expired` movements over the tenant's usage window (`tenants.usage_window_days`, default 30, allowed 1–365), averaged per day; purchases and adjustments are excluded because neither says anything about consumption.

`nata_core::reorder_plan` targets `average_daily_usage × lead_time_days + min_stock` and orders `max(0, target − on_hand)`, rounded up to three decimals. When the material has a shelf life, the order is capped at `average_daily_usage × shelf_life_days − on_hand` so a long lead time cannot push a tenant into buying stock that expires before it is used; the response flags that case. Lead time comes from the material's default supplier and is zero when none is set.

A material with no outbound movement in the window comes back with `insufficient_history: true` and only closes the gap to `min_stock`; the shelf-life cap is skipped because a zero usage would cap every order to zero. Materials whose recommendation is zero are omitted.

### Offline

The Tauri shell records stock-in while offline; the browser build stays online-only (D2). A receipt is queued in SQLite (`stock_outbox`, created by the shell's plugin migration) with a client-generated `client_ref` UUID, and `POST /sync/stock-in` replays the queue when the device reconnects.

`client_ref` is the idempotency key: `material_batches` has a unique `(tenant_id, client_ref)` index, and replaying a receipt returns the batch the first attempt created with `200` instead of `201`. The sync endpoint runs each entry in its own transaction, so one rejected receipt never rolls back the others; the response reports `created`, `duplicate`, or `rejected` with the API error code per entry. Rejected entries stay in the queue marked failed and appear on the stock page to retry or dismiss — they are never dropped silently and never merged last-write-wins. The online route accepts the same `client_ref`, so a retry after a timeout is safe outside the queue too.

### Tenancy

Every business row carries `tenant_id`. Postgres RLS is the enforcement layer; app-level filters are defense in depth. The JWT carries `user_id` plus the active `tenant_id`, and each request transaction sets `app.tenant_id` and `app.user_id` via `SET LOCAL`. RLS is forced on every account table, and tests must `SET LOCAL ROLE` to a non-owner role because the table owner bypasses policies. Child tables without `tenant_id` (`recipe_items`, `production_consumptions`, `unit_conversions`) need either their own column or an `EXISTS` policy against the parent — decide before writing policies. `sale_items` took the column because its product reference also has to be tenant-checked.

### Business invariants

1. On-hand = `sum(qty_remaining)` over active batches.
2. Production consumes FEFO: earliest `expires_at` first, nulls last, then `received_at`.
3. HPP per unit = (material cost from `production_consumptions` + labor + allocated overhead) ÷ `actual_qty`.
4. Material requirement = `qty × (1 + waste_pct) × (production qty ÷ yield_qty)`.
5. The daily job fills `alerts` for expiring batches and below-min-stock materials, and must be idempotent.
6. Costs are snapshotted (`production_consumptions.unit_cost`, `production_orders.unit_cost`, `product_batches.unit_cost`, `sale_items.unit_cost`). Never recompute history from current prices.
7. Finished goods are a batch ledger too: a sale allocates `product_batches` FEFO, and `products.stock_qty` is a cache written in the same transaction, never the source of truth.

Use `rust_decimal` and Postgres `NUMERIC` end to end. Never `f64` for money or quantities.

## Gotchas

- Next static export: no SSR, route handlers, server actions, middleware, or ISR. Set `images.unoptimized = true`. Any server behavior belongs in Axum.
- Tauri mobile runs an old WebView on cheap Android phones. Test on low-end devices from the first build, not at the end.
- Server push (FCM/APNs) is out of MVP scope; alerts go through WhatsApp.
- sqlx compile-time checked queries need `DATABASE_URL` at build time or a committed `.sqlx` cache. CI must use one of them.
- Offline sync covers stock-in only (D2). Production and corrections need a live connection because they depend on current stock; do not widen the outbox without a new decision.
- `warehouse_id` is nullable on `material_batches`, `product_batches`, and `stock_movements`, and the `warehouses` table exists (D6, migration `0021`). Nothing writes the column yet — P4-01 owns the endpoints and the UI, so do not invent a warehouse workflow ahead of it.
- Dates: `expires_at` is a `date`, not a timestamp; avoid timezone drift when comparing.

## Conventions

- English everywhere in code and docs: identifiers, comments, test names, API/CLI messages, and documentation. UI copy comes from i18n resources with `id` and `en` locales; never hardcode strings in components.
- Migrations are forward-only and numbered; never edit an applied migration.
- Core functions are pure and covered by golden tests (rounding, null expiry, shortfall, conversion chains).
- Integration tests hit a real Postgres with RLS enabled. Mocked DB tests are not acceptable for tenancy or stock math.
- Workflow: create a branch, commit, push, open a PR. Do not merge; the user merges.

## Where to look first

| Question | File |
| --- | --- |
| What to build next, acceptance criteria, open decisions | `docs/ROADMAP.md` |
| Why the product exists, schema draft, pricing, stack rationale | `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` |
| Dev environment | `flake.nix`, `.envrc` |
