//! Audit log reads.
//!
//! Handlers write entries through `nata_db::audit::record` inside the same
//! transaction as the action. This module only reads them: there is no write
//! route, so the log cannot be rewritten through the API.
//!
//! Reading is owner-only (`AuditRead`). The log reports on role changes,
//! billing, and settings, which is broader than what an admin can already see.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::guard::{CanReadAudit, Require};
use crate::{error::ApiError, AppState};

/// Matches the audit screen's first page.
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct AuditFilter {
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: Uuid,
    /// Null when the actor is not a member of the tenant: the operator who
    /// verifies a payment from the CLI has no `users` row.
    pub actor_user_id: Option<Uuid>,
    pub actor_name: Option<String>,
    /// Dotted action name, e.g. `member.role_changed`.
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_name: Option<String>,
    action: String,
    entity_type: String,
    entity_id: Option<Uuid>,
    metadata: Value,
    created_at: DateTime<Utc>,
}

pub async fn list_audit_logs(
    State(state): State<AppState>,
    guard: Require<CanReadAudit>,
    Query(filter): Query<AuditFilter>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    let auth = guard.auth;

    if let Some(limit) = filter.limit {
        if limit < 1 {
            return Err(ApiError::BadRequest("invalid_limit"));
        }
    }

    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<AuditRow> = sqlx::query_as(
        "select a.id, a.actor_user_id, u.name as actor_name,
                a.action, a.entity_type, a.entity_id, a.metadata, a.created_at
         from audit_logs a
         left join users u on u.id = a.actor_user_id
         where a.tenant_id = $1
         order by a.created_at desc, a.id desc
         limit $2",
    )
    .bind(auth.tenant_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(|row| AuditEntry {
                id: row.id,
                actor_user_id: row.actor_user_id,
                actor_name: row.actor_name,
                action: row.action,
                entity_type: row.entity_type,
                entity_id: row.entity_id,
                metadata: row.metadata,
                created_at: row.created_at,
            })
            .collect(),
    ))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/audit-logs", get(list_audit_logs))
}
