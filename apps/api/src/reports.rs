//! Business summary: what the stock is worth, what was lost, and what each
//! product earns.
//!
//! Every number comes from the ledger and the snapshots taken when the
//! documents were written. Nothing here is recalculated from today's prices,
//! so a new purchase or a new production run cannot rewrite an earlier month.
//!
//! Margin needs a cost of goods sold. `sale_items.unit_cost` holds the HPP at
//! the moment of the sale; sales written before that column existed have none,
//! and the response marks them instead of valuing them at today's cost.
//!
//! The period is inclusive calendar days. Without a range the tenant's usage
//! window is used, ending today. A range is honoured for every plan, because it
//! only changes how far back a report the tenant may already read reaches; the
//! daily trend, material usage, and purchasing sections belong to Business.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use nata_domain::plan::Plan;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::guard::{CanReadReports, Require};
use crate::{error::ApiError, AppState};

/// Money is stored as `numeric(18,2)`.
const MONEY_SCALE: u32 = 2;
/// Enough rows to show the biggest losses without paging.
const WASTE_LIMIT: i64 = 10;
/// Same cap for the usage and purchasing tables.
const DETAIL_LIMIT: i64 = 50;
/// A range longer than a year is a client bug, and the trend would return a
/// row per day without adding any insight.
const MAX_RANGE_DAYS: i64 = 366;

/// `from` and `to` are both optional, but only ever together: a half-given
/// range has no obvious missing end, so it is refused rather than guessed.
#[derive(Debug, Default, Deserialize)]
pub struct ReportQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// The period the report actually covered, echoed back so the screen can say
/// what it is looking at instead of repeating the request.
#[derive(Debug, Serialize)]
pub struct ReportRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: i32,
}

#[derive(Debug, Serialize)]
pub struct BusinessView {
    /// The tenant's usage window, kept for the reset button and the exports.
    /// It is the period used when the caller did not choose one.
    pub window_days: i32,
    /// The period the response covers. Equal to `window_days` unless the
    /// caller asked for a range.
    pub range: ReportRange,
    /// Whether the plan includes taking this view away as a file. The screen
    /// shows the report to every plan and only offers the export when this is
    /// true, so a refusal is explainable instead of a missing button.
    pub can_export: bool,
    /// Whether the plan includes the daily trend, material usage, and
    /// purchasing sections. They are empty when this is false.
    pub can_advanced: bool,
    pub profit_loss: ProfitLoss,
    pub trend: Vec<TrendDay>,
    pub stock: StockValue,
    pub waste: WasteValue,
    pub usage: Vec<UsageRow>,
    pub purchasing: Vec<PurchaseRow>,
    pub products: Vec<ProductMargin>,
}

/// The period's laba rugi: what came in, what the goods cost, and the difference.
///
/// There is no separate operating-expense line. Overhead is priced into the HPP
/// (`production_orders.overhead_cost` -> `product_batches.unit_cost` ->
/// `sale_items.unit_cost`), so it is already inside `cost_of_goods`; subtracting
/// the period overhead again would charge it twice. Nata records no other
/// operating cost, so the gross profit is also the net one.
#[derive(Debug, Serialize)]
pub struct ProfitLoss {
    /// Revenue from every sale in the period, at the price each line charged.
    pub revenue: Decimal,
    /// Cost of the goods sold, at the cost snapshotted on each sale. Null when
    /// any line in the window has no cost, because an unknown cost is not a
    /// zero cost.
    pub cost_of_goods: Option<Decimal>,
    /// Revenue minus cost of goods, null while the cost is incomplete.
    pub gross_profit: Option<Decimal>,
    /// True when at least one sold line in the window has no cost snapshot.
    pub cost_incomplete: bool,
}

/// One day of the trend. Days with nothing sold are left out, so the chart
/// shows only the days that moved.
#[derive(Debug, Serialize)]
pub struct TrendDay {
    pub date: NaiveDate,
    pub revenue: Decimal,
    /// Null when any line sold that day had no cost snapshot.
    pub cost_of_goods: Option<Decimal>,
    pub gross_profit: Option<Decimal>,
    pub sold_qty: Decimal,
    pub cost_incomplete: bool,
}

#[derive(Debug, Serialize)]
pub struct StockValue {
    /// Material batches at the cost they were received at.
    pub materials: Decimal,
    /// Finished goods at the cost of the batch they came from.
    pub products: Decimal,
    pub total: Decimal,
}

#[derive(Debug, Serialize)]
pub struct WasteValue {
    pub value: Decimal,
    pub movements: i64,
    /// Biggest losses first.
    pub by_material: Vec<WasteRow>,
}

#[derive(Debug, Serialize)]
pub struct WasteRow {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub qty: Decimal,
    pub value: Decimal,
}

/// Material drawn into production during the period, at the cost it carried
/// when it was used.
#[derive(Debug, Serialize)]
pub struct UsageRow {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    /// Positive quantity consumed.
    pub qty: Decimal,
    /// Quantity-weighted average cost, so a material bought at two prices shows
    /// one number a spreadsheet can divide back into `value`.
    pub unit_cost: Decimal,
    pub value: Decimal,
    /// How many production runs drew on this material.
    pub runs: i64,
}

/// Purchases grouped by supplier and material.
#[derive(Debug, Serialize)]
pub struct PurchaseRow {
    pub supplier_id: Option<Uuid>,
    /// Null when the batch records no supplier, which is allowed.
    pub supplier_name: Option<String>,
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub qty: Decimal,
    pub value: Decimal,
    pub batches: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductMargin {
    pub product_id: Uuid,
    pub name: String,
    pub sell_price: Decimal,
    /// Latest production cost per unit, null when never produced.
    pub unit_cost: Option<Decimal>,
    pub margin_per_unit: Option<Decimal>,
    /// Percent of the selling price, null when the price is zero.
    pub margin_pct: Option<Decimal>,
    pub sold_qty: Decimal,
    pub revenue: Decimal,
    /// Null when no unit sold in the window had a cost snapshot.
    pub cost_of_goods: Option<Decimal>,
    pub margin: Option<Decimal>,
    /// True when at least one sold line had no cost, so the totals cannot be
    /// trusted for this product.
    pub cost_incomplete: bool,
}

type ProductRow = (
    Uuid,
    String,
    Decimal,
    Option<Decimal>,
    Decimal,
    Decimal,
    Option<Decimal>,
    i64,
);

type WasteTuple = (Uuid, String, String, Decimal, Decimal);

/// Revenue, cost of goods, and how many sold lines carry no cost snapshot.
type SalesTuple = (Decimal, Decimal, i64);

type TrendTuple = (NaiveDate, Decimal, Decimal, Decimal, i64);

type UsageTuple = (Uuid, String, String, Decimal, Decimal, i64);

type PurchaseTuple = (
    Option<Uuid>,
    Option<String>,
    Uuid,
    String,
    String,
    Decimal,
    Decimal,
    i64,
);

pub async fn business(
    State(state): State<AppState>,
    guard: Require<CanReadReports>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<BusinessView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // The same window the shopping list uses: one setting, one explanation.
    // The plan comes along because export is an entitlement, and reading both
    // from the same row keeps the button and the data from disagreeing.
    let (window_days, plan): (i32, String) =
        sqlx::query_as("select usage_window_days, plan from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    let plan = Plan::parse(&plan)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("unknown plan {plan}")))?;

    let range = resolve_range(&query, window_days)?;
    // Half-open bounds: `from` at midnight, the day after `to` at midnight, so
    // an entire day is inside whatever its timestamps say.
    let start = at_midnight(range.from)?;
    let end = at_midnight(range.to + Duration::days(1))?;

    // Expired stock is left out: it is not sellable, so valuing it at cost
    // would overstate what the business owns.
    let (material_value, product_value): (Decimal, Decimal) = sqlx::query_as(
        "select
             (select coalesce(sum(b.qty_remaining * b.unit_cost) filter (
                      where b.expires_at is null or b.expires_at >= current_date
                  ), 0)::numeric(18,2)
              from material_batches b
              where b.tenant_id = $1 and b.qty_remaining > 0),
             (select coalesce(sum(pb.qty_remaining * pb.unit_cost) filter (
                      where pb.expires_at is null or pb.expires_at >= current_date
                  ), 0)::numeric(18,2)
              from product_batches pb
              where pb.tenant_id = $1 and pb.qty_remaining > 0)",
    )
    .bind(auth.tenant_id)
    .fetch_one(&mut *tx)
    .await?;

    let (waste_value, waste_movements): (Decimal, i64) = sqlx::query_as(
        "select coalesce(sum(-mov.qty * coalesce(mov.unit_cost, 0)), 0)::numeric(18,2),
                count(*)
         from stock_movements mov
         where mov.tenant_id = $1
           and mov.type in ('waste', 'expired')
           and mov.created_at >= $2
           and mov.created_at < $3",
    )
    .bind(auth.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *tx)
    .await?;

    let waste_rows: Vec<WasteTuple> = sqlx::query_as(
        "select b.material_id, mat.name, u.code,
                sum(-mov.qty)::numeric(18,3),
                sum(-mov.qty * coalesce(mov.unit_cost, 0))::numeric(18,2)
         from stock_movements mov
         join material_batches b on b.id = mov.batch_id
         join materials mat on mat.id = b.material_id
         join units u on u.id = mat.base_unit_id
         where mov.tenant_id = $1
           and mov.type in ('waste', 'expired')
           and mov.created_at >= $2
           and mov.created_at < $3
         group by b.material_id, mat.name, u.code
         order by 5 desc, mat.name
         limit $4",
    )
    .bind(auth.tenant_id)
    .bind(start)
    .bind(end)
    .bind(WASTE_LIMIT)
    .fetch_all(&mut *tx)
    .await?;

    // The same window as the product margin below, and the same rule the
    // overview screen uses: revenue minus the cost recorded on each sale. A
    // line written before `sale_items.unit_cost` existed makes the whole
    // period's cost unknown rather than valuing the past at today's prices.
    let (revenue, cost_of_goods, uncosted): SalesTuple = sqlx::query_as(
        "select coalesce(sum(si.qty * si.price), 0)::numeric(18,2),
                coalesce(sum(si.qty * si.unit_cost), 0)::numeric(18,2),
                count(*) filter (where si.unit_cost is null)
         from sale_items si
         join sales s on s.id = si.sale_id
         where si.tenant_id = $1
           and s.sold_at >= $2
           and s.sold_at < $3",
    )
    .bind(auth.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *tx)
    .await?;

    let rows: Vec<ProductRow> = sqlx::query_as(
        "with latest as (
             select distinct on (product_id) product_id, unit_cost
             from production_orders
             where tenant_id = $1 and status = 'done' and unit_cost is not null
             order by product_id, produced_on desc, created_at desc
         ),
         sold as (
             select si.product_id,
                    sum(si.qty)::numeric(18,3) as qty,
                    sum(si.qty * si.price)::numeric(18,2) as revenue,
                    sum(si.qty * si.unit_cost)::numeric(18,2) as cogs,
                    count(*) filter (where si.unit_cost is null) as uncosted
             from sale_items si
             join sales s on s.id = si.sale_id
             where si.tenant_id = $1
               and s.sold_at >= $2
               and s.sold_at < $3
             group by si.product_id
         )
         select p.id, p.name, p.sell_price, latest.unit_cost,
                coalesce(sold.qty, 0), coalesce(sold.revenue, 0),
                sold.cogs, coalesce(sold.uncosted, 0)
         from products p
         left join latest on latest.product_id = p.id
         left join sold on sold.product_id = p.id
         where p.tenant_id = $1 and p.is_active
         order by coalesce(sold.revenue, 0) desc, p.name",
    )
    .bind(auth.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_all(&mut *tx)
    .await?;

    // The deeper sections are an entitlement, so a tenant without them is not
    // charged for the queries either.
    let (trend, usage, purchasing) = if plan.allows_advanced_reports() {
        let trend_rows: Vec<TrendTuple> = sqlx::query_as(
            "select (s.sold_at)::date,
                    coalesce(sum(si.qty * si.price), 0)::numeric(18,2),
                    coalesce(sum(si.qty * si.unit_cost), 0)::numeric(18,2),
                    coalesce(sum(si.qty), 0)::numeric(18,3),
                    count(*) filter (where si.unit_cost is null)
             from sale_items si
             join sales s on s.id = si.sale_id
             where si.tenant_id = $1
               and s.sold_at >= $2
               and s.sold_at < $3
             group by 1
             order by 1",
        )
        .bind(auth.tenant_id)
        .bind(start)
        .bind(end)
        .fetch_all(&mut *tx)
        .await?;

        let usage_rows: Vec<UsageTuple> = sqlx::query_as(
            "select b.material_id, m.name, u.code,
                    sum(pc.qty)::numeric(18,3),
                    sum(pc.qty * pc.unit_cost)::numeric(18,2),
                    count(distinct pc.production_order_id)
             from production_consumptions pc
             join production_orders o on o.id = pc.production_order_id
             join material_batches b on b.id = pc.batch_id
             join materials m on m.id = b.material_id
             join units u on u.id = m.base_unit_id
             where o.tenant_id = $1
               and o.produced_on >= $2
               and o.produced_on <= $3
             group by 1, 2, 3
             order by 5 desc, m.name
             limit $4",
        )
        .bind(auth.tenant_id)
        .bind(range.from)
        .bind(range.to)
        .bind(DETAIL_LIMIT)
        .fetch_all(&mut *tx)
        .await?;

        let purchase_rows: Vec<PurchaseTuple> = sqlx::query_as(
            "select b.supplier_id, s.name, b.material_id, m.name, u.code,
                    sum(mov.qty)::numeric(18,3),
                    sum(mov.qty * mov.unit_cost)::numeric(18,2),
                    count(*)
             from stock_movements mov
             join material_batches b on b.id = mov.batch_id
             join materials m on m.id = b.material_id
             join units u on u.id = m.base_unit_id
             left join suppliers s on s.id = b.supplier_id
             where mov.tenant_id = $1
               and mov.type = 'purchase'
               and mov.created_at >= $2
               and mov.created_at < $3
             group by 1, 2, 3, 4, 5
             order by 7 desc, m.name
             limit $4",
        )
        .bind(auth.tenant_id)
        .bind(start)
        .bind(end)
        .bind(DETAIL_LIMIT)
        .fetch_all(&mut *tx)
        .await?;

        (trend_rows, usage_rows, purchase_rows)
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };

    tx.commit().await?;

    let trend = trend
        .into_iter()
        .map(|(date, revenue, cogs, sold_qty, uncosted)| {
            let cost_incomplete = uncosted > 0;
            let cost_of_goods = if cost_incomplete {
                None
            } else {
                Some(money(cogs))
            };

            TrendDay {
                date,
                revenue: money(revenue),
                gross_profit: cost_of_goods.map(|cost| money(revenue - cost)),
                cost_of_goods,
                sold_qty: qty(sold_qty),
                cost_incomplete,
            }
        })
        .collect();

    let usage = usage
        .into_iter()
        .map(
            |(material_id, material_name, unit_code, used, value, runs)| UsageRow {
                material_id,
                material_name,
                unit_code,
                qty: qty(used),
                // `qty` is positive by the check on `production_consumptions`.
                unit_cost: unit_cost(value, used),
                value: money(value),
                runs,
            },
        )
        .collect();

    let purchasing = purchasing
        .into_iter()
        .map(
            |(
                supplier_id,
                supplier_name,
                material_id,
                material_name,
                unit_code,
                bought,
                value,
                batches,
            )| PurchaseRow {
                supplier_id,
                supplier_name,
                material_id,
                material_name,
                unit_code,
                qty: qty(bought),
                value: money(value),
                batches,
            },
        )
        .collect();

    let products = rows
        .into_iter()
        .map(
            |(product_id, name, sell_price, unit_cost, sold_qty, revenue, cogs, uncosted)| {
                let margin_per_unit = unit_cost.map(|cost| money(sell_price - cost));

                let margin_pct = match (margin_per_unit, sell_price) {
                    (Some(margin), price) if price > Decimal::ZERO => {
                        Some(money(margin / price * Decimal::from(100)))
                    }
                    _ => None,
                };

                let cost_incomplete = uncosted > 0;
                let cost_of_goods = if cost_incomplete { None } else { cogs };

                ProductMargin {
                    product_id,
                    name,
                    sell_price,
                    unit_cost,
                    margin_per_unit,
                    margin_pct,
                    sold_qty,
                    revenue: money(revenue),
                    cost_of_goods: cost_of_goods.map(money),
                    margin: cost_of_goods.map(|cost| money(revenue - cost)),
                    cost_incomplete,
                }
            },
        )
        .collect();

    let cost_incomplete = uncosted > 0;
    let cost_of_goods = if cost_incomplete {
        None
    } else {
        Some(money(cost_of_goods))
    };
    let gross_profit = cost_of_goods.map(|cost| money(revenue - cost));

    Ok(Json(BusinessView {
        window_days,
        range,
        can_export: plan.allows_export(),
        can_advanced: plan.allows_advanced_reports(),
        profit_loss: ProfitLoss {
            revenue: money(revenue),
            cost_of_goods,
            gross_profit,
            cost_incomplete,
        },
        trend,
        stock: StockValue {
            materials: money(material_value),
            products: money(product_value),
            total: money(material_value + product_value),
        },
        waste: WasteValue {
            value: money(waste_value),
            movements: waste_movements,
            by_material: waste_rows
                .into_iter()
                .map(
                    |(material_id, material_name, unit_code, qty, value)| WasteRow {
                        material_id,
                        material_name,
                        unit_code,
                        qty,
                        value: money(value),
                    },
                )
                .collect(),
        },
        usage,
        purchasing,
        products,
    }))
}

/// The period to report on. A missing range falls back to the tenant's usage
/// window, ending today; a half-given one is refused rather than completed.
fn resolve_range(query: &ReportQuery, window_days: i32) -> Result<ReportRange, ApiError> {
    let today = Utc::now().date_naive();

    let (from, to) = match (&query.from, &query.to) {
        (None, None) => {
            let days = window_days.clamp(1, MAX_RANGE_DAYS as i32);
            (today - Duration::days(i64::from(days - 1)), today)
        }
        (Some(from), Some(to)) => (parse_date(from)?, parse_date(to)?),
        _ => return Err(ApiError::BadRequest("invalid_range")),
    };

    if from > to {
        return Err(ApiError::BadRequest("invalid_range"));
    }

    let days = (to - from).num_days() + 1;

    if days > MAX_RANGE_DAYS {
        return Err(ApiError::BadRequest("range_too_long"));
    }

    Ok(ReportRange {
        from,
        to,
        days: days as i32,
    })
}

/// Dates arrive as `YYYY-MM-DD`; anything else is a bad request rather than a
/// parse we have to apologise for.
fn parse_date(value: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| ApiError::BadRequest("invalid_range"))
}

/// Midnight at the start of a date, as an instant the database can compare
/// against a `timestamptz`.
fn at_midnight(date: NaiveDate) -> Result<DateTime<Utc>, ApiError> {
    date.and_hms_opt(0, 0, 0)
        .map(|moment| moment.and_utc())
        .ok_or(ApiError::Internal(anyhow::anyhow!("date out of range")))
}

/// Two decimals and a fixed scale. A computed `coalesce(…, 0)` can arrive with
/// scale zero, which would serialize as `"0"` instead of `"0.00"`; rescaling
/// keeps every money field the same shape for the client.
fn money(value: Decimal) -> Decimal {
    let mut value =
        value.round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(MONEY_SCALE);
    value
}

/// Quantities keep three decimals, matching the ledger columns they came from.
fn qty(value: Decimal) -> Decimal {
    let mut value = value.round_dp_with_strategy(3, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(3);
    value
}

/// Weighted average cost, four decimals like the `unit_cost` columns. The
/// division is exact enough at this scale and the caller always has a non-zero
/// quantity.
fn unit_cost(value: Decimal, qty: Decimal) -> Decimal {
    let mut cost = value / qty;
    cost.rescale(4);
    cost
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/reports", get(business))
}
