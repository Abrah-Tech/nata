//! Finished-goods batch stock: the active batch list and its corrections.
//!
//! `product_batches` is the finished-goods ledger (D8), so it needs the same
//! write path materials have: waste, expiry write-off, and count adjustments.
//! Without it an expired batch keeps its remaining quantity and FEFO keeps
//! allocating it to sales. Every correction is a signed movement on
//! `stock_movements`, and the batch is locked while it is corrected so two
//! people cannot take the same remaining quantity.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::corrections::{validate, CorrectionType};
use super::queries::ensure_warehouse;
use crate::auth::guard::{CanAdjustStock, CanReadRecipes, Require};
use crate::{error::ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct ProductBatchView {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub production_order_id: Option<Uuid>,
    pub produced_on: NaiveDate,
    pub expires_at: Option<NaiveDate>,
    pub qty_received: Decimal,
    pub qty_remaining: Decimal,
    pub unit_cost: Option<Decimal>,
    pub warehouse_id: Option<Uuid>,
    pub warehouse_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ProductBatchFilter {
    pub product_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
}

type ProductBatchRow = (
    Uuid,
    Uuid,
    String,
    Option<Uuid>,
    NaiveDate,
    Option<NaiveDate>,
    Decimal,
    Decimal,
    Option<Decimal>,
    Option<Uuid>,
    Option<String>,
    DateTime<Utc>,
);

fn batch_view(row: ProductBatchRow) -> ProductBatchView {
    let (
        id,
        product_id,
        product_name,
        production_order_id,
        produced_on,
        expires_at,
        qty_received,
        qty_remaining,
        unit_cost,
        warehouse_id,
        warehouse_name,
        created_at,
    ) = row;

    ProductBatchView {
        id,
        product_id,
        product_name,
        production_order_id,
        produced_on,
        expires_at,
        qty_received,
        qty_remaining,
        unit_cost,
        warehouse_id,
        warehouse_name,
        created_at,
    }
}

/// Active finished-goods batches in FEFO order: earliest expiry first, no
/// expiry last, then the earliest production day.
pub async fn list_batches(
    State(state): State<AppState>,
    guard: Require<CanReadRecipes>,
    Query(filter): Query<ProductBatchFilter>,
) -> Result<Json<Vec<ProductBatchView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    if let Some(warehouse_id) = filter.warehouse_id {
        ensure_warehouse(&mut tx, auth.tenant_id, warehouse_id).await?;
    }

    let rows: Vec<ProductBatchRow> = sqlx::query_as(
        "select b.id, b.product_id, p.name, b.production_order_id,
                b.produced_on, b.expires_at, b.qty_received, b.qty_remaining,
                b.unit_cost, b.warehouse_id, w.name, b.created_at
         from product_batches b
         join products p on p.id = b.product_id and p.tenant_id = b.tenant_id
         left join warehouses w on w.id = b.warehouse_id
         where b.tenant_id = $1
           and b.qty_remaining > 0
           and ($2::uuid is null or b.product_id = $2)
           and ($3::uuid is null or b.warehouse_id = $3)
         order by b.expires_at asc nulls last, b.produced_on asc, b.created_at asc",
    )
    .bind(auth.tenant_id)
    .bind(filter.product_id)
    .bind(filter.warehouse_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(rows.into_iter().map(batch_view).collect()))
}

#[derive(Debug, Deserialize)]
pub struct ProductCorrectionBody {
    pub product_batch_id: Uuid,
    #[serde(rename = "type")]
    pub correction_type: CorrectionType,
    /// Signed like every movement: outbound is negative, inbound positive.
    /// Waste and expiry must be negative; an adjustment may go either way.
    pub qty: Decimal,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProductCorrectionView {
    pub id: Uuid,
    pub product_batch_id: Uuid,
    pub product_id: Uuid,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub qty: Decimal,
    pub unit_cost: Option<Decimal>,
    pub note: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub batch_qty_remaining: Decimal,
}

type CorrectedBatch = (Uuid, Decimal, Decimal, Option<Decimal>, Option<Uuid>);

pub async fn correct(
    State(state): State<AppState>,
    guard: Require<CanAdjustStock>,
    Json(body): Json<ProductCorrectionBody>,
) -> Result<(StatusCode, Json<ProductCorrectionView>), ApiError> {
    let auth = guard.auth;
    validate(body.correction_type, body.qty)?;

    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // Lock the batch: the bounds check below is only meaningful while no one
    // else can change the same remaining quantity.
    let batch: Option<CorrectedBatch> = sqlx::query_as(
        "select product_id, qty_received, qty_remaining, unit_cost, warehouse_id
         from product_batches
         where id = $1 and tenant_id = $2
         for update",
    )
    .bind(body.product_batch_id)
    .bind(auth.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (product_id, qty_received, qty_remaining, unit_cost, warehouse_id) =
        batch.ok_or(ApiError::NotFound("batch_not_found"))?;

    let new_remaining = qty_remaining + body.qty;

    if new_remaining.is_sign_negative() {
        return Err(ApiError::BadRequest("insufficient_batch_quantity"));
    }

    if new_remaining > qty_received {
        return Err(ApiError::BadRequest("quantity_exceeds_received"));
    }

    let (movement_id, created_at): (Uuid, DateTime<Utc>) = sqlx::query_as(
        "insert into stock_movements
             (tenant_id, product_batch_id, type, qty, unit_cost, note, created_by, warehouse_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         returning id, created_at",
    )
    .bind(auth.tenant_id)
    .bind(body.product_batch_id)
    .bind(body.correction_type.as_str())
    .bind(body.qty)
    .bind(unit_cost)
    .bind(note)
    .bind(auth.user_id)
    .bind(warehouse_id)
    .fetch_one(&mut *tx)
    .await?;

    let (batch_qty_remaining,): (Decimal,) = sqlx::query_as(
        "update product_batches set qty_remaining = $3
         where id = $1 and tenant_id = $2
         returning qty_remaining",
    )
    .bind(body.product_batch_id)
    .bind(auth.tenant_id)
    .bind(new_remaining)
    .fetch_one(&mut *tx)
    .await?;

    // Keep the scalar cache in step, exactly like production and a sale.
    sqlx::query(
        "update products
         set stock_qty = stock_qty + $3, updated_at = now()
         where id = $1 and tenant_id = $2",
    )
    .bind(product_id)
    .bind(auth.tenant_id)
    .bind(body.qty)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(ProductCorrectionView {
            id: movement_id,
            product_batch_id: body.product_batch_id,
            product_id,
            movement_type: body.correction_type.as_str().to_string(),
            qty: body.qty,
            unit_cost,
            note: note.map(str::to_string),
            created_by: Some(auth.user_id),
            created_at,
            batch_qty_remaining,
        }),
    ))
}
