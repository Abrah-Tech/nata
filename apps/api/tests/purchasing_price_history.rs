mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, stock_in, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn supplier(app: &Router, token: &str, name: &str) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/suppliers",
        Some(json!({ "name": name, "lead_time_days": 2 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn material(app: &Router, token: &str, name: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;
    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn buy(
    app: &Router,
    token: &str,
    material_id: Uuid,
    supplier_id: Option<Uuid>,
    qty: &str,
    unit_cost: &str,
    received_at: &str,
) {
    let (status, body) = stock_in(
        app,
        token,
        json!({
            "material_id": material_id,
            "supplier_id": supplier_id,
            "received_at": received_at,
            "qty_received": qty,
            "unit_cost": unit_cost,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn history(app: &Router, token: &str, material_id: Uuid) -> (StatusCode, Value) {
    call(
        app,
        "GET",
        &format!("/purchasing/price-history?material_id={material_id}"),
        None,
        Some(token),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipts_are_grouped_by_supplier_newest_first(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let pasar = supplier(&app, token, "Pasar Induk").await;
    let toko = supplier(&app, token, "Toko Bangunan").await;
    let flour = material(&app, token, "Tepung terigu").await;

    buy(&app, token, flour, Some(toko), "50", "11500", "2026-10-28").await;
    buy(&app, token, flour, Some(pasar), "25", "12000", "2026-11-12").await;
    buy(&app, token, flour, Some(pasar), "25", "11500", "2026-11-05").await;

    let (status, body) = history(&app, token, flour).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["material_name"], "Tepung terigu");
    assert_eq!(body["unit_code"], "kg");

    let suppliers = body["suppliers"].as_array().expect("suppliers");
    assert_eq!(suppliers.len(), 2, "{}", body);

    // Pasar Induk received the newest receipt, so it leads.
    assert_eq!(suppliers[0]["supplier_name"], "Pasar Induk");
    assert_eq!(suppliers[0]["last_price"], "12000.0000");
    assert_eq!(suppliers[0]["lowest_price"], "11500.0000");
    assert_eq!(suppliers[0]["highest_price"], "12000.0000");
    assert_eq!(suppliers[0]["total_qty"], "50.000");

    let purchases = suppliers[0]["purchases"].as_array().expect("purchases");
    assert_eq!(purchases.len(), 2);
    assert!(
        purchases[0]["id"].as_str().is_some_and(|id| !id.is_empty()),
        "the receipt id keys the list"
    );
    assert_eq!(purchases[0]["received_at"], "2026-11-12");
    assert_eq!(purchases[0]["unit_cost"], "12000.0000");
    assert_eq!(
        purchases[0]["total"], "300000.00",
        "the total is computed with Decimal, not floating point"
    );
    assert_eq!(purchases[1]["received_at"], "2026-11-05");
    assert_eq!(purchases[1]["total"], "287500.00");

    assert_eq!(suppliers[1]["supplier_name"], "Toko Bangunan");
    assert_eq!(suppliers[1]["last_price"], "11500.0000");
    assert_eq!(suppliers[1]["total_qty"], "50.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_receipt_without_a_supplier_still_has_a_price(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    buy(&app, token, flour, None, "10", "9000", "2026-11-10").await;

    let (status, body) = history(&app, token, flour).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let suppliers = body["suppliers"].as_array().expect("suppliers");
    assert_eq!(suppliers.len(), 1);
    assert_eq!(suppliers[0]["supplier_id"], Value::Null);
    assert_eq!(suppliers[0]["supplier_name"], Value::Null);
    assert_eq!(suppliers[0]["last_price"], "9000.0000");
    assert_eq!(suppliers[0]["total_qty"], "10.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_ingredient_that_was_never_bought_has_no_history(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let sugar = material(&app, token, "Gula pasir").await;

    let (status, body) = history(&app, token, sugar).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["suppliers"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn another_tenants_ingredient_is_not_reachable(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    buy(&app, token, flour, None, "10", "9000", "2026-11-10").await;

    let theirs = material(&app, other_token, "Cabai").await;

    let (status, body) = history(&app, other_token, flour).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let (status, _) = history(&app, "", flour).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A well-formed id that belongs to nobody is the same answer, so the
    // endpoint cannot be used to probe which ids exist.
    let (status, _) = history(&app, token, Uuid::new_v4()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The tenant's own ingredient still reads after the refusals.
    let (status, body) = history(&app, token, flour).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["suppliers"].as_array().expect("suppliers").len(), 1);

    let (status, body) = history(&app, other_token, theirs).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["suppliers"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_malformed_material_id_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, _) = call(
        &app,
        "GET",
        "/purchasing/price-history?material_id=not-a-uuid",
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
