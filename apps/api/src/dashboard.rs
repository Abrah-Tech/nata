//! Overview payload: one request with everything the first screen needs.
//!
//! The dashboard is the page an owner opens on a phone over mobile data, so it
//! is deliberately a single bounded response instead of a fan-out over the
//! stock endpoints. Counts and lists are derived from the batch ledger; nothing
//! here is stored.

use axum::{extract::State, Json};
use chrono::NaiveDate;
use nata_domain::Permission;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, Require};
use crate::master::units::UnitRef;
use crate::{error::ApiError, AppState};

/// Batches expiring inside this window are worth acting on now.
const EXPIRY_HORIZON_DAYS: i32 = 7;
/// Enough rows to fill the fold on a phone without making the payload heavy.
const LIST_LIMIT: i64 = 8;
/// The ranking is a glance at what moved, not a second report page.
const BEST_SELLER_LIMIT: i64 = 5;
/// Money is stored as `numeric(18,2)`.
const MONEY_SCALE: u32 = 2;

#[derive(Debug, Serialize)]
pub struct DashboardView {
    pub summary: DashboardSummary,
    /// Materials under their declared minimum, worst shortfall first.
    pub critical: Vec<CriticalRow>,
    /// Active batches at or past their expiry window, earliest first.
    pub expiring: Vec<ExpiringRow>,
    /// Today's sales numbers. Absent when the caller's role cannot read
    /// reports: the landing screen must not fail for a role that only needs
    /// stock, and profit is the one figure a cashier should never see.
    pub sales: Option<DashboardSales>,
}

#[derive(Debug, Serialize)]
pub struct DashboardSales {
    pub revenue: Decimal,
    /// Null when any line sold today has no cost snapshot, the same rule the
    /// report uses: an unknown cost is not a zero cost.
    pub cost_of_goods: Option<Decimal>,
    pub profit: Option<Decimal>,
    /// True when the totals above cannot be trusted because a line has no cost.
    pub cost_incomplete: bool,
    /// Yesterday over the same stretch of the day. A half-finished today is
    /// compared with a half-finished yesterday, never with a complete one.
    pub yesterday_revenue: Decimal,
    pub yesterday_profit: Option<Decimal>,
    /// Units sold today, most first.
    pub best_sellers: Vec<BestSeller>,
}

#[derive(Debug, Serialize)]
pub struct BestSeller {
    pub product_id: Uuid,
    pub name: String,
    pub qty: Decimal,
    pub revenue: Decimal,
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub materials: i64,
    pub below_min: i64,
    /// Zero on hand with a minimum above zero, a subset of `below_min`.
    pub out_of_stock: i64,
    /// Batches expiring within `horizon_days` but not yet past it.
    pub expiring: i64,
    /// Batches already past `expires_at` with stock left to use or waste.
    pub expired: i64,
    pub active_batches: i64,
    pub stock_value: Decimal,
    /// Echoed so the UI does not hardcode the window in its copy.
    pub horizon_days: i32,
}

#[derive(Debug, Serialize)]
pub struct CriticalRow {
    pub material_id: Uuid,
    pub name: String,
    pub base_unit: UnitRef,
    pub on_hand: Decimal,
    pub min_stock: Decimal,
    pub shortfall: Decimal,
    pub active_batches: i64,
    pub nearest_expiry: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct ExpiringRow {
    pub batch_id: Uuid,
    pub material_id: Uuid,
    pub material_name: String,
    pub base_unit: UnitRef,
    pub qty_remaining: Decimal,
    pub unit_cost: Decimal,
    pub expires_at: NaiveDate,
    /// Days from today, negative once the batch is past its expiry date.
    pub days_left: i32,
}

type SummaryRow = (i64, i64, i64, Decimal, i64, i64, i64);
type CriticalTuple = (
    Uuid,
    String,
    Uuid,
    String,
    Decimal,
    Decimal,
    Decimal,
    i64,
    Option<NaiveDate>,
);
type ExpiringTuple = (
    Uuid,
    Uuid,
    String,
    Uuid,
    String,
    Decimal,
    Decimal,
    NaiveDate,
    i32,
);
/// Revenue, cost of goods, and how many lines had no cost snapshot.
type SalesTuple = (Decimal, Option<Decimal>, i64);
type BestSellerTuple = (Uuid, String, Decimal, Decimal);

/// Two decimals and a fixed scale. A computed `coalesce(…, 0)` can arrive with
/// scale zero, which would serialize as `"0"` instead of `"0.00"`; rescaling
/// keeps every money field the same shape for the client.
fn money(value: Decimal) -> Decimal {
    let mut value =
        value.round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(MONEY_SCALE);
    value
}

/// Revenue, cost of goods, and the count of lines with no cost snapshot, over
/// one day measured from that day's midnight to this time of day.
///
/// `day_offset` is days back from today: `0` is today so far, `1` is yesterday
/// so far. Comparing the two on the same stretch keeps a half-finished day from
/// reading as a collapse next to a complete one.
async fn day_totals(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    day_offset: i32,
) -> Result<SalesTuple, ApiError> {
    sqlx::query_as(
        "select coalesce(sum(si.qty * si.price), 0)::numeric(18,2) as revenue,
                sum(si.qty * si.unit_cost)::numeric(18,2) as cost_of_goods,
                count(*) filter (where si.unit_cost is null) as uncosted
         from sale_items si
         join sales s on s.id = si.sale_id
         where si.tenant_id = $1
           and s.sold_at >= date_trunc('day', now()) - ($2::int * interval '1 day')
           and s.sold_at < now() - ($2::int * interval '1 day')",
    )
    .bind(tenant_id)
    .bind(day_offset)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

/// The products that moved the most units today, biggest first.
async fn best_sellers(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<Vec<BestSellerTuple>, ApiError> {
    sqlx::query_as(
        "select p.id, p.name,
                sum(si.qty)::numeric(18,3) as qty,
                sum(si.qty * si.price)::numeric(18,2) as revenue
         from sale_items si
         join sales s on s.id = si.sale_id
         join products p on p.id = si.product_id
         where si.tenant_id = $1
           and s.sold_at >= date_trunc('day', now())
           and s.sold_at < now()
         group by p.id, p.name
         order by qty desc, revenue desc, p.name
         limit $2",
    )
    .bind(tenant_id)
    .bind(BEST_SELLER_LIMIT)
    .fetch_all(&mut **tx)
    .await
    .map_err(Into::into)
}

/// Everything the overview screen shows, in one round trip.
pub async fn overview(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<DashboardView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (materials, below_min, out_of_stock, stock_value, active_batches, expiring, expired): SummaryRow =
        sqlx::query_as(
             "with levels as (
                 select m.id, m.min_stock,
                        -- Only stock inside its shelf life counts as on hand:
                        -- an expired batch cannot be used or sold.
                        coalesce(sum(b.qty_remaining) filter (
                            where b.expires_at is null or b.expires_at >= current_date
                        ), 0)::numeric(18,3) as on_hand
                 from materials m
                 left join material_batches b
                        on b.tenant_id = m.tenant_id
                       and b.material_id = m.id
                       and b.qty_remaining > 0
                 where m.tenant_id = $1 and m.is_active
                 group by m.id
             ),
             batches as (
                 select count(*) filter (where b.qty_remaining > 0) as active_batches,
                        count(*) filter (
                            where b.qty_remaining > 0
                              and b.expires_at >= current_date
                              and b.expires_at <= current_date + $2::int
                        ) as expiring,
                        count(*) filter (
                            where b.qty_remaining > 0
                              and b.expires_at < current_date
                        ) as expired
                 from material_batches b
                 where b.tenant_id = $1
             )
             select (select count(*) from levels),
                    (select count(*) from levels where on_hand < min_stock),
                    (select count(*) from levels where on_hand = 0 and min_stock > 0),
                    -- Valued at the cost each batch was received at, not the
                    -- material's latest price: a new purchase must not revalue
                    -- stock that was already on the shelf.
                    (select coalesce(sum(b.qty_remaining * b.unit_cost) filter (
                             where b.expires_at is null or b.expires_at >= current_date
                         ), 0)::numeric(18,2)
                     from material_batches b
                     where b.tenant_id = $1 and b.qty_remaining > 0),
                    batches.active_batches,
                    batches.expiring,
                    batches.expired
             from batches",
        )
        .bind(auth.tenant_id)
        .bind(EXPIRY_HORIZON_DAYS)
        .fetch_one(&mut *tx)
        .await?;

    let critical_rows: Vec<CriticalTuple> = sqlx::query_as(
        "select m.id, m.name, u.id, u.code,
                coalesce(sum(b.qty_remaining) filter (
                    where b.expires_at is null or b.expires_at >= current_date
                ), 0)::numeric(18,3) as on_hand,
                m.min_stock,
                (m.min_stock - coalesce(sum(b.qty_remaining) filter (
                    where b.expires_at is null or b.expires_at >= current_date
                ), 0))::numeric(18,3) as shortfall,
                count(b.id) as active_batches,
                min(b.expires_at) as nearest_expiry
         from materials m
         join units u on u.id = m.base_unit_id
         left join material_batches b
                on b.tenant_id = m.tenant_id
               and b.material_id = m.id
               and b.qty_remaining > 0
         where m.tenant_id = $1 and m.is_active
         group by m.id, u.id
         having coalesce(sum(b.qty_remaining) filter (
             where b.expires_at is null or b.expires_at >= current_date
         ), 0) < m.min_stock
         order by (m.min_stock - coalesce(sum(b.qty_remaining) filter (
                 where b.expires_at is null or b.expires_at >= current_date
             ), 0))
                      / nullif(m.min_stock, 0) desc,
                  m.name asc
         limit $2",
    )
    .bind(auth.tenant_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await?;

    let expiring_rows: Vec<ExpiringTuple> = sqlx::query_as(
        "select b.id, b.material_id, m.name, u.id, u.code,
                b.qty_remaining, b.unit_cost, b.expires_at,
                (b.expires_at - current_date) as days_left
         from material_batches b
         join materials m on m.id = b.material_id
         join units u on u.id = m.base_unit_id
         where b.tenant_id = $1
           and b.qty_remaining > 0
           and b.expires_at <= current_date + $2::int
         order by b.expires_at asc, b.received_at asc, b.id asc
         limit $3",
    )
    .bind(auth.tenant_id)
    .bind(EXPIRY_HORIZON_DAYS)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await?;

    // Profit is the one figure a cashier should never see, so the sales half is
    // left out for a role without report access rather than refusing the landing
    // screen. Every role today holds that permission; the check is there for the
    // one that comes next.
    let sales = if auth.role.can(Permission::ReportRead) {
        let (revenue, cogs, uncosted) = day_totals(&mut tx, auth.tenant_id, 0).await?;
        let (yesterday_revenue, yesterday_cogs, yesterday_uncosted) =
            day_totals(&mut tx, auth.tenant_id, 1).await?;
        let best_sellers = best_sellers(&mut tx, auth.tenant_id).await?;

        let cost_of_goods = if uncosted > 0 { None } else { cogs.map(money) };
        let yesterday_profit = match (yesterday_uncosted, yesterday_cogs) {
            (0, Some(cost)) => Some(money(yesterday_revenue - cost)),
            _ => None,
        };

        Some(DashboardSales {
            revenue: money(revenue),
            profit: cost_of_goods.map(|cost| money(revenue - cost)),
            cost_of_goods,
            cost_incomplete: uncosted > 0,
            yesterday_revenue: money(yesterday_revenue),
            yesterday_profit,
            best_sellers: best_sellers
                .into_iter()
                .map(|(product_id, name, qty, revenue)| BestSeller {
                    product_id,
                    name,
                    qty,
                    revenue: money(revenue),
                })
                .collect(),
        })
    } else {
        None
    };

    tx.commit().await?;

    Ok(Json(DashboardView {
        summary: DashboardSummary {
            materials,
            below_min,
            out_of_stock,
            expiring,
            expired,
            active_batches,
            stock_value,
            horizon_days: EXPIRY_HORIZON_DAYS,
        },
        sales,
        critical: critical_rows
            .into_iter()
            .map(
                |(
                    material_id,
                    name,
                    unit_id,
                    unit_code,
                    on_hand,
                    min_stock,
                    shortfall,
                    active_batches,
                    nearest_expiry,
                )| CriticalRow {
                    material_id,
                    name,
                    base_unit: UnitRef {
                        id: unit_id,
                        code: unit_code,
                    },
                    on_hand,
                    min_stock,
                    shortfall,
                    active_batches,
                    nearest_expiry,
                },
            )
            .collect(),
        expiring: expiring_rows
            .into_iter()
            .map(
                |(
                    batch_id,
                    material_id,
                    material_name,
                    unit_id,
                    unit_code,
                    qty_remaining,
                    unit_cost,
                    expires_at,
                    days_left,
                )| ExpiringRow {
                    batch_id,
                    material_id,
                    material_name,
                    base_unit: UnitRef {
                        id: unit_id,
                        code: unit_code,
                    },
                    qty_remaining,
                    unit_cost,
                    expires_at,
                    days_left,
                },
            )
            .collect(),
    }))
}
