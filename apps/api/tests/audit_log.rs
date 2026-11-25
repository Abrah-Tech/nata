mod common;

use axum::{http::StatusCode, Router};
use common::{add_member, call, login, signup, tenant_of, test_app, user_of, CaptureOtp};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

/// Invitations need a free seat, and the trial allows one.
async fn signup_with_seats(app: &Router, otp: &CaptureOtp, pool: &PgPool, contact: &str) -> Value {
    let owner = signup(app, otp, contact).await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(app, token).await;

    sqlx::query("update tenants set plan = 'business' where id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("plan");

    owner
}

async fn audit(app: &Router, token: &str) -> Vec<Value> {
    let (status, body) = call(app, "GET", "/audit-logs", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array().expect("audit entries").clone()
}

fn entry_for<'a>(entries: &'a [Value], action: &str) -> &'a Value {
    entries
        .iter()
        .find(|entry| entry["action"] == action)
        .unwrap_or_else(|| panic!("no {action} entry in {entries:?}"))
}

#[sqlx::test(migrations = "../../migrations")]
async fn role_and_settings_changes_are_recorded(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let admin = add_member(
        &pool,
        tenant_id,
        "admin@example.test",
        "Admin User",
        "admin",
    )
    .await;

    let (status, changed) = call(
        &app,
        "PATCH",
        &format!("/members/{admin}/role"),
        Some(json!({ "role": "production" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");

    let (status, settings) = call(
        &app,
        "PUT",
        "/settings",
        Some(json!({
            "labor_rate_per_hour": "15000",
            "overhead_rate_per_hour": "25000"
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{settings}");

    let entries = audit(&app, token).await;

    let role = entry_for(&entries, "member.role_changed");
    assert_eq!(role["entity_type"], "member");
    assert_eq!(role["entity_id"], admin.to_string());
    assert_eq!(role["actor_name"], "Test Owner");
    assert_eq!(role["metadata"]["from"], "admin");
    assert_eq!(role["metadata"]["to"], "production");

    let settings = entry_for(&entries, "settings.updated");
    assert_eq!(settings["entity_type"], "tenant");
    assert_eq!(settings["entity_id"], tenant_id.to_string());
    let from: Decimal = settings["metadata"]["labor_rate_per_hour"]["from"]
        .as_str()
        .expect("from")
        .parse()
        .expect("decimal");
    assert_eq!(from, Decimal::ZERO, "the tenant default is zero");
    assert_eq!(
        settings["metadata"]["labor_rate_per_hour"]["to"],
        "15000.00"
    );

    let overhead_from: Decimal = settings["metadata"]["overhead_rate_per_hour"]["from"]
        .as_str()
        .expect("from")
        .parse()
        .expect("decimal");
    assert_eq!(overhead_from, Decimal::ZERO, "the tenant default is zero");
    assert_eq!(
        settings["metadata"]["overhead_rate_per_hour"]["to"],
        "25000.00"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_owner_may_read_the_audit_log(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;

    add_member(
        &pool,
        tenant_id,
        "admin@example.test",
        "Admin User",
        "admin",
    )
    .await;
    add_member(
        &pool,
        tenant_id,
        "production@example.test",
        "Production User",
        "production",
    )
    .await;

    // An admin manages the team day to day but cannot read a feed that also
    // reports on billing and settings.
    let admin = login(&app, &otp, "admin@example.test").await;
    let admin_token = admin["access_token"].as_str().expect("token");
    let (status, body) = call(&app, "GET", "/audit-logs", None, Some(admin_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");

    let production = login(&app, &otp, "production@example.test").await;
    let production_token = production["access_token"].as_str().expect("token");
    let (status, body) = call(&app, "GET", "/audit-logs", None, Some(production_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let (status, _) = call(&app, "GET", "/audit-logs", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_logs_are_scoped_to_the_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;
    let admin = add_member(
        &pool,
        tenant_id,
        "admin@example.test",
        "Admin User",
        "admin",
    )
    .await;

    let (status, _) = call(
        &app,
        "PATCH",
        &format!("/members/{admin}/role"),
        Some(json!({ "role": "production" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");
    let entries = audit(&app, second_token).await;
    assert!(entries.is_empty(), "{entries:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_accepted_invitation_is_recorded(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup_with_seats(&app, &otp, &pool, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let owner_user = user_of(&app, owner_token).await;

    let (status, created) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "baker@example.test", "role": "production" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let invite_token = created["token"].as_str().expect("token");

    let pending = login(&app, &otp, "baker@example.test").await;
    let selection = pending["access_token"].as_str().expect("token");
    let (status, accepted) = call(
        &app,
        "POST",
        "/auth/accept-invitation",
        Some(json!({ "token": invite_token })),
        Some(selection),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let entries = audit(&app, owner_token).await;

    let invited = entry_for(&entries, "member.invited");
    assert_eq!(invited["entity_type"], "invitation");
    assert_eq!(invited["actor_user_id"], owner_user.to_string());
    assert_eq!(invited["metadata"]["role"], "production");

    let joined = entry_for(&entries, "member.joined");
    assert_eq!(joined["entity_type"], "member");
    assert_ne!(
        joined["actor_user_id"],
        owner_user.to_string(),
        "the invitee joins, the owner only sent the invitation"
    );
    assert_eq!(joined["metadata"]["role"], "production");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_payment_records_what_it_was(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, payment) = call(
        &app,
        "POST",
        "/billing/payments",
        Some(json!({ "plan": "pro", "months": 1, "method": "transfer" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{payment}");
    let id = payment["id"].as_str().expect("id");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/billing/payments/{id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let entries = audit(&app, token).await;
    let deleted = entry_for(&entries, "billing.payment_deleted");
    assert_eq!(deleted["entity_id"], id);
    assert_eq!(deleted["metadata"]["plan"], "pro");
    assert_eq!(
        deleted["metadata"]["amount"], payment["amount"],
        "the row is gone, so the audit entry is the only trace of the amount"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_operator_who_verifies_a_payment_is_recorded(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, payment) = call(
        &app,
        "POST",
        "/billing/payments",
        Some(json!({ "plan": "pro", "months": 1, "method": "transfer" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{payment}");
    let id: Uuid = payment["id"].as_str().expect("id").parse().expect("uuid");

    // The CLI path: the operator has no tenant and no `users` row, so the
    // actor is null and the label lives in the metadata.
    nata_api::billing::verify_payment(&pool, id, "operator")
        .await
        .expect("verify");

    let entries = audit(&app, token).await;
    let verified = entry_for(&entries, "billing.payment_verified");
    assert_eq!(verified["entity_id"], id.to_string());
    assert!(verified["actor_user_id"].is_null());
    assert_eq!(verified["metadata"]["by"], "operator");
    assert_eq!(verified["metadata"]["plan"], "pro");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_limit_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = call(&app, "GET", "/audit-logs?limit=0", None, Some(token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_limit");
}
