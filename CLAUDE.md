# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Nata is a PPIC + light POS SaaS for F&B UMKM: batch and expiry tracking, FEFO
allocation, versioned recipes/BOM, production costing (HPP), purchasing
recommendations, sales, plans/billing, and WhatsApp/email alerts. Three runtimes
share one calculation core: a Next.js static-export web app, a Tauri 2 shell
(Android first), and an Axum API. Postgres (Neon in production) is the source of
truth; the client never invents numbers.

The documents that define the work:

- `docs/ROADMAP.md` — phase order, task IDs, gates, open decisions.
- `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` — product spec, schema
  draft, pricing, stack rationale (local-only, gitignored).
- `CODEBUDDY.md` — the longer architecture notes this file is distilled from.

## Commands

Everything runs inside the Nix dev shell. `.envrc` already uses the flake.

```bash
nix develop          # enter dev shell
direnv allow         # optional
```

Use Bun for everything JavaScript/TypeScript — no npm, pnpm, or yarn.

### Rust

```bash
cargo build --workspace
cargo test --workspace
cargo test -p nata-core fefo              # single module
cargo test -p nata-core -- --nocapture
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo fmt --all -- --check
```

### Web and shell

```bash
bun install
bun run --filter web dev
bun run --filter web build                 # must stay a static export
bun run --filter web lint
bun run --filter web typecheck
bun test apps/web/tests                     # browser-side units, via Bun's runner
bun run --filter shell tauri android dev
```

### Database

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

`#[sqlx::test]` creates a throwaway database per test, so the URL must point at
a server where the user can create databases.

### Jobs and billing

```bash
cargo run -p nata-api --bin alert_job     # fills, resolves, and sends alerts; safe to re-run
cargo run -p nata-api --bin billing -- list
cargo run -p nata-api --bin billing -- verify <payment-id> --by <label>
```

Both need a role that can see every tenant (the migration role or `bypassrls`);
per-tenant work still runs in a tenant-scoped transaction.

### Local sign-in

The API loads `.env` at startup. With `DEV_OTP_CODE` set (default `000000`),
debug builds use that fixed code for any contact instead of a random one, so
authenticated routes are reachable without reading the log. Release builds
always randomize. OTP delivery is chosen from the environment, never from code:
`EMAIL_PROVIDER=brevo` (+ `BREVO_API_KEY`, `EMAIL_FROM`, `EMAIL_FROM_NAME`) or
`WHATSAPP_PROVIDER=fonnte` (+ `FONNTE_TOKEN`). With none set, debug builds log
the message and release builds answer `otp_delivery_failed`.

## Architecture

- `apps/web` — Next.js static export; the UI for both browser and Tauri WebView.
- `apps/shell` — Tauri 2 wrapper. Android first, iOS after Android is stable.
- `apps/api` — Axum. The only place with server logic (auth, jobs, HTTP).
- `crates/core` — pure functions: FEFO allocation, HPP, unit conversion,
  reorder planning. No IO, no sqlx, no async. Used by `api` and `shell` so
  client and server produce identical numbers.
- `crates/domain` — domain types and the API contract (utoipa/specta). Generates
  `packages/api-client`.
- `crates/db` — sqlx queries and data access.
- `migrations/` — Postgres DDL, including RLS policies.

Data flow: UI → generated TypeScript client → Axum → `nata-db`/`nata-core` →
Postgres. Core may also run inside Tauri for local previews, but the server stays
the source of truth.

### Web UI

Tailwind v4 runs through PostCSS and is loaded by `app/globals.css`, the only
style entry: the `nata.pen` tokens, their `@theme` mapping, the layer order
`theme, base, components, utilities`, and hand-written global resets in `base`.
Preflight is deliberately not loaded. Style with the mapped tokens (`bg-card`,
`text-ink-3`, `border-border`), never a raw palette class.

Components in `components/ui` are copied shadcn source, not a dependency. Do not
add `@radix-ui/themes`. `components/modal.tsx` is the thin wrapper every form
dialog uses. Loading uses skeletons (`components/skeletons.tsx`), never a
spinner or a "Memuat…" line. `AppShell` takes `me` as optional and renders the
chrome from the first paint with a skeleton inside.

### Domain rules that are easy to get wrong

- Stock is a batch ledger, not a scalar. `material_batches.qty_remaining` is
  on-hand per batch; `stock_movements` is the audit trail. Never add a single
  `stock` column to `materials`.
- Recipes are versioned, never edited in place. A partial unique index enforces
  one active version per product; production stores the `recipe_id` it consumed.
- Production is the only writer of `production_use` movements. It plans FEFO in
  one transaction and rejects the whole run with `409 insufficient_stock` plus
  the shortage list before writing anything.
- Costs are snapshotted (`production_consumptions.unit_cost`,
  `production_orders.unit_cost`, `sale_items.unit_cost`). Never recompute history
  from current prices.
- A sale locks the products it sells and rejects the whole document with
  `409 insufficient_stock` plus every shortage. `client_ref` is the idempotency
  key; both typed and imported sales call `sales::write_sale`.
- Plans are entitlements, not permissions. `Plan::limits` is checked in the
  service layer before a create, never in RLS. Billing access is derived from
  `trial_ends_at`/`paid_until` and enforced once in `AuthContext`.
- Tenancy: every business row carries `tenant_id`; Postgres RLS is the
  enforcement layer. The JWT carries `user_id` + active `tenant_id`; each
  transaction sets `app.tenant_id`/`app.user_id` via `SET LOCAL`. Tests must
  `SET LOCAL ROLE` to a non-owner role.
- Offline sync covers stock-in only. `client_ref` makes replay idempotent;
  rejected entries stay in the outbox for the user to retry or dismiss.

Use `rust_decimal` and Postgres `NUMERIC` end to end. Never `f64` for money or
quantities.

## Deployment

NixOS VPS, no Docker. `apps/web` is served as a static export by nginx;
`apps/api` runs as a systemd service on `127.0.0.1:3001` under the `/api`
prefix. Database is Neon. The deploy workflow builds the release binary and the
static export, rsyncs a release directory, flips the `/srv/nata/current`
symlink atomically, runs `nata-migrate`, restarts `nata-api`, and health-checks
`/healthz` with automatic rollback.

The host units, nginx vhost, ACME certificate, and `/etc/nata.env` live in the
separate `arifinoid/nixos` repo (`nix/modules/nixos/vps-nata.nix`) — renaming
the units/roles/paths here requires the matching change there. See
`deploy/vps/README.md` for the full procedure, role split (`nata_app` vs the
Neon owner), backup, and rollback.

## Gotchas

- Next static export: no SSR, route handlers, server actions, middleware, or ISR.
  Set `images.unoptimized = true`. Any server behavior belongs in Axum.
- Tauri mobile runs an old WebView on cheap Android phones. Test on low-end
  devices from the first build.
- Server push (FCM/APNs) is out of MVP scope; alerts go through WhatsApp.
- sqlx compile-time checked queries need `DATABASE_URL` at build time or a
  committed `.sqlx` cache.
- The draft schema has no `warehouse_id` although Pro promises two warehouses.
  Follow the roadmap decision; do not add ad-hoc columns in phase 1.
- Dates: `expires_at` is a `date`, not a timestamp.

## Conventions

- English everywhere in code and docs: identifiers, comments, test names,
  API/CLI messages, and documentation. UI copy comes from i18n resources with
  `id` and `en` locales; never hardcode strings in components.
- Migrations are forward-only and numbered; never edit an applied migration.
- Core functions are pure and covered by golden tests.
- Integration tests hit a real Postgres with RLS enabled; mocked DB tests are not
  acceptable for tenancy or stock math.
- Workflow: create a branch, commit, push, open a PR. Do not merge; the user
  merges.

## Where to look first

| Question | File |
| --- | --- |
| What to build next, acceptance criteria, open decisions | `docs/ROADMAP.md` |
| Why the product exists, schema draft, pricing, stack rationale | `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` |
| Dev environment | `flake.nix`, `.envrc` |
| Deploy procedure, VPS host config | `deploy/vps/README.md` |
