//! Stock corrections: waste, expiry write-off, and count adjustments.
//!
//! Every correction is a signed movement, so on-hand never changes without a
//! trail. The batch row is locked while it is corrected, so two people cannot
//! take the same remaining quantity.

use axum::{extract::State, http::StatusCode, Json};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::queries::MovementView;
use crate::auth::extract::AuthContext;
use crate::auth::guard::{CanAdjustStock, Require};
use crate::{error::ApiError, AppState};

/// The movement types a person may record by hand. Purchases come from
/// stock-in and production use comes from production orders, so neither is
/// accepted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionType {
    Waste,
    Expired,
    Adjustment,
}

impl CorrectionType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CorrectionType::Waste => "waste",
            CorrectionType::Expired => "expired",
            CorrectionType::Adjustment => "adjustment",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CorrectionBody {
    pub batch_id: Uuid,
    #[serde(rename = "type")]
    pub correction_type: CorrectionType,
    /// Signed like every movement: outbound is negative, inbound positive.
    /// Waste and expiry must be negative; an adjustment may go either way.
    pub qty: Decimal,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CorrectionView {
    #[serde(flatten)]
    pub movement: MovementView,
    pub batch_qty_remaining: Decimal,
}

pub(crate) fn validate(kind: CorrectionType, qty: Decimal) -> Result<(), ApiError> {
    match kind {
        CorrectionType::Waste | CorrectionType::Expired if qty >= Decimal::ZERO => {
            Err(ApiError::BadRequest("invalid_quantity"))
        }
        CorrectionType::Adjustment if qty.is_zero() => {
            Err(ApiError::BadRequest("invalid_quantity"))
        }
        _ => Ok(()),
    }
}

/// A batch row held for the length of the transaction, carrying the fields a
/// movement needs.
///
/// The lock is what makes the quantity checks meaningful: without it two people
/// could both read the same remainder and the second write would be based on a
/// number that no longer holds.
pub(crate) struct LockedBatch {
    pub id: Uuid,
    pub material_id: Uuid,
    /// Where the batch is stored; a correction happens in that location.
    pub warehouse_id: Option<Uuid>,
    pub warehouse_name: Option<String>,
    pub qty_received: Decimal,
    pub qty_remaining: Decimal,
    pub unit_cost: Decimal,
    pub material_name: String,
    pub unit_code: String,
}

type LockedRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<String>,
    Decimal,
    Decimal,
    Decimal,
    String,
    String,
);

pub(crate) async fn lock_batch(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    batch_id: Uuid,
) -> Result<Option<LockedBatch>, ApiError> {
    let row: Option<LockedRow> = sqlx::query_as(
        "select b.id, b.material_id, b.warehouse_id, w.name,
                b.qty_received, b.qty_remaining, b.unit_cost,
                m.name, u.code
         from material_batches b
         join materials m on m.id = b.material_id
         join units u on u.id = m.base_unit_id
         left join warehouses w on w.id = b.warehouse_id
         where b.id = $1 and b.tenant_id = $2
         for update of b",
    )
    .bind(batch_id)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.map(
        |(
            id,
            material_id,
            warehouse_id,
            warehouse_name,
            qty_received,
            qty_remaining,
            unit_cost,
            material_name,
            unit_code,
        )| LockedBatch {
            id,
            material_id,
            warehouse_id,
            warehouse_name,
            qty_received,
            qty_remaining,
            unit_cost,
            material_name,
            unit_code,
        },
    ))
}

/// Writes a signed movement and applies it to a batch the caller has locked.
pub(crate) async fn record_movement(
    tx: &mut Transaction<'_, Postgres>,
    auth: &AuthContext,
    batch: LockedBatch,
    qty: Decimal,
    movement_type: &str,
    note: Option<String>,
) -> Result<CorrectionView, ApiError> {
    let new_remaining = batch.qty_remaining + qty;

    if new_remaining.is_sign_negative() {
        return Err(ApiError::BadRequest("insufficient_batch_quantity"));
    }

    if new_remaining > batch.qty_received {
        return Err(ApiError::BadRequest("quantity_exceeds_received"));
    }

    let (movement_id, created_at): (Uuid, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "insert into stock_movements
             (tenant_id, batch_id, type, qty, unit_cost, note, created_by, warehouse_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         returning id, created_at",
    )
    .bind(auth.tenant_id)
    .bind(batch.id)
    .bind(movement_type)
    .bind(qty)
    .bind(batch.unit_cost)
    .bind(note.clone())
    .bind(auth.user_id)
    .bind(batch.warehouse_id)
    .fetch_one(&mut **tx)
    .await?;

    let (batch_qty_remaining,): (Decimal,) = sqlx::query_as(
        "update material_batches set qty_remaining = $3
         where id = $1 and tenant_id = $2
         returning qty_remaining",
    )
    .bind(batch.id)
    .bind(auth.tenant_id)
    .bind(new_remaining)
    .fetch_one(&mut **tx)
    .await?;

    Ok(CorrectionView {
        movement: MovementView {
            id: movement_id,
            batch_id: Some(batch.id),
            material_id: Some(batch.material_id),
            material_name: Some(batch.material_name),
            unit_code: Some(batch.unit_code),
            product_batch_id: None,
            product_id: None,
            product_name: None,
            warehouse_id: batch.warehouse_id,
            warehouse_name: batch.warehouse_name,
            movement_type: movement_type.to_string(),
            qty,
            unit_cost: Some(batch.unit_cost),
            note,
            created_by: Some(auth.user_id),
            created_at,
        },
        batch_qty_remaining,
    })
}

/// Sets a batch to the quantity that was actually counted, for a stock take.
///
/// Returns `None` when the count already agrees with the ledger, so a row the
/// owner left alone writes nothing at all: an adjustment of zero is noise in the
/// trail, not a record of the count.
pub(crate) async fn set_remaining(
    tx: &mut Transaction<'_, Postgres>,
    auth: &AuthContext,
    batch_id: Uuid,
    counted: Decimal,
    note: Option<String>,
) -> Result<Option<CorrectionView>, ApiError> {
    let batch = lock_batch(tx, auth.tenant_id, batch_id)
        .await?
        .ok_or(ApiError::NotFound("batch_not_found"))?;

    let difference = counted - batch.qty_remaining;

    if difference.is_zero() {
        return Ok(None);
    }

    record_movement(tx, auth, batch, difference, "adjustment", note)
        .await
        .map(Some)
}

pub async fn correct(
    State(state): State<AppState>,
    guard: Require<CanAdjustStock>,
    Json(body): Json<CorrectionBody>,
) -> Result<(StatusCode, Json<CorrectionView>), ApiError> {
    let auth = guard.auth;
    validate(body.correction_type, body.qty)?;

    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let batch = lock_batch(&mut tx, auth.tenant_id, body.batch_id)
        .await?
        .ok_or(ApiError::NotFound("batch_not_found"))?;

    let view = record_movement(
        &mut tx,
        &auth,
        batch,
        body.qty,
        body.correction_type.as_str(),
        note.map(str::to_string),
    )
    .await?;

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(view)))
}
