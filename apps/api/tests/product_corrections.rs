mod common;

use axum::{http::StatusCode, Router};
use chrono::NaiveDate;
use common::{add_member, call, login, signup, test_app, CaptureOtp};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn product(app: &Router, token: &str, name: &str) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/products",
        Some(json!({ "name": name, "sell_price": "15000" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn correct(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/stock/product-corrections",
        Some(body),
        Some(token),
    )
    .await
}

async fn list_batches(app: &Router, token: &str, product_id: Uuid) -> Value {
    let (status, body) = call(
        app,
        "GET",
        &format!("/stock/product-batches?product_id={product_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Inserts a finished-goods batch directly and keeps the scalar cache in step,
/// the way a production run does.
async fn add_batch(
    pool: &PgPool,
    product_id: Uuid,
    produced_on: &str,
    expires_at: Option<&str>,
    qty: &str,
    unit_cost: Option<&str>,
) -> Uuid {
    let tenant_id: Uuid = sqlx::query_scalar("select tenant_id from products where id = $1")
        .bind(product_id)
        .fetch_one(pool)
        .await
        .expect("tenant");

    let produced_on = NaiveDate::parse_from_str(produced_on, "%Y-%m-%d").expect("date");
    let expires_at =
        expires_at.map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("date"));
    let qty = qty.parse::<Decimal>().expect("qty");
    let unit_cost = unit_cost.map(|value| value.parse::<Decimal>().expect("cost"));

    let id: Uuid = sqlx::query_scalar(
        "insert into product_batches
             (tenant_id, product_id, produced_on, expires_at,
              qty_received, qty_remaining, unit_cost)
         values ($1, $2, $3, $4, $5, $5, $6)
         returning id",
    )
    .bind(tenant_id)
    .bind(product_id)
    .bind(produced_on)
    .bind(expires_at)
    .bind(qty)
    .bind(unit_cost)
    .fetch_one(pool)
    .await
    .expect("batch");

    sqlx::query("update products set stock_qty = stock_qty + $2 where id = $1")
        .bind(product_id)
        .bind(qty)
        .execute(pool)
        .await
        .expect("cache");

    id
}

async fn cache(pool: &PgPool, product_id: Uuid) -> Decimal {
    sqlx::query_scalar("select stock_qty from products where id = $1")
        .bind(product_id)
        .fetch_one(pool)
        .await
        .expect("cache")
}

fn dec(value: &str) -> Decimal {
    value.parse().expect("decimal")
}

#[sqlx::test(migrations = "../../migrations")]
async fn waste_removes_finished_goods_and_records_a_negative_movement(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;
    let batch = add_batch(&pool, cake, "2026-01-01", None, "10", Some("12000")).await;

    let (status, body) = correct(
        &app,
        token,
        json!({
            "product_batch_id": batch,
            "type": "waste",
            "qty": "-2.5",
            "note": "  cracked  "
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["type"], "waste");
    assert_eq!(body["qty"], "-2.5");
    assert_eq!(body["unit_cost"], "12000.0000");
    assert_eq!(body["note"], "cracked");
    assert_eq!(body["product_id"], cake.to_string());
    assert_eq!(body["product_batch_id"], batch.to_string());
    assert_eq!(body["batch_qty_remaining"], "7.500");

    assert_eq!(cache(&pool, cake).await, dec("7.500"));

    let (product_batch, material_batch): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "select product_batch_id, batch_id from stock_movements
         where product_batch_id = $1",
    )
    .bind(batch)
    .fetch_one(&pool)
    .await
    .expect("movement");
    assert_eq!(product_batch, Some(batch));
    assert_eq!(material_batch, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn writing_off_a_whole_batch_depletes_it(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;
    let batch = add_batch(
        &pool,
        cake,
        "2026-01-01",
        Some("2026-01-04"),
        "5",
        Some("12000"),
    )
    .await;

    let (status, body) = correct(
        &app,
        token,
        json!({ "product_batch_id": batch, "type": "expired", "qty": "-5" }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["batch_qty_remaining"], "0");

    let batches = list_batches(&app, token, cake).await;
    assert!(
        batches.as_array().expect("batches").is_empty(),
        "a depleted batch is history, not stock"
    );
    assert_eq!(cache(&pool, cake).await, dec("0.000"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_depleted_batch_can_no_longer_be_sold(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;
    let batch = add_batch(
        &pool,
        cake,
        "2026-01-01",
        Some("2026-01-04"),
        "5",
        Some("12000"),
    )
    .await;

    let (status, body) = correct(
        &app,
        token,
        json!({ "product_batch_id": batch, "type": "expired", "qty": "-5" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = call(
        &app,
        "POST",
        "/sales",
        Some(json!({ "items": [{ "product_id": cake, "qty": "1" }] })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "insufficient_stock");
}

#[sqlx::test(migrations = "../../migrations")]
async fn adjustments_move_stock_both_ways(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;
    let batch = add_batch(&pool, cake, "2026-01-01", None, "10", Some("12000")).await;

    for (qty, expected) in [("-3", "7.000"), ("2", "9.000")] {
        let (status, body) = correct(
            &app,
            token,
            json!({ "product_batch_id": batch, "type": "adjustment", "qty": qty }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["type"], "adjustment");
        assert_eq!(body["batch_qty_remaining"], expected);
    }

    assert_eq!(cache(&pool, cake).await, dec("9.000"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_correction_cannot_take_more_than_the_batch_holds(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;
    let batch = add_batch(&pool, cake, "2026-01-01", None, "5", Some("12000")).await;

    let (status, body) = correct(
        &app,
        token,
        json!({ "product_batch_id": batch, "type": "waste", "qty": "-6" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "insufficient_batch_quantity");

    let (status, body) = correct(
        &app,
        token,
        json!({ "product_batch_id": batch, "type": "adjustment", "qty": "1" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "quantity_exceeds_received");

    assert_eq!(cache(&pool, cake).await, dec("5.000"));

    let (movements,): (i64,) =
        sqlx::query_as("select count(*) from stock_movements where product_batch_id = $1")
            .bind(batch)
            .fetch_one(&pool)
            .await
            .expect("movements");
    assert_eq!(
        movements, 0,
        "no movement is written for a rejected correction"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn correction_signs_and_types_are_validated(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;
    let batch = add_batch(&pool, cake, "2026-01-01", None, "5", Some("12000")).await;

    for body in [
        json!({ "product_batch_id": batch, "type": "waste", "qty": "1" }),
        json!({ "product_batch_id": batch, "type": "expired", "qty": "0" }),
        json!({ "product_batch_id": batch, "type": "adjustment", "qty": "0" }),
    ] {
        let (status, response) = correct(&app, token, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["error"]["code"], "invalid_quantity");
    }

    let (status, _) = correct(
        &app,
        token,
        json!({ "product_batch_id": batch, "type": "purchase", "qty": "1" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "purchases belong to production, not corrections"
    );

    assert_eq!(cache(&pool, cake).await, dec("5.000"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_can_produce_but_not_correct(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, owner_token).await;
    let cake = product(&app, owner_token, "Bolu pandan").await;
    let batch = add_batch(&pool, cake, "2026-01-01", None, "5", Some("12000")).await;

    add_member(
        &pool,
        tenant_id,
        "production@example.test",
        "Production",
        "production",
    )
    .await;
    let production = login(&app, &otp, "production@example.test").await;
    let token = production["access_token"].as_str().expect("token");

    let (status, body) = correct(
        &app,
        token,
        json!({ "product_batch_id": batch, "type": "waste", "qty": "-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "permission_denied");

    let batches = list_batches(&app, token, cake).await;
    assert_eq!(batches.as_array().expect("batches").len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_from_another_tenant_cannot_be_corrected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let cake = product(&app, first_token, "Bolu pandan").await;
    let batch = add_batch(&pool, cake, "2026-01-01", None, "5", Some("12000")).await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = correct(
        &app,
        second_token,
        json!({ "product_batch_id": batch, "type": "waste", "qty": "-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "batch_not_found");

    let (remaining,): (Decimal,) =
        sqlx::query_as("select qty_remaining from product_batches where id = $1")
            .bind(batch)
            .fetch_one(&pool)
            .await
            .expect("batch");
    assert_eq!(remaining, dec("5.000"));
    assert_eq!(cache(&pool, cake).await, dec("5.000"));

    let listed = list_batches(&app, second_token, cake).await;
    assert!(listed.as_array().expect("batches").is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_batch_list_is_fefo_and_excludes_depleted_batches(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let cake = product(&app, token, "Bolu pandan").await;

    // Produced earlier but expires later, so it sorts after the second batch.
    add_batch(
        &pool,
        cake,
        "2026-01-01",
        Some("2026-01-10"),
        "5",
        Some("10000"),
    )
    .await;
    add_batch(
        &pool,
        cake,
        "2026-01-02",
        Some("2026-01-05"),
        "5",
        Some("20000"),
    )
    .await;
    add_batch(&pool, cake, "2026-01-03", None, "5", Some("30000")).await;

    let batches = list_batches(&app, token, cake).await;
    let rows = batches.as_array().expect("batches");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["expires_at"], "2026-01-05");
    assert_eq!(rows[1]["expires_at"], "2026-01-10");
    assert_eq!(rows[1]["product_name"], "Bolu pandan");
    assert_eq!(rows[2]["expires_at"], Value::Null, "no expiry sorts last");
}
