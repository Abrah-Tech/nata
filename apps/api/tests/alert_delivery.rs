mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{http::StatusCode, Router};
use chrono::{DateTime, NaiveDate, Utc};
use common::{call, login, test_app, CaptureOtp};
use nata_api::alerts::message::{compose, subject, Digest};
use nata_api::alerts::send::{send_pending, SendReport};
use nata_api::notify::{Message, Router as NotifyRouter, Transport, TransportError};
use nata_db::alert_delivery::{self, MAX_ATTEMPTS};
use nata_db::alerts::{run_on, seed_default_rules};
use sqlx::types::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

#[derive(Clone, Debug)]
struct Sent {
    to: String,
    subject: String,
    text: String,
}

#[derive(Default)]
struct Recorder {
    sent: Mutex<Vec<Sent>>,
    failure: Mutex<Option<String>>,
}

impl Recorder {
    fn failing(message: &str) -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            failure: Mutex::new(Some(message.to_string())),
        }
    }

    fn heal(&self) {
        *self.failure.lock().expect("failure") = None;
    }

    fn messages(&self) -> Vec<Sent> {
        self.sent.lock().expect("sent").clone()
    }
}

#[async_trait]
impl Transport for Recorder {
    fn name(&self) -> &'static str {
        "recorder"
    }

    async fn send(&self, message: &Message<'_>) -> Result<(), TransportError> {
        if let Some(reason) = self.failure.lock().expect("failure").clone() {
            return Err(TransportError::permanent(reason));
        }

        self.sent.lock().expect("sent").push(Sent {
            to: message.to.to_string(),
            subject: message.subject.to_string(),
            text: message.text.to_string(),
        });

        Ok(())
    }
}

fn whatsapp_router(recorder: &Arc<Recorder>) -> NotifyRouter {
    NotifyRouter::new(None, Some(recorder.clone()), None)
}

fn email_router(recorder: &Arc<Recorder>) -> NotifyRouter {
    NotifyRouter::new(Some(recorder.clone()), None, None)
}

async fn tenant_with_defaults(pool: &PgPool, name: &str) -> Uuid {
    let tenant_id = Uuid::new_v4();

    sqlx::query("insert into tenants (id, name) values ($1, $2)")
        .bind(tenant_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("tenant");

    let mut tx = pool.begin().await.expect("transaction");
    seed_default_rules(&mut tx, tenant_id)
        .await
        .expect("default rules");
    tx.commit().await.expect("commit");

    tenant_id
}

async fn add_member(
    pool: &PgPool,
    tenant_id: Uuid,
    role: &str,
    phone: Option<&str>,
    email: Option<&str>,
) -> Uuid {
    let user_id = Uuid::new_v4();

    sqlx::query("insert into users (id, name, phone, email) values ($1, 'Member', $2, $3)")
        .bind(user_id)
        .bind(phone)
        .bind(email)
        .execute(pool)
        .await
        .expect("user");

    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("membership");

    user_id
}

async fn add_owner(
    pool: &PgPool,
    tenant_id: Uuid,
    phone: Option<&str>,
    email: Option<&str>,
) -> Uuid {
    add_member(pool, tenant_id, "owner", phone, email).await
}

async fn add_material(pool: &PgPool, tenant_id: Uuid, name: &str, min_stock: &str) -> Uuid {
    let (unit_id,): (Uuid,) = sqlx::query_as("select id from units where code = 'kg'")
        .fetch_one(pool)
        .await
        .expect("unit");

    let id = Uuid::new_v4();
    let min_stock: Decimal = min_stock.parse().expect("decimal");

    sqlx::query(
        "insert into materials (id, tenant_id, name, base_unit_id, min_stock)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .bind(unit_id)
    .bind(min_stock)
    .execute(pool)
    .await
    .expect("material");

    id
}

async fn add_batch(
    pool: &PgPool,
    tenant_id: Uuid,
    material_id: Uuid,
    qty: &str,
    expires_at: Option<NaiveDate>,
) -> Uuid {
    let id = Uuid::new_v4();
    let qty: Decimal = qty.parse().expect("decimal");

    sqlx::query(
        "insert into material_batches
             (id, tenant_id, material_id, received_at, expires_at, qty_received, qty_remaining, unit_cost)
         values ($1, $2, $3, $4, $5, $6, $6, 1000)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(material_id)
    .bind(date(2026, 1, 1))
    .bind(expires_at)
    .bind(qty)
    .execute(pool)
    .await
    .expect("batch");

    id
}

async fn add_product(pool: &PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query(
        "insert into products (id, tenant_id, name, sell_price) values ($1, $2, $3, 15000)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("product");

    id
}

async fn add_product_batch(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
    qty: &str,
    expires_at: Option<NaiveDate>,
) -> Uuid {
    let id = Uuid::new_v4();
    let qty: Decimal = qty.parse().expect("decimal");

    sqlx::query(
        "insert into product_batches
             (id, tenant_id, product_id, produced_on, expires_at,
              qty_received, qty_remaining, unit_cost)
         values ($1, $2, $3, $4, $5, $6, $6, 12000)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(product_id)
    .bind(date(2026, 1, 1))
    .bind(expires_at)
    .bind(qty)
    .execute(pool)
    .await
    .expect("product batch");

    id
}

#[derive(Debug, sqlx::FromRow)]
struct DeliveryRow {
    status: String,
    attempts: i32,
    sent_at: Option<DateTime<Utc>>,
    channel: Option<String>,
    recipient: Option<String>,
    last_error: Option<String>,
}

async fn deliveries(pool: &PgPool, tenant_id: Uuid) -> Vec<DeliveryRow> {
    sqlx::query_as(
        "select delivery_status as status,
                attempts,
                sent_at,
                delivery_channel as channel,
                delivery_recipient as recipient,
                last_error
         from alerts
         where tenant_id = $1
         order by created_at, id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .expect("deliveries")
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_digest_carries_every_alert_and_all_are_marked_sent(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, Some("+6281234567890"), None).await;
    let material_id = add_material(&pool, tenant_id, "Tepung terigu", "5").await;
    add_batch(&pool, tenant_id, material_id, "2", Some(date(2026, 11, 12))).await;

    let run = run_on(&pool, today).await.expect("run alerts");
    assert_eq!(run.created, 3, "two expiry rules and one low-stock rule");

    let recorder = Arc::new(Recorder::default());
    let report = send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("send");

    assert_eq!(
        report,
        SendReport {
            sent: 3,
            failed: 0,
            skipped: 0
        }
    );

    let messages = recorder.messages();
    assert_eq!(messages.len(), 1, "one message per tenant, not per alert");
    assert_eq!(messages[0].to, "+6281234567890");
    assert_eq!(
        messages[0].subject, "Nata · 2 peringatan stok",
        "the H-7 and H-3 rules describe the same batch"
    );
    assert!(messages[0].text.contains("Dapur Uji"));
    assert!(messages[0].text.contains("Kedaluwarsa"));
    assert!(messages[0].text.contains("Tepung terigu 2 kg"));
    assert!(messages[0].text.contains("Stok minimum"));
    assert!(messages[0].text.contains("minimum 5 kg"));
    assert!(messages[0]
        .text
        .contains("Buka stok: https://nata.test/stock"));

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.status == "sent"));
    assert!(rows.iter().all(|row| row.attempts == 1));
    assert!(rows.iter().all(|row| row.sent_at.is_some()));
    assert!(rows
        .iter()
        .all(|row| row.channel.as_deref() == Some("whatsapp")));
    assert!(rows
        .iter()
        .all(|row| row.recipient.as_deref() == Some("+6281234567890")));
    assert!(rows.iter().all(|row| row.last_error.is_none()));

    assert!(
        alert_delivery::pending(&pool)
            .await
            .expect("pending")
            .is_empty(),
        "a sent alert is never queued again"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_send_stays_pending_and_is_retried(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, Some("+6281234567890"), None).await;
    let material_id = add_material(&pool, tenant_id, "Gula", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    run_on(&pool, today).await.expect("run alerts");

    let recorder = Arc::new(Recorder::failing("provider down"));
    let failed = send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("send");

    assert_eq!(
        failed,
        SendReport {
            sent: 0,
            failed: 1,
            skipped: 0
        }
    );
    assert!(recorder.messages().is_empty());

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(rows[0].status, "failed");
    assert_eq!(rows[0].attempts, 1);
    assert!(rows[0].sent_at.is_none());
    assert_eq!(rows[0].last_error.as_deref(), Some("provider down"));

    let queued = alert_delivery::pending(&pool).await.expect("pending");
    assert_eq!(queued.len(), 1, "a failed alert is offered again");
    assert_eq!(queued[0].attempts, 1);

    recorder.heal();
    let retried = send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("retry");

    assert_eq!(retried.sent, 1);
    assert_eq!(recorder.messages().len(), 1);

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(rows[0].status, "sent");
    assert_eq!(rows[0].attempts, 2);
    assert!(rows[0].sent_at.is_some());
    assert!(rows[0].last_error.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_email_only_owner_gets_the_digest_by_email(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, None, Some("owner@example.test")).await;
    let material_id = add_material(&pool, tenant_id, "Beras", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    run_on(&pool, today).await.expect("run alerts");

    let email = Arc::new(Recorder::default());
    let phone = Arc::new(Recorder::default());
    let router = NotifyRouter::new(Some(email.clone()), Some(phone.clone()), None);

    let report = send_pending(&pool, &router, "https://nata.test/", today)
        .await
        .expect("send");

    assert_eq!(report.sent, 1);
    assert!(
        phone.messages().is_empty(),
        "the rule prefers WhatsApp, but there is no number"
    );
    assert_eq!(email.messages().len(), 1);
    assert_eq!(email.messages()[0].to, "owner@example.test");

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(
        rows[0].channel.as_deref(),
        Some("email"),
        "the recorded channel is the one actually used"
    );
    assert_eq!(rows[0].recipient.as_deref(), Some("owner@example.test"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_phone_only_owner_gets_an_email_rule_by_whatsapp(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, Some("+6281234567890"), None).await;
    let material_id = add_material(&pool, tenant_id, "Beras", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    sqlx::query("update alert_rules set channel = 'email' where tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("switch channel");

    run_on(&pool, today).await.expect("run alerts");

    let email = Arc::new(Recorder::default());
    let phone = Arc::new(Recorder::default());
    let router = NotifyRouter::new(Some(email.clone()), Some(phone.clone()), None);

    let report = send_pending(&pool, &router, "https://nata.test/", today)
        .await
        .expect("send");

    assert_eq!(report.sent, 1);
    assert!(email.messages().is_empty());
    assert_eq!(phone.messages().len(), 1);
    assert_eq!(phone.messages()[0].to, "+6281234567890");

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(rows[0].channel.as_deref(), Some("whatsapp"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_tenant_without_an_owner_is_skipped(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_member(&pool, tenant_id, "admin", Some("+6281234567890"), None).await;
    let material_id = add_material(&pool, tenant_id, "Beras", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    run_on(&pool, today).await.expect("run alerts");

    let recorder = Arc::new(Recorder::default());
    let report = send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("send");

    assert_eq!(
        report,
        SendReport {
            sent: 0,
            failed: 0,
            skipped: 1
        }
    );
    assert!(recorder.messages().is_empty());

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(rows[0].status, "skipped");
    assert_eq!(rows[0].attempts, 1);
    assert_eq!(rows[0].last_error.as_deref(), Some("no_recipient"));
    assert!(
        alert_delivery::pending(&pool)
            .await
            .expect("pending")
            .is_empty(),
        "an alert that cannot be delivered is not retried forever"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_alert_that_ran_out_of_attempts_is_not_offered_again(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, Some("+6281234567890"), None).await;
    let material_id = add_material(&pool, tenant_id, "Cabai", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    run_on(&pool, today).await.expect("run alerts");

    sqlx::query(
        "update alerts
         set delivery_status = 'failed', attempts = $2, last_error = 'provider down'
         where tenant_id = $1",
    )
    .bind(tenant_id)
    .bind(MAX_ATTEMPTS)
    .execute(&pool)
    .await
    .expect("exhaust attempts");

    assert!(alert_delivery::pending(&pool)
        .await
        .expect("pending")
        .is_empty());

    let recorder = Arc::new(Recorder::default());
    let report = send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("send");

    assert_eq!(report, SendReport::default());
    assert!(recorder.messages().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_email_rule_uses_the_email_transport_and_recipient(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(
        &pool,
        tenant_id,
        Some("+6281234567890"),
        Some("owner@example.test"),
    )
    .await;
    let material_id = add_material(&pool, tenant_id, "Mentega", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    sqlx::query("update alert_rules set channel = 'email' where tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("switch channel");

    run_on(&pool, today).await.expect("run alerts");

    let recorder = Arc::new(Recorder::default());
    let report = send_pending(&pool, &email_router(&recorder), "https://nata.test/", today)
        .await
        .expect("send");

    assert_eq!(report.sent, 1);
    assert_eq!(recorder.messages().len(), 1);
    assert_eq!(recorder.messages()[0].to, "owner@example.test");

    let rows = deliveries(&pool, tenant_id).await;
    assert_eq!(rows[0].channel.as_deref(), Some("email"));
    assert_eq!(rows[0].recipient.as_deref(), Some("owner@example.test"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_batch_expiry_is_warned_once_and_delivered(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, Some("+6281234567890"), None).await;
    let cake = add_product(&pool, tenant_id, "Bolu pandan").await;
    add_product_batch(&pool, tenant_id, cake, "3", Some(date(2026, 11, 11))).await;

    let run = run_on(&pool, today).await.expect("run alerts");
    assert_eq!(
        run.created, 2,
        "the H-7 and H-3 rules both cover two days out"
    );

    let recorder = Arc::new(Recorder::default());
    let report = send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("send");

    assert_eq!(
        report.sent, 2,
        "both rules are recorded, one digest goes out"
    );
    let messages = recorder.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].to, "+6281234567890");
    assert!(messages[0].text.contains("Kedaluwarsa"));
    // The countdown comes from `current_date` at send time, so only the name,
    // quantity, and expiry date are pinned here.
    assert!(messages[0].text.contains("Bolu pandan 3 · 11 Nov ("));
    assert!(
        messages[0].text.matches("Bolu pandan").count() == 1,
        "two rules describing one batch still read as one line"
    );
    assert!(
        !messages[0].text.contains("kg"),
        "finished goods carry no unit"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_depleted_product_batch_stops_warning(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(&pool, tenant_id, Some("+6281234567890"), None).await;
    let cake = add_product(&pool, tenant_id, "Bolu pandan").await;
    let batch = add_product_batch(&pool, tenant_id, cake, "3", Some(date(2026, 11, 11))).await;

    let first = run_on(&pool, today).await.expect("first run");
    assert_eq!(first.created, 2);

    // Selling or writing off the whole batch makes the warning untrue.
    sqlx::query("update product_batches set qty_remaining = 0 where id = $1")
        .bind(batch)
        .execute(&pool)
        .await
        .expect("deplete");

    let second = run_on(&pool, today).await.expect("second run");
    assert_eq!(second.created, 0);
    assert_eq!(second.resolved, 2);

    assert!(alert_delivery::pending(&pool)
        .await
        .expect("pending")
        .is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_alerts_stay_inside_the_tenant(pool: PgPool) {
    let today = date(2026, 11, 9);
    let first = tenant_with_defaults(&pool, "Dapur Satu").await;
    add_owner(&pool, first, Some("+6281234567890"), None).await;
    let cake = add_product(&pool, first, "Bolu pandan").await;
    add_product_batch(&pool, first, cake, "3", Some(date(2026, 11, 11))).await;

    let second = tenant_with_defaults(&pool, "Dapur Dua").await;
    add_owner(&pool, second, Some("+6281234567891"), None).await;

    let run = run_on(&pool, today).await.expect("run alerts");
    assert_eq!(
        run.created, 2,
        "only the first tenant has an expiring batch"
    );

    let recorder = Arc::new(Recorder::default());
    send_pending(
        &pool,
        &whatsapp_router(&recorder),
        "https://nata.test/",
        today,
    )
    .await
    .expect("send");

    let messages = recorder.messages();
    assert_eq!(
        messages.len(),
        1,
        "the second tenant has nothing to warn about"
    );
    assert_eq!(messages[0].to, "+6281234567890");
    assert!(messages[0].text.contains("Dapur Satu"));
    assert!(messages[0].text.contains("Bolu pandan"));
}

/// Signs in the owner of a tenant built straight into the database, so a test
/// can reach a fixture without going through signup.
async fn owner_token(app: &Router, otp: &CaptureOtp, email: &str) -> String {
    let owner = login(app, otp, email).await;

    owner["access_token"].as_str().expect("token").to_string()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_preview_is_exactly_the_digest_the_sender_composes(pool: PgPool) {
    let today = date(2026, 11, 9);
    let tenant_id = tenant_with_defaults(&pool, "Dapur Uji").await;
    add_owner(
        &pool,
        tenant_id,
        Some("+6281234567890"),
        Some("owner@example.test"),
    )
    .await;
    let material_id = add_material(&pool, tenant_id, "Tepung terigu", "5").await;
    add_batch(&pool, tenant_id, material_id, "2", Some(date(2026, 11, 12))).await;

    run_on(&pool, today).await.expect("run alerts");

    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let token = owner_token(&app, &otp, "owner@example.test").await;

    let (status, preview) = call(&app, "GET", "/alerts/preview", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{preview}");

    let pending = alert_delivery::pending(&pool).await.expect("pending");
    // The alerts were created at a fixed date so the fixture does not drift with
    // the calendar, but the message always carries the date it is composed on.
    let digest = Digest {
        tenant: &pending[0].tenant_name,
        today: Utc::now().date_naive(),
        app_url: "https://nata.test",
        alerts: &pending,
    };

    assert_eq!(preview["count"].as_i64(), Some(pending.len() as i64));
    assert_eq!(
        preview["subject"].as_str().expect("subject"),
        subject(&digest)
    );
    assert_eq!(preview["body"].as_str().expect("body"), compose(&digest));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_tenant_with_nothing_pending_previews_an_empty_message(pool: PgPool) {
    let tenant_id = tenant_with_defaults(&pool, "Dapur Sepi").await;
    add_owner(
        &pool,
        tenant_id,
        Some("+6281234567890"),
        Some("owner@example.test"),
    )
    .await;

    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let token = owner_token(&app, &otp, "owner@example.test").await;

    let (status, preview) = call(&app, "GET", "/alerts/preview", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["count"], 0);
    assert_eq!(preview["subject"], "");
    assert_eq!(preview["body"], "");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_preview_carries_only_the_signed_in_tenant(pool: PgPool) {
    let today = date(2026, 11, 9);
    let first = tenant_with_defaults(&pool, "Dapur Satu").await;
    add_owner(
        &pool,
        first,
        Some("+6281234567890"),
        Some("satu@example.test"),
    )
    .await;
    let material_id = add_material(&pool, first, "Tepung terigu", "5").await;
    add_batch(&pool, first, material_id, "2", Some(date(2026, 11, 12))).await;

    let second = tenant_with_defaults(&pool, "Dapur Dua").await;
    add_owner(
        &pool,
        second,
        Some("+6281234567891"),
        Some("dua@example.test"),
    )
    .await;

    run_on(&pool, today).await.expect("run alerts");

    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let token = owner_token(&app, &otp, "dua@example.test").await;

    let (status, preview) = call(&app, "GET", "/alerts/preview", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["count"], 0, "the second tenant has nothing pending");
    assert_eq!(preview["body"], "");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_preview_requires_a_token(pool: PgPool) {
    let app = test_app(pool.clone(), CaptureOtp::default());

    let (status, _) = call(&app, "GET", "/alerts/preview", None, None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
