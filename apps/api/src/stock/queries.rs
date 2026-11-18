//! Stock reads: on-hand per material, active batches, and the movement trail.
//!
//! All three read the batch ledger. On-hand is never stored as a scalar, so
//! these queries are the only definition of "how much do we have".

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::batches::BatchView;
use crate::auth::guard::{CanReadMaterials, Require};
use crate::master::units::UnitRef;
use crate::{error::ApiError, AppState};

const DEFAULT_MOVEMENT_LIMIT: i64 = 50;
const MAX_MOVEMENT_LIMIT: i64 = 200;

#[derive(Debug, Serialize)]
pub struct StockLevelView {
    pub material_id: Uuid,
    pub name: String,
    pub base_unit: UnitRef,
    pub on_hand: Decimal,
    pub min_stock: Decimal,
    pub below_min: bool,
    pub active_batches: i64,
    pub nearest_expiry: Option<NaiveDate>,
    pub last_cost: Decimal,
}

#[derive(Debug, Serialize)]
pub struct MovementView {
    pub id: Uuid,
    /// Material batch; null for a finished-goods movement.
    pub batch_id: Option<Uuid>,
    pub material_id: Option<Uuid>,
    pub material_name: Option<String>,
    pub unit_code: Option<String>,
    /// Finished-goods batch; null for a material movement.
    pub product_batch_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub product_name: Option<String>,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub qty: Decimal,
    pub unit_cost: Option<Decimal>,
    pub note: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct BatchFilter {
    pub material_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct MovementFilter {
    pub material_id: Option<Uuid>,
    pub batch_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub product_batch_id: Option<Uuid>,
    pub limit: Option<i64>,
}

type OnHandRow = (
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

type MovementRow = (
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    Option<String>,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    String,
    Decimal,
    Option<Decimal>,
    Option<String>,
    Option<Uuid>,
    DateTime<Utc>,
);

/// Refuses an unknown material id instead of returning an empty list, which
/// would hide a typo or a cross-tenant id.
async fn ensure_material(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    material_id: Uuid,
) -> Result<(), ApiError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("select id from materials where id = $1 and tenant_id = $2")
            .bind(material_id)
            .bind(tenant_id)
            .fetch_optional(&mut **tx)
            .await?;

    row.map(|_| ())
        .ok_or(ApiError::NotFound("material_not_found"))
}

/// Same contract as `ensure_material`, for a finished-goods filter.
async fn ensure_product(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    product_id: Uuid,
) -> Result<(), ApiError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("select id from products where id = $1 and tenant_id = $2 and is_active")
            .bind(product_id)
            .bind(tenant_id)
            .fetch_optional(&mut **tx)
            .await?;

    row.map(|_| ())
        .ok_or(ApiError::NotFound("product_not_found"))
}

pub async fn on_hand(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<Vec<StockLevelView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // `on_hand` only counts stock inside its shelf life, because that is what
    // can still be used. The batch count and the nearest expiry keep looking at
    // every non-empty batch, so a material whose only stock is expired still
    // shows the batch that needs a write-off.
    let rows: Vec<OnHandRow> = sqlx::query_as(
        "select m.id, m.name, u.id, u.code, m.min_stock, m.last_cost,
                coalesce(sum(b.qty_remaining) filter (
                    where b.expires_at is null or b.expires_at >= current_date
                ), 0)::numeric(18,3) as on_hand,
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
         order by m.name, m.id",
    )
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(
                    material_id,
                    name,
                    unit_id,
                    unit_code,
                    min_stock,
                    last_cost,
                    on_hand,
                    active_batches,
                    nearest_expiry,
                )| StockLevelView {
                    material_id,
                    name,
                    base_unit: UnitRef {
                        id: unit_id,
                        code: unit_code,
                    },
                    on_hand,
                    min_stock,
                    below_min: on_hand < min_stock,
                    active_batches,
                    nearest_expiry,
                    last_cost,
                },
            )
            .collect(),
    ))
}

/// Active batches in FEFO order: earliest expiry first, null expiry last, then
/// the earliest receipt. Depleted batches are history, not stock.
pub async fn list_batches(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
    Query(filter): Query<BatchFilter>,
) -> Result<Json<Vec<BatchView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    if let Some(material_id) = filter.material_id {
        ensure_material(&mut tx, auth.tenant_id, material_id).await?;
    }

    let rows: Vec<BatchView> = sqlx::query_as(
        "select b.id, b.material_id, b.supplier_id, b.received_at, b.expires_at,
                b.qty_received, b.qty_remaining, b.unit_cost, b.note, b.created_at
         from material_batches b
         where b.tenant_id = $1
           and b.qty_remaining > 0
           and ($2::uuid is null or b.material_id = $2)
         order by b.expires_at asc nulls last, b.received_at asc, b.created_at asc",
    )
    .bind(auth.tenant_id)
    .bind(filter.material_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(rows))
}

pub async fn list_movements(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
    Query(filter): Query<MovementFilter>,
) -> Result<Json<Vec<MovementView>>, ApiError> {
    let auth = guard.auth;
    let limit = filter
        .limit
        .unwrap_or(DEFAULT_MOVEMENT_LIMIT)
        .clamp(1, MAX_MOVEMENT_LIMIT);
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    if let Some(material_id) = filter.material_id {
        ensure_material(&mut tx, auth.tenant_id, material_id).await?;
    }
    if let Some(product_id) = filter.product_id {
        ensure_product(&mut tx, auth.tenant_id, product_id).await?;
    }

    // A movement points at exactly one ledger, so the other side is null. The
    // trail is one list on purpose: a production run shows both the material it
    // consumed and the finished batch it produced.
    let rows: Vec<MovementRow> = sqlx::query_as(
        "select mv.id, mv.batch_id, b.material_id, m.name as material_name,
                u.code as unit_code,
                mv.product_batch_id, pb.product_id, p.name as product_name,
                mv.type, mv.qty, mv.unit_cost, mv.note, mv.created_by, mv.created_at
         from stock_movements mv
         left join material_batches b on b.id = mv.batch_id
         left join materials m on m.id = b.material_id
         left join units u on u.id = m.base_unit_id
         left join product_batches pb on pb.id = mv.product_batch_id
         left join products p on p.id = pb.product_id
         where mv.tenant_id = $1
           and ($2::uuid is null or b.material_id = $2)
           and ($3::uuid is null or mv.batch_id = $3)
           and ($4::uuid is null or pb.product_id = $4)
           and ($5::uuid is null or mv.product_batch_id = $5)
         order by mv.created_at desc, mv.id desc
         limit $6",
    )
    .bind(auth.tenant_id)
    .bind(filter.material_id)
    .bind(filter.batch_id)
    .bind(filter.product_id)
    .bind(filter.product_batch_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(
                    id,
                    batch_id,
                    material_id,
                    material_name,
                    unit_code,
                    product_batch_id,
                    product_id,
                    product_name,
                    movement_type,
                    qty,
                    unit_cost,
                    note,
                    created_by,
                    created_at,
                )| MovementView {
                    id,
                    batch_id,
                    material_id,
                    material_name,
                    unit_code,
                    product_batch_id,
                    product_id,
                    product_name,
                    movement_type,
                    qty,
                    unit_cost,
                    note,
                    created_by,
                    created_at,
                },
            )
            .collect(),
    ))
}
