//! Delivery bookkeeping for alerts.
//!
//! The sender asks for pending work, composes one message per tenant, and
//! records the outcome here. A failed send stays visible and is picked up again
//! on the next run until it succeeds or runs out of attempts.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// How many times one alert may be attempted before it is left alone.
pub const MAX_ATTEMPTS: i32 = 3;

/// One alert waiting to be delivered, with everything the message needs.
#[derive(Debug, sqlx::FromRow)]
pub struct PendingAlert {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub alert_type: String,
    /// The material or product the alert is about.
    pub name: String,
    /// Null for finished goods, which carry no unit.
    pub unit_code: Option<String>,
    pub channel: String,
    pub owner_phone: Option<String>,
    pub owner_email: Option<String>,
    pub qty_remaining: Option<Decimal>,
    pub expires_at: Option<NaiveDate>,
    pub days_left: Option<i32>,
    pub on_hand: Option<Decimal>,
    pub min_stock: Decimal,
    pub attempts: i32,
}

impl PendingAlert {
    /// The owner contact for a channel, if the tenant has one. Owners are the
    /// only recipients the product knows about until tenant settings exist.
    pub fn contact(&self, channel: &str) -> Option<&str> {
        match channel {
            "email" => self.owner_email.as_deref(),
            "whatsapp" => self.owner_phone.as_deref(),
            _ => None,
        }
    }

    /// The contact for this alert's rule channel.
    pub fn recipient(&self) -> Option<&str> {
        self.contact(&self.channel)
    }
}

/// Open alerts that still need a message, oldest first.
///
/// Runs outside a tenant scope on purpose: the job role bypasses RLS and walks
/// every tenant in one pass.
pub async fn pending(pool: &PgPool) -> Result<Vec<PendingAlert>, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    pending_from(&mut connection, None).await
}

/// The same set for one tenant, read inside a tenant-scoped transaction.
///
/// The preview on the settings screen shows what the next run would send, so it
/// has to select exactly what `pending` selects. The scope comes from the
/// transaction as well as from the filter, which keeps a screen from ever
/// reading another tenant's digest.
pub async fn pending_for_tenant(
    connection: &mut PgConnection,
    tenant_id: Uuid,
) -> Result<Vec<PendingAlert>, sqlx::Error> {
    pending_from(connection, Some(tenant_id)).await
}

async fn pending_from(
    connection: &mut PgConnection,
    tenant_id: Option<Uuid>,
) -> Result<Vec<PendingAlert>, sqlx::Error> {
    // One row per alert, whichever ledger it belongs to. A material alert joins
    // the material side; a finished-goods alert joins the product side. The
    // unused side is null, so `coalesce` picks the name and the unit is null.
    sqlx::query_as::<_, PendingAlert>(
        "select a.id,
                a.tenant_id,
                t.name as tenant_name,
                a.type as alert_type,
                coalesce(m.name, p.name) as name,
                u.code as unit_code,
                r.channel,
                (select us.phone
                 from memberships mb
                 join users us on us.id = mb.user_id
                 where mb.tenant_id = a.tenant_id
                   and mb.role = 'owner'
                   and mb.status = 'active'
                   and us.phone is not null
                 order by mb.joined_at
                 limit 1) as owner_phone,
                (select us.email
                 from memberships mb
                 join users us on us.id = mb.user_id
                 where mb.tenant_id = a.tenant_id
                   and mb.role = 'owner'
                   and mb.status = 'active'
                   and us.email is not null
                 order by mb.joined_at
                 limit 1) as owner_email,
                coalesce(b.qty_remaining, pb.qty_remaining) as qty_remaining,
                coalesce(b.expires_at, pb.expires_at) as expires_at,
                (coalesce(b.expires_at, pb.expires_at) - current_date) as days_left,
                lv.on_hand,
                coalesce(m.min_stock, 0) as min_stock,
                a.attempts
         from alerts a
         join tenants t on t.id = a.tenant_id
         join alert_rules r on r.id = a.rule_id
         left join materials m on m.id = a.material_id
         left join units u on u.id = m.base_unit_id
         left join material_batches b on b.id = a.batch_id
         left join products p on p.id = a.product_id
         left join product_batches pb on pb.id = a.product_batch_id
         -- Usable stock only: expired batches cannot be used, so counting them
         -- would show a material as healthy while the alert says it is short.
         left join lateral (
             select coalesce(sum(qty_remaining), 0) as on_hand
             from material_batches
             where tenant_id = m.tenant_id
               and material_id = m.id
               and qty_remaining > 0
               and (expires_at is null or expires_at >= current_date)
         ) lv on m.id is not null
         where (a.material_id is not null or a.product_batch_id is not null)
           and a.resolved_at is null
           and a.delivery_status in ('pending', 'failed')
           and a.attempts < $1
           and ($2::uuid is null or a.tenant_id = $2)
         order by a.tenant_id, a.created_at, a.id",
    )
    .bind(MAX_ATTEMPTS)
    .bind(tenant_id)
    .fetch_all(connection)
    .await
}

pub enum Outcome<'a> {
    Sent,
    Failed(&'a str),
    Skipped(&'a str),
}

impl Outcome<'_> {
    fn status(&self) -> &'static str {
        match self {
            Outcome::Sent => "sent",
            Outcome::Failed(_) => "failed",
            Outcome::Skipped(_) => "skipped",
        }
    }

    fn error(&self) -> Option<&str> {
        match self {
            Outcome::Sent => None,
            Outcome::Failed(reason) | Outcome::Skipped(reason) => Some(reason),
        }
    }
}

/// Records one attempt for every alert in the group.
pub async fn record(
    pool: &PgPool,
    ids: &[Uuid],
    channel: &str,
    recipient: Option<&str>,
    outcome: Outcome<'_>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "update alerts
         set attempts = attempts + 1,
             delivery_status = $2,
             delivery_channel = $3,
             delivery_recipient = $4,
             last_error = $5,
             sent_at = case when $2 = 'sent' then now() else sent_at end
         where id = any($1)",
    )
    .bind(ids)
    .bind(outcome.status())
    .bind(channel)
    .bind(recipient)
    .bind(outcome.error())
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
