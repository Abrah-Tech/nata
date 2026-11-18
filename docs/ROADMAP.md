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
| 03 · Batch & FEFO | `product_batches`, FEFO sale allocation, `/stock/product-batches`, `/stock/product-corrections` | Batch list with expiry and remaining quantity, correction form (waste / expired / adjustment) |
| 09 · Alert WhatsApp | `alert_rules`, `/alert-rules`, `/alerts`, `alert_job`, `alert_delivery` | Digest preview, open versus resolved history, rule editor (H-7/H-3, low stock, channel) |
| 10 · Tim & Langganan | `memberships`, invitations, plan limits, billing | Member list with roles, invite form, plan and subscription status |
| 12 · Pengaturan (suggested) | `audit_logs`, `/audit-logs`, `nata_db::audit::record` | Who changed a role, revoked an invitation, deleted or verified a payment, or moved the labor rate — newest first, owner-only |

Do not build these screens before their frame exists in the design file.

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

**HPP scope note.** P2-05 snapshotted material cost only; P2-08 adds labor from the tenant rate, so a new order prices `recipe.labor_minutes` at `tenants.labor_rate_per_hour`. Overhead stays out until `cost_overheads` gets an input flow; when it lands, a monthly amount allocated per unit is the recommended shape.

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

**Phase 3 gate:** first paying customer.

**Status (2026-11-15):** P3-01 through P3-06 are built. Report export is a Pro and Business entitlement: the report stays readable on every plan, the file is built in the browser from the payload the screen already shows, so the CSV cannot drift from the view, and the API reports `can_export` so the button can explain itself instead of disappearing. Billing is manual, per D4: the owner records a transfer, the operator checks it and runs `cargo run -p nata-api --bin billing -- verify <payment-id>`, and access runs to the paid period plus a 7-day grace before the tenant goes read-only. Reads and the billing routes stay open in read-only mode so a tenant can always pay its way back.

---

## Phase 4 — Expansion

| ID | Task | Notes |
| --- | --- | --- |
| P4-01 | Multi-warehouse | Requires the `warehouse_id` migration; see D6 |
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
3. **No `warehouse_id`** even though Pro sells 2 warehouses. Add a nullable column to batches and movements now, or accept a large migration in phase 4.
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
| D6 | Multi-warehouse: prepare now or later? | Nullable column now, UI in phase 4 |
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
