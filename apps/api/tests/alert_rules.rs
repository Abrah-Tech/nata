mod common;

use axum::{http::StatusCode, Router};
use common::{add_member, call, login, signup, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

/// Finds one seeded rule by type, narrowing by lead time when the type alone is
/// ambiguous. The seeded set holds two expiry rules and one low-stock rule.
async fn rule_id(app: &Router, token: &str, rule_type: &str, days_before: Option<i32>) -> Uuid {
    let (status, rules) = call(app, "GET", "/alert-rules", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{rules}");

    rules
        .as_array()
        .expect("rules")
        .iter()
        .find(|rule| {
            rule["type"] == rule_type
                && match days_before {
                    Some(days) => rule["days_before"] == days,
                    None => rule["days_before"].is_null(),
                }
        })
        .unwrap_or_else(|| panic!("no {rule_type} rule with {days_before:?} days in {rules:?}"))
        ["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

async fn audit_entries(app: &Router, token: &str) -> Vec<Value> {
    let (status, entries) = call(app, "GET", "/audit-logs", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{entries}");
    entries.as_array().expect("audit entries").clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_tenant_starts_with_the_default_alert_rules(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let rules: Vec<(String, Option<i32>, String)> = sqlx::query_as(
        "select type, days_before, channel
         from alert_rules
         where tenant_id = $1
         order by type, days_before nulls first",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .expect("rules");

    assert_eq!(
        rules,
        vec![
            ("expiry".to_string(), Some(3), "whatsapp".to_string()),
            ("expiry".to_string(), Some(7), "whatsapp".to_string()),
            ("low_stock".to_string(), None, "whatsapp".to_string()),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_tenant_gets_its_own_rules(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let mut tenants = Vec::new();
    for contact in ["first@example.test", "second@example.test"] {
        let owner = signup(&app, &otp, contact).await;
        let token = owner["access_token"].as_str().expect("token");
        tenants.push(tenant_of(&app, token).await);
    }

    for tenant_id in tenants {
        let (count,): (i64,) =
            sqlx::query_as("select count(*) from alert_rules where tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&pool)
                .await
                .expect("count");

        assert_eq!(count, 3);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_owner_can_retune_a_rule(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let rule = rule_id(&app, token, "expiry", Some(3)).await;
    let (status, changed) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "days_before": 5, "channel": "email", "is_active": false })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["id"], rule.to_string());
    assert_eq!(changed["type"], "expiry");
    assert_eq!(changed["days_before"], 5);
    assert_eq!(changed["channel"], "email");
    assert_eq!(changed["is_active"], false);

    // The response is not the only thing that changed; the row has to agree.
    let (days_before, channel, is_active): (Option<i32>, String, bool) =
        sqlx::query_as("select days_before, channel, is_active from alert_rules where id = $1")
            .bind(rule)
            .fetch_one(&pool)
            .await
            .expect("rule row");

    assert_eq!(days_before, Some(5));
    assert_eq!(channel, "email");
    assert!(!is_active);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_omitted_field_keeps_its_value(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let rule = rule_id(&app, token, "expiry", Some(7)).await;
    let (status, changed) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "is_active": false })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["days_before"], 7, "the screen sent only the flag");
    assert_eq!(changed["channel"], "whatsapp");
    assert_eq!(changed["is_active"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_owner_may_retune_a_rule(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

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

    let rule = rule_id(&app, token, "low_stock", None).await;

    // Tenant configuration stays with the owner, the same as `PUT /settings`:
    // an admin runs the team day to day, and alert rules reach every member.
    let admin = login(&app, &otp, "admin@example.test").await;
    let admin_token = admin["access_token"].as_str().expect("token");
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "channel": "email" })),
        Some(admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");

    let production = login(&app, &otp, "production@example.test").await;
    let production_token = production["access_token"].as_str().expect("token");
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "channel": "email" })),
        Some(production_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");

    let (status, _) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "channel": "email" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rule_rejects_values_it_cannot_store(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let expiry = rule_id(&app, token, "expiry", Some(7)).await;

    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{expiry}"),
        Some(json!({ "channel": "sms" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_channel");

    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{expiry}"),
        Some(json!({ "days_before": -1 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_days_before");

    // The shape check requires a low-stock rule to carry no lead time.
    let low_stock = rule_id(&app, token, "low_stock", None).await;
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{low_stock}"),
        Some(json!({ "days_before": 2 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_days_before");

    let (count,): (i64,) = sqlx::query_as(
        "select count(*) from alert_rules where tenant_id = (select tenant_id from alert_rules where id = $1)
         and channel <> 'whatsapp'",
    )
    .bind(expiry)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count, 0, "a rejected request must not write anything");
}

#[sqlx::test(migrations = "../../migrations")]
async fn another_tenants_rule_is_not_found(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let rule = rule_id(&app, first_token, "expiry", Some(3)).await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "channel": "email" })),
        Some(second_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "rule_not_found");
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_expiry_rules_cannot_share_a_lead_time(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let rule = rule_id(&app, token, "expiry", Some(3)).await;
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "days_before": 7 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "duplicate_rule_days");
}

#[sqlx::test(migrations = "../../migrations")]
async fn retuning_a_rule_is_recorded_in_the_audit_log(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let rule = rule_id(&app, token, "expiry", Some(3)).await;
    let (status, _) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "days_before": 5, "channel": "email" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let entries = audit_entries(&app, token).await;
    let entry = entries
        .iter()
        .find(|entry| entry["action"] == "alert_rules.updated")
        .unwrap_or_else(|| panic!("no alert_rules.updated entry in {entries:?}"));

    assert_eq!(entry["entity_type"], "alert_rule");
    assert_eq!(entry["entity_id"], rule.to_string());
    assert_eq!(entry["metadata"]["days_before"]["from"], 3);
    assert_eq!(entry["metadata"]["days_before"]["to"], 5);
    assert_eq!(entry["metadata"]["channel"]["from"], "whatsapp");
    assert_eq!(entry["metadata"]["channel"]["to"], "email");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_request_that_changes_nothing_writes_no_history(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let rule = rule_id(&app, token, "low_stock", None).await;
    let (status, changed) = call(
        &app,
        "PATCH",
        &format!("/alert-rules/{rule}"),
        Some(json!({ "channel": "whatsapp", "is_active": true })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");

    let entries = audit_entries(&app, token).await;
    assert!(
        !entries
            .iter()
            .any(|entry| entry["action"] == "alert_rules.updated"),
        "{entries:?}"
    );
}
