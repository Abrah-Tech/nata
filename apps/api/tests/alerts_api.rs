mod common;

use axum::{http::StatusCode, Router};
use common::{call, date_from_now, signup, stock_in, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn material(app: &Router, token: &str, name: &str, min_stock: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;

    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg, "min_stock": min_stock })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn product(app: &Router, token: &str, name: &str, shelf_life_days: i32) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/products",
        Some(json!({
            "name": name,
            "sell_price": "15000",
            "shelf_life_days": shelf_life_days
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn create_recipe(app: &Router, token: &str, product_id: Uuid, material_id: Uuid) {
    let (status, recipe) = call(
        app,
        "POST",
        &format!("/products/{product_id}/recipes"),
        Some(json!({
            "yield_qty": "10",
            "items": [{ "material_id": material_id, "qty": "1" }]
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{recipe}");
}

async fn produce(app: &Router, token: &str, body: Value) {
    let (status, order) = call(app, "POST", "/production-orders", Some(body), Some(token)).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn rules_are_readable_and_tenant_scoped(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, rules) = call(&app, "GET", "/alert-rules", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{rules}");

    let rules = rules.as_array().expect("rules");
    assert_eq!(rules.len(), 3, "H-3, H-7, and low stock");
    assert_eq!(rules[0]["type"], "expiry");
    assert_eq!(rules[0]["days_before"], 3);
    assert_eq!(rules[0]["channel"], "whatsapp");
    assert_eq!(rules[0]["is_active"], true);
    assert_eq!(rules[1]["days_before"], 7);
    assert_eq!(rules[2]["type"], "low_stock");
    assert!(rules[2]["days_before"].is_null());

    let (status, body) = call(&app, "GET", "/alert-rules", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let (status, other_rules) = call(&app, "GET", "/alert-rules", None, Some(other_token)).await;
    assert_eq!(status, StatusCode::OK, "{other_rules}");
    assert_eq!(
        other_rules.as_array().expect("rules").len(),
        3,
        "a new tenant starts from the same defaults"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn history_lists_open_and_resolved_alerts(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu", "5").await;
    let (status, _) = stock_in(
        &app,
        token,
        json!({
            "material_id": flour,
            "qty_received": "2",
            "expires_at": date_from_now(2),
            "unit_cost": "10000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let run = nata_db::alerts::run(&pool).await.expect("run alerts");
    assert_eq!(run.created, 3, "H-7, H-3, and low stock");

    let (status, body) = call(&app, "GET", "/alerts", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let rows = body.as_array().expect("alerts");
    assert_eq!(rows.len(), 3);

    let expiry = rows
        .iter()
        .find(|row| row["type"] == "expiry")
        .expect("expiry");
    assert_eq!(expiry["status"], "open");
    assert_eq!(expiry["material_name"], "Tepung terigu");
    assert_eq!(expiry["material_id"], flour.to_string());
    assert!(expiry["batch_id"].is_string());
    assert!(expiry["product_id"].is_null());
    assert!(expiry["product_name"].is_null());
    assert!(expiry["product_batch_id"].is_null());
    assert_eq!(expiry["qty_remaining"], "2.000");
    assert_eq!(expiry["days_left"], 2);
    assert!(expiry["on_hand"].is_null(), "expiry alerts carry no level");
    assert!(expiry["min_stock"].is_null());
    assert_eq!(expiry["delivery_status"], "pending");
    assert_eq!(expiry["attempts"], 0);
    assert!(expiry["resolved_at"].is_null());

    let low_stock = rows
        .iter()
        .find(|row| row["type"] == "low_stock")
        .expect("low stock");
    assert_eq!(low_stock["material_id"], flour.to_string());
    assert!(low_stock["batch_id"].is_null());
    assert_eq!(low_stock["on_hand"], "2.000");
    assert_eq!(low_stock["min_stock"], "5.000");
    assert!(low_stock["qty_remaining"].is_null());
    assert!(low_stock["expires_at"].is_null());

    // Resolving one alert leaves it in the history and out of the open filter.
    sqlx::query("update alerts set resolved_at = now() where type = 'low_stock'")
        .execute(&pool)
        .await
        .expect("resolve");

    let (status, open) = call(&app, "GET", "/alerts?status=open", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{open}");
    let open = open.as_array().expect("open");
    assert_eq!(open.len(), 2);
    assert!(open.iter().all(|row| row["status"] == "open"));

    let (status, resolved) = call(&app, "GET", "/alerts?status=resolved", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    let resolved = resolved.as_array().expect("resolved");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0]["type"], "low_stock");
    assert!(resolved[0]["resolved_at"].is_string());

    let (status, body) = call(&app, "GET", "/alerts?status=everything", None, Some(token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = call(&app, "GET", "/alerts", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    // Another tenant sees none of this.
    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let (status, body) = call(&app, "GET", "/alerts", None, Some(other_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.as_array().expect("alerts").is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_finished_goods_expiry_is_listed_with_its_product(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", 1).await;
    let flour = material(&app, token, "Tepung terigu", "0").await;
    create_recipe(&app, token, cake, flour).await;
    let (status, _) = stock_in(
        &app,
        token,
        json!({
            "material_id": flour,
            "qty_received": "10",
            "expires_at": date_from_now(30),
            "unit_cost": "9000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Produced five days ago with a one-day shelf life: the batch is already
    // past its date, and an expired batch still needs action.
    produce(
        &app,
        token,
        json!({
            "product_id": cake,
            "qty": "10",
            "produced_on": date_from_now(-5)
        }),
    )
    .await;

    let run = nata_db::alerts::run(&pool).await.expect("run alerts");
    assert!(run.created >= 2, "both expiry rules fire for the batch");

    let (status, body) = call(&app, "GET", "/alerts", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let product_alert = body
        .as_array()
        .expect("alerts")
        .iter()
        .find(|row| row["product_id"] == cake.to_string())
        .expect("product alert");
    assert_eq!(product_alert["type"], "expiry");
    assert_eq!(product_alert["product_name"], "Bolu pandan");
    assert!(product_alert["product_batch_id"].is_string());
    assert!(product_alert["material_id"].is_null());
    assert!(product_alert["material_name"].is_null());
    assert!(product_alert["batch_id"].is_null());
    assert_eq!(product_alert["qty_remaining"], "10.000");
}
