//! Alerts: the tenant's rules, its alert history, and the one write.
//!
//! The daily job writes alerts; these routes are how a screen reads them back
//! and how an owner tunes when a rule fires. Everything is tenant-scoped, so
//! RLS applies to the reads and to the row the update locks.

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::auth::guard::{CanManageTenant, CanReadMaterials, Require};
use crate::{error::ApiError, AppState};

const DEFAULT_ALERT_LIMIT: i64 = 50;
const MAX_ALERT_LIMIT: i64 = 200;

/// Matches the `alert_rules.channel` check.
const CHANNELS: [&str; 2] = ["whatsapp", "email"];

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

/// The digest the next alert run would send, composed by the server so the
/// screen shows the text that actually goes out rather than a second template.
#[derive(Debug, Serialize)]
pub struct AlertPreviewView {
    /// Empty when nothing is waiting, so the screen can say so in its own words.
    pub subject: String,
    pub body: String,
    /// How many alerts the digest covers.
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct AlertFilter {
    /// `open`, `resolved`, or omitted for both.
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// Every field is optional, so a screen can send the one it changed. Omitted
/// fields keep their stored value.
#[derive(Debug, Deserialize)]
pub struct AlertRuleUpdate {
    /// Days before expiry. A low-stock rule carries none, so a value sent for
    /// one is rejected rather than silently dropped.
    pub days_before: Option<i32>,
    pub channel: Option<String>,
    pub is_active: Option<bool>,
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

/// Updates one rule's timing, channel, or on/off state.
///
/// Adding and removing rules is deliberately not here: the seeded set is what
/// a tenant starts from, and a rule that does not exist yet is a separate
/// decision with its own limits (one low-stock rule per tenant, one expiry
/// rule per lead time).
///
/// The row is locked and re-read in the same transaction, so two screens
/// editing the same rule cannot interleave, and the audit entry records the
/// values before and after.
pub async fn update_rule(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Path(rule_id): Path<Uuid>,
    Json(body): Json<AlertRuleUpdate>,
) -> Result<Json<AlertRuleView>, ApiError> {
    let auth = guard.auth;

    let channel = match body.channel.as_deref() {
        Some(value) if CHANNELS.contains(&value) => Some(value.to_string()),
        Some(_) => return Err(ApiError::BadRequest("invalid_channel")),
        None => None,
    };

    if body.days_before.is_some_and(|days| days < 0) {
        return Err(ApiError::BadRequest("invalid_days_before"));
    }

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let existing: Option<(String, Option<i32>, String, bool)> = sqlx::query_as(
        "select type, days_before, channel, is_active
         from alert_rules
         where id = $1 and tenant_id = $2
         for update",
    )
    .bind(rule_id)
    .bind(auth.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (rule_type, previous_days, previous_channel, previous_active) =
        existing.ok_or(ApiError::NotFound("rule_not_found"))?;

    // `alert_rules_shape` requires a low-stock rule to carry no lead time, so
    // letting this through would fail the check with a database error instead
    // of a message the screen can show.
    if rule_type == "low_stock" && body.days_before.is_some() {
        return Err(ApiError::BadRequest("invalid_days_before"));
    }

    let (rule_type, days_before, channel, is_active, created_at): (
        String,
        Option<i32>,
        String,
        bool,
        DateTime<Utc>,
    ) = sqlx::query_as(
        "update alert_rules
         set days_before = coalesce($2, days_before),
             channel = coalesce($3, channel),
             is_active = coalesce($4, is_active)
         where id = $1 and tenant_id = $5
         returning type, days_before, channel, is_active, created_at",
    )
    .bind(rule_id)
    .bind(body.days_before)
    .bind(channel)
    .bind(body.is_active)
    .bind(auth.tenant_id)
    .fetch_one(&mut *tx)
    .await
    // Two expiry rules cannot share a lead time; the screen needs a code, not
    // a database error.
    .map_err(|error| ApiError::from_write(error, "duplicate_rule_days"))?;

    let mut metadata = Map::new();

    if previous_days != days_before {
        metadata.insert(
            "days_before".to_string(),
            json!({ "from": previous_days, "to": days_before }),
        );
    }

    if previous_channel != channel {
        metadata.insert(
            "channel".to_string(),
            json!({ "from": previous_channel, "to": channel }),
        );
    }

    if previous_active != is_active {
        metadata.insert(
            "is_active".to_string(),
            json!({ "from": previous_active, "to": is_active }),
        );
    }

    // A request that changes nothing succeeds and simply has no history.
    if !metadata.is_empty() {
        nata_db::audit::record(
            &mut tx,
            auth.tenant_id,
            Some(auth.user_id),
            "alert_rules.updated",
            "alert_rule",
            Some(rule_id),
            Some(Value::Object(metadata)),
        )
        .await?;
    }

    tx.commit().await?;

    Ok(Json(AlertRuleView {
        id: rule_id,
        rule_type,
        days_before,
        channel,
        is_active,
        created_at,
    }))
}

/// Shows the message the next run would send for this tenant.
///
/// It selects the same pending set the sender does, so the preview is the
/// digest and not a sample. An empty set returns an empty message with
/// `count` 0 rather than a digest about nothing.
pub async fn preview(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<AlertPreviewView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let alerts = nata_db::alert_delivery::pending_for_tenant(&mut tx, auth.tenant_id).await?;

    tx.commit().await?;

    if alerts.is_empty() {
        return Ok(Json(AlertPreviewView {
            subject: String::new(),
            body: String::new(),
            count: 0,
        }));
    }

    let digest = super::message::Digest {
        tenant: &alerts[0].tenant_name,
        today: Utc::now().date_naive(),
        app_url: &state.app_url,
        alerts: &alerts,
    };

    Ok(Json(AlertPreviewView {
        subject: super::message::subject(&digest),
        body: super::message::compose(&digest),
        count: alerts.len() as i64,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/alerts", get(list_alerts))
        .route("/alerts/preview", get(preview))
        .route("/alert-rules", get(list_rules))
        .route("/alert-rules/{id}", patch(update_rule))
}
