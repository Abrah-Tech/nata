//! Tenant settings.
//!
//! Three values so far: the hourly labor and overhead rates used to cost a
//! production run, and the window purchase recommendations average usage over.
//! All belong to the tenant because they describe the business, not a single
//! recipe.

use axum::{extract::State, routing::get, Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::auth::guard::{CanManageTenant, Require};
use crate::auth::AuthContext;
use crate::{error::ApiError, AppState};

/// Matches `tenants.labor_rate_per_hour`.
const MONEY_SCALE: u32 = 2;

/// Matches the check on `tenants.usage_window_days`.
const MIN_WINDOW_DAYS: i32 = 1;
const MAX_WINDOW_DAYS: i32 = 365;

/// Validates and rescales one hourly rate. `code` is the error the client maps
/// back to the field, so labor and overhead report their own failure.
fn normalized_rate(
    value: Option<Decimal>,
    code: &'static str,
) -> Result<Option<Decimal>, ApiError> {
    match value {
        Some(value) if value.is_sign_negative() => Err(ApiError::BadRequest(code)),
        Some(mut value) => {
            value.rescale(MONEY_SCALE);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub labor_rate_per_hour: Decimal,
    /// Priced on the same recipe minutes as labor.
    pub overhead_rate_per_hour: Decimal,
    /// Days of movement history purchase recommendations average over.
    pub usage_window_days: i32,
}

/// Every field is optional so one screen can save one value without sending
/// the others back. Omitted fields keep their stored value.
#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub labor_rate_per_hour: Option<Decimal>,
    pub overhead_rate_per_hour: Option<Decimal>,
    pub usage_window_days: Option<i32>,
}

/// Every member may read the settings: the rate explains the HPP they see on a
/// run, and the window explains the shopping list.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<SettingsView>, ApiError> {
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (labor_rate_per_hour, overhead_rate_per_hour, usage_window_days): (Decimal, Decimal, i32) =
        sqlx::query_as(
            "select labor_rate_per_hour, overhead_rate_per_hour, usage_window_days
         from tenants where id = $1",
        )
        .bind(auth.tenant_id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Json(SettingsView {
        labor_rate_per_hour,
        overhead_rate_per_hour,
        usage_window_days,
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<SettingsView>, ApiError> {
    let auth = guard.auth;

    let rate = normalized_rate(body.labor_rate_per_hour, "invalid_labor_rate")?;
    let overhead = normalized_rate(body.overhead_rate_per_hour, "invalid_overhead_rate")?;

    if let Some(days) = body.usage_window_days {
        if !(MIN_WINDOW_DAYS..=MAX_WINDOW_DAYS).contains(&days) {
            return Err(ApiError::BadRequest("invalid_usage_window"));
        }
    }

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // Read the current values first: the audit entry is the only record of what
    // the rate was before this change.
    let (previous_rate, previous_overhead, previous_window): (Decimal, Decimal, i32) =
        sqlx::query_as(
            "select labor_rate_per_hour, overhead_rate_per_hour, usage_window_days
         from tenants where id = $1 for update",
        )
        .bind(auth.tenant_id)
        .fetch_one(&mut *tx)
        .await?;

    let (labor_rate_per_hour, overhead_rate_per_hour, usage_window_days): (Decimal, Decimal, i32) =
        sqlx::query_as(
            "update tenants
         set labor_rate_per_hour = coalesce($2, labor_rate_per_hour),
             overhead_rate_per_hour = coalesce($3, overhead_rate_per_hour),
             usage_window_days = coalesce($4, usage_window_days),
             updated_at = now()
         where id = $1
         returning labor_rate_per_hour, overhead_rate_per_hour, usage_window_days",
        )
        .bind(auth.tenant_id)
        .bind(rate)
        .bind(overhead)
        .bind(body.usage_window_days)
        .fetch_one(&mut *tx)
        .await?;

    let mut metadata = Map::new();
    if let Some(new_rate) = rate {
        metadata.insert(
            "labor_rate_per_hour".to_string(),
            json!({ "from": previous_rate.to_string(), "to": new_rate.to_string() }),
        );
    }
    if let Some(new_overhead) = overhead {
        metadata.insert(
            "overhead_rate_per_hour".to_string(),
            json!({ "from": previous_overhead.to_string(), "to": new_overhead.to_string() }),
        );
    }
    if let Some(new_window) = body.usage_window_days {
        metadata.insert(
            "usage_window_days".to_string(),
            json!({ "from": previous_window, "to": new_window }),
        );
    }

    nata_db::audit::record(
        &mut tx,
        auth.tenant_id,
        Some(auth.user_id),
        "settings.updated",
        "tenant",
        Some(auth.tenant_id),
        Some(Value::Object(metadata)),
    )
    .await?;

    tx.commit().await?;

    Ok(Json(SettingsView {
        labor_rate_per_hour,
        overhead_rate_per_hour,
        usage_window_days,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(update_settings))
}
