//! Append-only audit trail for sensitive actions.
//!
//! The log answers "who changed this, and when" for the actions that have no
//! other history: role and membership changes, invitation revocation, payment
//! deletion and verification, and tenant settings. Domain documents keep their
//! own author (`sales`, `production_orders`, `recipes`) or are already a signed
//! movement (`stock_movements`), so they are not repeated here.
//!
//! Writes go through the caller's transaction, so an entry and the change it
//! describes commit together or not at all. A failure here fails the action: an
//! audit log that silently skips entries is worse than none.

use serde_json::{Map, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Records one action.
///
/// `metadata` carries the action-specific detail: before and after values, the
/// operator label, the amount. Pass `None` for an empty object; a null is never
/// stored, so the reader always sees one shape.
pub async fn record(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    actor_user_id: Option<Uuid>,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    metadata: Option<Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into audit_logs
            (tenant_id, actor_user_id, action, entity_type, entity_id, metadata)
         values ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(actor_user_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(metadata.unwrap_or_else(|| Value::Object(Map::new())))
    .execute(&mut **tx)
    .await?;

    Ok(())
}
