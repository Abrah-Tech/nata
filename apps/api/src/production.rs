//! Production orders.
//!
//! One order consumes material batches in FEFO order and adds finished goods.
//! Everything runs in a single transaction: if any material is short, the
//! request is rejected before the first write, so stock never moves halfway
//! through a run. The order points at the recipe version it used, and each
//! consumption snapshots the batch cost at the moment of use.
//!
//! The preview endpoint runs the same plan without writing, so the UI can show
//! requirements, the exact batches FEFO would take, and the expected HPP before
//! the user confirms.

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use nata_core::{Allocation, Consumption, ProductionCost, StockBatch};

use crate::auth::guard::{CanReadProduction, CanWriteProduction, Require};
use crate::{error::ApiError, AppState};

/// Batches store three decimals. Requirements round up to the same scale so
/// the batch, the movement, and the consumption always agree.
const LEDGER_SCALE: u32 = 3;

#[derive(Debug, Deserialize)]
pub struct ProductionBody {
    pub product_id: Uuid,
    /// How much finished product the run is expected to make, in product units.
    pub qty: Decimal,
    pub produced_on: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct ProductionOrderView {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub recipe_id: Uuid,
    pub recipe_version: i32,
    pub planned_qty: Decimal,
    pub actual_qty: Decimal,
    pub material_cost: Decimal,
    pub labor_cost: Decimal,
    pub overhead_cost: Decimal,
    /// HPP per finished unit (D7: four decimals).
    pub unit_cost: Decimal,
    pub produced_on: NaiveDate,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub consumptions: Vec<ConsumptionView>,
}

#[derive(Debug, Serialize)]
pub struct ConsumptionView {
    pub batch_id: Uuid,
    pub material_id: Uuid,
    pub material_name: String,
    pub qty: Decimal,
    pub unit_cost: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ProductionPreviewView {
    pub product_id: Uuid,
    pub product_name: String,
    pub sell_price: Decimal,
    /// Cache of every non-empty batch; `sellable_qty` is what FEFO can ship.
    pub stock_qty: Decimal,
    /// Stock that can still be sold, computed from the ledger. Expiry is a
    /// function of the calendar, so the cache cannot be kept up to date.
    pub sellable_qty: Decimal,
    pub recipe_id: Uuid,
    pub recipe_version: i32,
    pub yield_qty: Decimal,
    /// Minutes of labor for one batch, and the tenant rate that prices it.
    pub labor_minutes: i32,
    pub labor_rate_per_hour: Decimal,
    pub qty: Decimal,
    /// False when any material is short; the UI must not allow the run.
    pub can_produce: bool,
    /// Cost fields are null when the run cannot be completed, because a
    /// partial allocation would understate the HPP.
    pub material_cost: Option<Decimal>,
    pub labor_cost: Option<Decimal>,
    pub overhead_cost: Option<Decimal>,
    pub unit_cost: Option<Decimal>,
    pub margin: Option<Decimal>,
    pub margin_pct: Option<Decimal>,
    pub materials: Vec<PreviewMaterialView>,
}

#[derive(Debug, Serialize)]
pub struct PreviewMaterialView {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub required: Decimal,
    /// On hand across the active batches, not what this run would take. The two
    /// differ whenever stock exceeds the requirement, which is the common case.
    pub available: Decimal,
    pub shortfall: Decimal,
    /// The batches FEFO would consume, with the quantity from each.
    pub batches: Vec<PreviewBatchView>,
}

#[derive(Debug, Serialize)]
pub struct PreviewBatchView {
    pub batch_id: Uuid,
    pub expires_at: Option<NaiveDate>,
    pub qty: Decimal,
    pub unit_cost: Decimal,
}

/// One material the run cannot cover, with the gap the user has to close.
#[derive(Debug, Serialize)]
pub struct ShortageView {
    pub material_id: Uuid,
    pub material_name: String,
    pub required: Decimal,
    pub available: Decimal,
    pub shortfall: Decimal,
}

type ProductRow = (
    Uuid,
    String,
    Decimal,
    Decimal,
    Decimal,
    Option<Uuid>,
    Option<i32>,
    Option<Decimal>,
    Option<i32>,
    Option<i32>,
);
type ItemRow = (Uuid, String, String, Decimal, Decimal);
type BatchRow = (Uuid, NaiveDate, Option<NaiveDate>, Decimal, Decimal);

/// A material requirement with the FEFO decision already made.
struct MaterialPlan {
    material_id: Uuid,
    material_name: String,
    unit_code: String,
    required: Decimal,
    /// Total `qty_remaining` over the usable batches. Reported to the client as
    /// on-hand stock, so it is independent of `required`.
    available: Decimal,
    allocation: Allocation<Uuid>,
    costs: HashMap<Uuid, Decimal>,
    lines: Vec<PlannedLine>,
}

struct PlannedLine {
    batch_id: Uuid,
    expires_at: Option<NaiveDate>,
    qty: Decimal,
    unit_cost: Decimal,
}

/// Everything a run would need, without touching stock.
struct PlannedOrder {
    product_name: String,
    sell_price: Decimal,
    stock_qty: Decimal,
    sellable_qty: Decimal,
    recipe_id: Uuid,
    recipe_version: i32,
    yield_qty: Decimal,
    labor_minutes: i32,
    labor_rate_per_hour: Decimal,
    /// Days from production until a batch expires; null means it does not.
    shelf_life_days: Option<i32>,
    materials: Vec<MaterialPlan>,
    shortages: Vec<ShortageView>,
}

/// Pads a decimal to a fixed number of decimals for a stable JSON contract.
fn at_scale(value: Decimal, scale: u32) -> Decimal {
    let mut value = value;
    value.rescale(scale);
    value
}

/// Three decimals, never zero or negative.
fn normalized_qty(value: Decimal) -> Result<Decimal, ApiError> {
    let mut qty = value;
    qty.rescale(LEDGER_SCALE);

    if qty <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_quantity"));
    }

    Ok(qty)
}

/// Reads the current recipe and allocates its materials. `lock` is true for a
/// real run and false for a preview, which must not hold rows.
async fn plan_order(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    product_id: Uuid,
    production_qty: Decimal,
    lock: bool,
) -> Result<PlannedOrder, ApiError> {
    // `sellable_qty` is a correlated subquery over the batch ledger: expiry is
    // time-based, so the `stock_qty` cache cannot be maintained incrementally
    // and would go stale overnight.
    let sellable = "(select coalesce(sum(b.qty_remaining), 0)::numeric(18,3)
         from product_batches b
         where b.tenant_id = p.tenant_id
           and b.product_id = p.id
           and b.qty_remaining > 0
           and (b.expires_at is null or b.expires_at >= current_date)) as sellable_qty";
    let product_sql = format!(
        "select p.id, p.name, p.sell_price, p.stock_qty, {sellable},
                p.current_recipe_id, r.version, r.yield_qty, r.labor_minutes,
                p.shelf_life_days
         from products p
         left join recipes r on r.id = p.current_recipe_id
         where p.id = $1 and p.tenant_id = $2 and p.is_active{}",
        if lock { " for update of p" } else { "" }
    );

    let product: Option<ProductRow> = sqlx::query_as(&product_sql)
        .bind(product_id)
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await?;

    let (
        _,
        product_name,
        sell_price,
        stock_qty,
        sellable_qty,
        current_recipe_id,
        current_version,
        current_yield,
        current_labor_minutes,
        shelf_life_days,
    ) = product.ok_or(ApiError::NotFound("product_not_found"))?;

    let (recipe_id, recipe_version, yield_qty, labor_minutes) = match (
        current_recipe_id,
        current_version,
        current_yield,
        current_labor_minutes,
    ) {
        (Some(recipe_id), Some(version), Some(yield_qty), Some(labor_minutes)) => {
            (recipe_id, version, yield_qty, labor_minutes)
        }
        _ => return Err(ApiError::Conflict("recipe_required")),
    };

    // The rate belongs to the tenant, so it is read once per plan. Zero means
    // no rate is configured and the run snapshots material cost only.
    let (labor_rate_per_hour,): (Decimal,) =
        sqlx::query_as("select labor_rate_per_hour from tenants where id = $1")
            .bind(tenant_id)
            .fetch_one(&mut **tx)
            .await?;

    // Material order is part of the lock order: every run locks batches in
    // material id order, so two runs sharing materials cannot deadlock.
    let items: Vec<ItemRow> = sqlx::query_as(
        "select ri.material_id, m.name, u.code, ri.qty, ri.waste_pct
         from recipe_items ri
         join materials m on m.id = ri.material_id
         join units u on u.id = m.base_unit_id
         where ri.recipe_id = $1
         order by ri.material_id",
    )
    .bind(recipe_id)
    .fetch_all(&mut **tx)
    .await?;

    // The API refuses to create an itemless recipe; this guards against rows
    // that reached the database another way.
    if items.is_empty() {
        return Err(ApiError::Conflict("recipe_items_required"));
    }

    // A batch past its expiry date is not usable, so it is left for a write-off
    // instead of being consumed; if that leaves too little, the plan reports a
    // shortage. `expires_at` is a date, so a batch expiring today is still good.
    let batch_sql = if lock {
        "select id, received_at, expires_at, qty_remaining, unit_cost
         from material_batches
         where tenant_id = $1 and material_id = $2 and qty_remaining > 0
           and (expires_at is null or expires_at >= current_date)
         order by expires_at asc nulls last, received_at asc
         for update"
    } else {
        "select id, received_at, expires_at, qty_remaining, unit_cost
         from material_batches
         where tenant_id = $1 and material_id = $2 and qty_remaining > 0
           and (expires_at is null or expires_at >= current_date)
         order by expires_at asc nulls last, received_at asc"
    };

    let mut materials = Vec::with_capacity(items.len());
    let mut shortages = Vec::new();

    for (material_id, material_name, unit_code, item_qty, waste_pct) in items {
        let required =
            nata_core::material_requirement(item_qty, waste_pct, production_qty, yield_qty)
                .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?
                .round_dp_with_strategy(LEDGER_SCALE, RoundingStrategy::ToPositiveInfinity);

        let rows: Vec<BatchRow> = sqlx::query_as(batch_sql)
            .bind(tenant_id)
            .bind(material_id)
            .fetch_all(&mut **tx)
            .await?;

        let mut costs: HashMap<Uuid, Decimal> = HashMap::with_capacity(rows.len());
        let mut batches: Vec<StockBatch<Uuid>> = Vec::with_capacity(rows.len());
        let mut expiries: HashMap<Uuid, Option<NaiveDate>> = HashMap::with_capacity(rows.len());

        for (id, received_at, expires_at, qty_remaining, unit_cost) in rows {
            costs.insert(id, unit_cost);
            expiries.insert(id, expires_at);
            batches.push(StockBatch {
                id,
                received_at,
                expires_at,
                qty_remaining,
            });
        }

        // `qty_remaining` is `numeric(18,3)` and the query already dropped
        // non-positive rows, so this sum is exact at ledger scale.
        let available = at_scale(
            batches
                .iter()
                .fold(Decimal::ZERO, |total, batch| total + batch.qty_remaining),
            LEDGER_SCALE,
        );

        let allocation = nata_core::allocate(&batches, required)
            .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;

        if !allocation.is_fully_allocated() {
            shortages.push(ShortageView {
                material_id,
                material_name: material_name.clone(),
                required,
                available,
                shortfall: allocation.shortfall,
            });
        }

        let lines = allocation
            .lines
            .iter()
            .map(|line| PlannedLine {
                batch_id: line.id,
                expires_at: expiries[&line.id],
                qty: line.qty,
                unit_cost: costs[&line.id],
            })
            .collect();

        materials.push(MaterialPlan {
            material_id,
            material_name,
            unit_code,
            required,
            available,
            allocation,
            costs,
            lines,
        });
    }

    Ok(PlannedOrder {
        product_name,
        sell_price,
        stock_qty,
        sellable_qty,
        recipe_id,
        recipe_version,
        yield_qty,
        labor_minutes,
        labor_rate_per_hour,
        shelf_life_days,
        materials,
        shortages,
    })
}

/// Snapshot cost from the plan.
///
/// A recipe's `labor_minutes` covers one batch, so a run of `qty` units takes
/// `qty / yield_qty` batches of labor. Overhead stays zero until a tenant
/// overhead input exists; the core already accepts the component.
fn order_cost(planned: &PlannedOrder, production_qty: Decimal) -> Result<ProductionCost, ApiError> {
    let consumption_costs: Vec<Consumption> = planned
        .materials
        .iter()
        .flat_map(|material| {
            material.lines.iter().map(|line| Consumption {
                qty: line.qty,
                unit_cost: line.unit_cost,
            })
        })
        .collect();

    let hours = Decimal::from(planned.labor_minutes) / Decimal::from(60);
    let batches = production_qty / planned.yield_qty;
    let labor_cost = planned.labor_rate_per_hour * hours * batches;

    nata_core::production_cost(
        &consumption_costs,
        labor_cost,
        Decimal::ZERO,
        production_qty,
    )
    .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))
}

pub async fn preview_order(
    State(state): State<AppState>,
    guard: Require<CanReadProduction>,
    Json(body): Json<ProductionBody>,
) -> Result<Json<ProductionPreviewView>, ApiError> {
    let auth = guard.auth;
    let production_qty = normalized_qty(body.qty)?;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let planned = plan_order(
        &mut tx,
        auth.tenant_id,
        body.product_id,
        production_qty,
        false,
    )
    .await?;

    let can_produce = planned.shortages.is_empty();
    let cost = if can_produce {
        Some(order_cost(&planned, production_qty)?)
    } else {
        None
    };

    let (margin, margin_pct) = match &cost {
        Some(cost) => {
            let margin = planned.sell_price - cost.unit_cost;
            let margin_pct = if planned.sell_price > Decimal::ZERO {
                Some((margin / planned.sell_price * Decimal::from(100)).round_dp(2))
            } else {
                None
            };

            (Some(margin), margin_pct)
        }
        None => (None, None),
    };

    let materials = planned
        .materials
        .iter()
        .map(|material| PreviewMaterialView {
            material_id: material.material_id,
            material_name: material.material_name.clone(),
            unit_code: material.unit_code.clone(),
            required: material.required,
            available: material.available,
            shortfall: material.allocation.shortfall,
            batches: material
                .lines
                .iter()
                .map(|line| PreviewBatchView {
                    batch_id: line.batch_id,
                    expires_at: line.expires_at,
                    qty: line.qty,
                    unit_cost: line.unit_cost,
                })
                .collect(),
        })
        .collect();

    tx.commit().await?;

    Ok(Json(ProductionPreviewView {
        product_id: body.product_id,
        product_name: planned.product_name,
        sell_price: planned.sell_price,
        stock_qty: planned.stock_qty,
        sellable_qty: planned.sellable_qty,
        recipe_id: planned.recipe_id,
        recipe_version: planned.recipe_version,
        yield_qty: planned.yield_qty,
        labor_minutes: planned.labor_minutes,
        labor_rate_per_hour: planned.labor_rate_per_hour,
        qty: production_qty,
        can_produce,
        material_cost: cost.map(|cost| at_scale(cost.material_cost, 2)),
        labor_cost: cost.map(|cost| at_scale(cost.labor_cost, 2)),
        overhead_cost: cost.map(|cost| at_scale(cost.overhead_cost, 2)),
        unit_cost: cost.map(|cost| at_scale(cost.unit_cost, 4)),
        margin: margin.map(|value| at_scale(value, 2)),
        margin_pct: margin_pct.map(|value| at_scale(value, 2)),
        materials,
    }))
}

pub async fn create_order(
    State(state): State<AppState>,
    guard: Require<CanWriteProduction>,
    Json(body): Json<ProductionBody>,
) -> Result<(StatusCode, Json<ProductionOrderView>), ApiError> {
    let auth = guard.auth;
    let production_qty = normalized_qty(body.qty)?;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let planned = plan_order(
        &mut tx,
        auth.tenant_id,
        body.product_id,
        production_qty,
        true,
    )
    .await?;

    // D3: block the whole run and show every gap at once. Nothing has been
    // written yet, and dropping the transaction rolls back cleanly.
    if !planned.shortages.is_empty() {
        return Err(ApiError::ConflictDetails(
            "insufficient_stock",
            serde_json::json!({ "shortages": planned.shortages }),
        ));
    }

    let cost = order_cost(&planned, production_qty)?;
    let produced_on = body.produced_on.unwrap_or_else(|| Utc::now().date_naive());

    let (order_id, created_at, material_cost, labor_cost, overhead_cost, unit_cost): (
        Uuid,
        DateTime<Utc>,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
    ) = sqlx::query_as(
        "insert into production_orders
             (tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on,
              material_cost, labor_cost, overhead_cost, unit_cost, created_by)
         values ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10)
         returning id, created_at, material_cost, labor_cost, overhead_cost, unit_cost",
    )
    .bind(auth.tenant_id)
    .bind(body.product_id)
    .bind(planned.recipe_id)
    .bind(production_qty)
    .bind(produced_on)
    .bind(cost.material_cost)
    .bind(cost.labor_cost)
    .bind(cost.overhead_cost)
    .bind(cost.unit_cost)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;

    let mut consumptions: Vec<ConsumptionView> = Vec::new();

    for material in planned.materials {
        let MaterialPlan {
            material_id,
            material_name,
            allocation,
            costs,
            ..
        } = material;

        for line in allocation.lines {
            let unit_cost = costs[&line.id];

            sqlx::query(
                "update material_batches
                 set qty_remaining = qty_remaining - $3
                 where id = $1 and tenant_id = $2",
            )
            .bind(line.id)
            .bind(auth.tenant_id)
            .bind(line.qty)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "insert into stock_movements
                     (tenant_id, batch_id, type, qty, unit_cost, created_by)
                 values ($1, $2, 'production_use', $3, $4, $5)",
            )
            .bind(auth.tenant_id)
            .bind(line.id)
            .bind(-line.qty)
            .bind(unit_cost)
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "insert into production_consumptions
                     (production_order_id, batch_id, qty, unit_cost)
                 values ($1, $2, $3, $4)",
            )
            .bind(order_id)
            .bind(line.id)
            .bind(line.qty)
            .bind(unit_cost)
            .execute(&mut *tx)
            .await?;

            consumptions.push(ConsumptionView {
                batch_id: line.id,
                material_id,
                material_name: material_name.clone(),
                qty: line.qty,
                unit_cost,
            });
        }
    }

    // The run becomes one finished-goods batch. Its expiry is the product's
    // shelf life from the day it was produced; a product without a shelf life
    // never expires.
    let expires_at = planned
        .shelf_life_days
        .map(|days| produced_on + Duration::days(i64::from(days)));

    let (product_batch_id,): (Uuid,) = sqlx::query_as(
        "insert into product_batches
             (tenant_id, product_id, production_order_id, produced_on, expires_at,
              qty_received, qty_remaining, unit_cost)
         values ($1, $2, $3, $4, $5, $6, $6, $7)
         returning id",
    )
    .bind(auth.tenant_id)
    .bind(body.product_id)
    .bind(order_id)
    .bind(produced_on)
    .bind(expires_at)
    .bind(production_qty)
    .bind(cost.unit_cost)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "insert into stock_movements
             (tenant_id, product_batch_id, type, qty, unit_cost, created_by)
         values ($1, $2, 'production', $3, $4, $5)",
    )
    .bind(auth.tenant_id)
    .bind(product_batch_id)
    .bind(production_qty)
    .bind(cost.unit_cost)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "update products
         set stock_qty = stock_qty + $3, updated_at = now()
         where id = $1 and tenant_id = $2",
    )
    .bind(body.product_id)
    .bind(auth.tenant_id)
    .bind(production_qty)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(ProductionOrderView {
            id: order_id,
            product_id: body.product_id,
            product_name: planned.product_name,
            recipe_id: planned.recipe_id,
            recipe_version: planned.recipe_version,
            planned_qty: production_qty,
            actual_qty: production_qty,
            material_cost,
            labor_cost,
            overhead_cost,
            unit_cost,
            produced_on,
            status: "done".to_string(),
            created_at,
            consumptions,
        }),
    ))
}

/// How far ahead a suggestion may look. Past a fortnight the sales rate is a
/// guess, and the shelf-life cap would have refused most of the run anyway.
const MAX_DAYS_AHEAD: i32 = 14;

#[derive(Debug, Deserialize)]
pub struct SuggestionsQuery {
    /// Days of sales the run is meant to cover. Defaults to tomorrow.
    pub days: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct SuggestionsView {
    pub days_ahead: i32,
    /// The sales window the daily rate is measured over, from the tenant's
    /// settings. Reported so the screen can state its own basis.
    pub window_days: i32,
    pub products: Vec<SuggestionView>,
}

#[derive(Debug, Serialize)]
pub struct SuggestionView {
    pub product_id: Uuid,
    pub product_name: String,
    pub average_daily_sales: Decimal,
    /// Stock that can still be sold; expired batches are already excluded.
    pub sellable_stock: Decimal,
    /// Sales the run is meant to cover, before the shelf-life cap.
    pub target_qty: Decimal,
    pub recommended_qty: Decimal,
    /// True when shelf life, not the sales target, decided the quantity.
    pub capped_by_shelf_life: bool,
    /// True when nothing sold in the window, so there is no rate to plan from.
    pub insufficient_history: bool,
}

type SuggestionRow = (Uuid, String, Option<i32>, Decimal, Decimal);

/// Suggests how much of each product to make for the next `days` days.
///
/// Only products with a recipe are listed: without one, the run cannot be
/// previewed or created, so a suggestion would be an action the app refuses.
pub async fn suggestions(
    State(state): State<AppState>,
    guard: Require<CanReadProduction>,
    Query(query): Query<SuggestionsQuery>,
) -> Result<Json<SuggestionsView>, ApiError> {
    let auth = guard.auth;
    let days_ahead = query.days.unwrap_or(1);

    if !(1..=MAX_DAYS_AHEAD).contains(&days_ahead) {
        return Err(ApiError::BadRequest("invalid_days"));
    }

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // The same window the shopping list and the reports use: one setting, one
    // explanation, so the numbers on different screens cannot disagree.
    let (window_days,): (i32,) =
        sqlx::query_as("select usage_window_days from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    let rows: Vec<SuggestionRow> = sqlx::query_as(
        "with sold as (
             select si.product_id, sum(si.qty)::numeric(18,3) as qty
             from sale_items si
             join sales s on s.id = si.sale_id
             where si.tenant_id = $1
               and s.sold_at >= now() - make_interval(days => $2)
             group by si.product_id
         )
         select p.id, p.name, p.shelf_life_days,
                (select coalesce(sum(b.qty_remaining), 0)::numeric(18,3)
                   from product_batches b
                  where b.tenant_id = p.tenant_id
                    and b.product_id = p.id
                    and b.qty_remaining > 0
                    and (b.expires_at is null or b.expires_at >= current_date)) as sellable_qty,
                coalesce(sold.qty, 0) as sold_qty
         from products p
         left join sold on sold.product_id = p.id
         where p.tenant_id = $1 and p.is_active and p.current_recipe_id is not null
         order by p.name",
    )
    .bind(auth.tenant_id)
    .bind(window_days)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    let mut products = Vec::with_capacity(rows.len());

    for (product_id, product_name, shelf_life_days, sellable_stock, sold_qty) in rows {
        let average_daily_sales = nata_core::average_daily_sales(sold_qty, window_days as u32)
            .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;

        let plan = nata_core::production_plan(nata_core::ProductionInput {
            sellable_stock,
            average_daily_sales,
            days_ahead,
            shelf_life_days,
        })
        .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;

        products.push(SuggestionView {
            product_id,
            product_name,
            average_daily_sales: at_scale(average_daily_sales, LEDGER_SCALE),
            sellable_stock,
            target_qty: plan.target_qty,
            recommended_qty: plan.recommended_qty,
            capped_by_shelf_life: plan.capped_by_shelf_life,
            insufficient_history: sold_qty == Decimal::ZERO,
        });
    }

    // Largest gap first, so the screen reads as a work list.
    products.sort_by(|a, b| {
        b.recommended_qty
            .cmp(&a.recommended_qty)
            .then_with(|| a.product_name.cmp(&b.product_name))
    });

    Ok(Json(SuggestionsView {
        days_ahead,
        window_days,
        products,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/production-orders", post(create_order))
        .route("/production-orders/preview", post(preview_order))
        .route("/production/suggestions", get(suggestions))
}
