//! Stock take: compare what the shelf actually holds with the ledger.
//!
//! The sheet is counted per batch and prefilled with the ledger's own quantity,
//! so a row the owner did not touch writes nothing. Every difference becomes one
//! signed `adjustment` movement, and the whole sheet is applied in a single
//! transaction: a count that cannot be recorded in full is not recorded at all.
//!
//! A count is a statement about a moment, so it is only ever written forward.
//! The movements it produces are the audit trail, and the ledger keeps its
//! history: nothing here rewrites an earlier correction.

use std::collections::HashSet;

use axum::{extract::State, Json};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::corrections::set_remaining;
use crate::auth::guard::{CanAdjustStock, Require};
use crate::{error::ApiError, AppState};

#[derive(Debug, Deserialize)]
pub struct CountSheet {
    /// One note for the whole sheet, so the trail reads as one count rather
    /// than a row of unrelated adjustments.
    pub note: Option<String>,
    pub counts: Vec<CountedBatch>,
}

#[derive(Debug, Deserialize)]
pub struct CountedBatch {
    pub batch_id: Uuid,
    /// What the shelf holds, an absolute quantity and never a difference.
    pub qty: Decimal,
}

#[derive(Debug, Serialize)]
pub struct CountSheetView {
    /// How many batches were written to.
    pub applied: i64,
    /// How many rows already agreed with the ledger and so were left alone.
    pub unchanged: i64,
}

pub async fn apply(
    State(state): State<AppState>,
    guard: Require<CanAdjustStock>,
    Json(body): Json<CountSheet>,
) -> Result<Json<CountSheetView>, ApiError> {
    let auth = guard.auth;

    if body.counts.is_empty() {
        return Err(ApiError::BadRequest("empty_count_sheet"));
    }

    // A repeated batch would be counted twice, and the second pass would read
    // the remainder the first one just wrote.
    let mut seen = HashSet::with_capacity(body.counts.len());

    for count in &body.counts {
        if count.qty.is_sign_negative() || !seen.insert(count.batch_id) {
            return Err(ApiError::BadRequest("invalid_count"));
        }
    }

    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let mut applied = 0;
    let mut unchanged = 0;

    for count in body.counts {
        if set_remaining(&mut tx, &auth, count.batch_id, count.qty, note.clone())
            .await?
            .is_some()
        {
            applied += 1;
        } else {
            unchanged += 1;
        }
    }

    tx.commit().await?;

    Ok(Json(CountSheetView { applied, unchanged }))
}
