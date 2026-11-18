//! Alert reads: the tenant's rules and its alert history.
//!
//! The daily job writes alerts; these two routes are how a screen reads them
//! back. They are tenant-scoped like every other read, so RLS applies.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, Require};
use crate::{error::ApiError, AppState};

const DEFAULT_ALERT_LIMIT: i64 = 50;
const MAX_ALERT_LIMIT: i64 = 200;

#[derive(Debug, Serialize)]
pub struct AlertRuleView {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub rule_type: String,
    pub days_before: Option<i32>,
    pub channel: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AlertView {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub alert_type: String,
    /// `open` until the condition stops holding, then `resolved`. History stays
    /// readable, and a repeat warning opens a fresh alert.
    pub status: String,
    pub rule_id: Uuid,
    pub material_id: Option<Uuid>,
    pub material_name: Option<String>,
    pub batch_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub product_name: Option<String>,
    pub product_batch_id: Option<Uuid>,
    pub qty_remaining: Option<Decimal>,
    pub expires_at: Option<NaiveDate>,
    pub days_left: Option<i32>,
    /// Usable stock for a low-stock alert; null for an expiry alert.
    pub on_hand: Option<Decimal>,
    pub min_stock: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub sent_at: Option<DateTime<Utc>>,
    pub delivery_status: String,
    pub attempts: i32,
}

#[derive(Debug, Deserialize)]
pub struct AlertFilter {
    /// `open`, `resolved`, or omitted for both.
    pub status: Option<String>,
    pub limit: Option<i64>,
}

type RuleRow = (Uuid, String, Option<i32>, String, bool, DateTime<Utc>);

/// One alert with both ledger sides joined. The unused side is null, so a
/// material expiry and a finished-goods expiry share this shape.
#[derive(sqlx::FromRow)]
struct AlertRow {
    id: Uuid,
    alert_type: String,
    rule_id: Uuid,
    material_id: Option<Uuid>,
    material_name: Option<String>,
    batch_id: Option<Uuid>,
    product_id: Option<Uuid>,
    product_name: Option<String>,
    product_batch_id: Option<Uuid>,
    qty_remaining: Option<Decimal>,
    expires_at: Option<NaiveDate>,
    days_left: Option<i32>,
    on_hand: Option<Decimal>,
    min_stock: Option<Decimal>,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    sent_at: Option<DateTime<Utc>>,
    delivery_status: String,
    attempts: i32,
}

pub async fn list_rules(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<Vec<AlertRuleView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<RuleRow> = sqlx::query_as(
        "select id, type, days_before, channel, is_active, created_at
         from alert_rules
         where tenant_id = $1
         order by type, days_before nulls first, id",
    )
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(id, rule_type, days_before, channel, is_active, created_at)| AlertRuleView {
                    id,
                    rule_type,
                    days_before,
                    channel,
                    is_active,
                    created_at,
                },
            )
            .collect(),
    ))
}

pub async fn list_alerts(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
    Query(filter): Query<AlertFilter>,
) -> Result<Json<Vec<AlertView>>, ApiError> {
    let auth = guard.auth;

    if let Some(status) = filter.status.as_deref() {
        if status != "open" && status != "resolved" {
            return Err(ApiError::BadRequest("invalid_status"));
        }
    }

    let limit = filter
        .limit
        .unwrap_or(DEFAULT_ALERT_LIMIT)
        .clamp(1, MAX_ALERT_LIMIT);
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<AlertRow> = sqlx::query_as(
        "select a.id, a.type as alert_type, a.rule_id,
                a.material_id, m.name as material_name,
                a.batch_id,
                a.product_id, p.name as product_name,
                a.product_batch_id,
                coalesce(b.qty_remaining, pb.qty_remaining) as qty_remaining,
                coalesce(b.expires_at, pb.expires_at) as expires_at,
                (coalesce(b.expires_at, pb.expires_at) - current_date) as days_left,
                lv.on_hand,
                case when a.type = 'low_stock' then m.min_stock end as min_stock,
                a.created_at, a.resolved_at, a.sent_at, a.delivery_status, a.attempts
         from alerts a
         left join materials m on m.id = a.material_id
         left join material_batches b on b.id = a.batch_id
         left join products p on p.id = a.product_id
         left join product_batches pb on pb.id = a.product_batch_id
         left join lateral (
             select coalesce(sum(qty_remaining), 0)::numeric(18,3) as on_hand
             from material_batches
             where tenant_id = m.tenant_id
               and material_id = m.id
               and qty_remaining > 0
               and (expires_at is null or expires_at >= current_date)
         ) lv on m.id is not null and a.type = 'low_stock'
         where a.tenant_id = $1
           and (
             $2::text is null
             or ($2 = 'open' and a.resolved_at is null)
             or ($2 = 'resolved' and a.resolved_at is not null)
           )
         order by a.created_at desc, a.id desc
         limit $3",
    )
    .bind(auth.tenant_id)
    .bind(filter.status)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(|row| AlertView {
                status: if row.resolved_at.is_none() {
                    "open".to_string()
                } else {
                    "resolved".to_string()
                },
                id: row.id,
                alert_type: row.alert_type,
                rule_id: row.rule_id,
                material_id: row.material_id,
                material_name: row.material_name,
                batch_id: row.batch_id,
                product_id: row.product_id,
                product_name: row.product_name,
                product_batch_id: row.product_batch_id,
                qty_remaining: row.qty_remaining,
                expires_at: row.expires_at,
                days_left: row.days_left,
                on_hand: row.on_hand,
                min_stock: row.min_stock,
                created_at: row.created_at,
                resolved_at: row.resolved_at,
                sent_at: row.sent_at,
                delivery_status: row.delivery_status,
                attempts: row.attempts,
            })
            .collect(),
    ))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/alerts", get(list_alerts))
        .route("/alert-rules", get(list_rules))
}
