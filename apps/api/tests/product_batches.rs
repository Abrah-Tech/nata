mod common;

use axum::{http::StatusCode, Router};
use chrono::NaiveDate;
use common::{call, signup, stock_in, test_app, CaptureOtp};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

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

async fn product(app: &Router, token: &str, name: &str, shelf_life_days: Option<i32>) -> Uuid {
    let body = json!({ "name": name, "sell_price": "15000", "shelf_life_days": shelf_life_days });

    let (status, body) = call(app, "POST", "/products", Some(body), Some(token)).await;
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

async fn stocked_batch(
    app: &Router,
    token: &str,
    material_id: Uuid,
    qty: &str,
    expires_at: &str,
    unit_cost: &str,
) -> Value {
    let (status, batch) = stock_in(
        app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": qty,
            "expires_at": expires_at,
            "unit_cost": unit_cost,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    batch
}

async fn produce(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/production-orders", Some(body), Some(token)).await
}

async fn sell(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/sales", Some(body), Some(token)).await
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

/// `(id, qty_remaining, expires_at, unit_cost)` for every active batch.
async fn batches(
    pool: &PgPool,
    product_id: Uuid,
) -> Vec<(Uuid, Decimal, Option<NaiveDate>, Option<Decimal>)> {
    sqlx::query_as(
        "select id, qty_remaining, expires_at, unit_cost
         from product_batches
         where product_id = $1
         order by produced_on, id",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await
    .expect("batches")
}

async fn sale_unit_cost(pool: &PgPool, sale_id: Uuid) -> Option<Decimal> {
    sqlx::query_scalar(
        "select unit_cost from sale_items where sale_id = $1 order by product_id limit 1",
    )
    .bind(sale_id)
    .fetch_one(pool)
    .await
    .expect("unit cost")
}

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("date")
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_writes_a_batch_with_the_product_shelf_life(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", Some(3)).await;
    let flour = material(&app, token, "Tepung terigu").await;
    create_recipe(&app, token, cake, flour).await;
    stocked_batch(&app, token, flour, "5", "2026-06-01", "10000").await;

    let (status, order) = produce(
        &app,
        token,
        json!({ "product_id": cake, "qty": "10", "produced_on": "2026-01-01" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let rows = batches(&pool, cake).await;
    assert_eq!(rows.len(), 1, "one batch per run");

    let (batch_id, remaining, expires_at, unit_cost) = &rows[0];
    assert_eq!(remaining.to_string(), "10.000");
    assert_eq!(*expires_at, Some(date("2026-01-04")));
    assert_eq!(
        unit_cost.expect("cost").to_string(),
        order["unit_cost"].as_str().expect("order cost")
    );

    // The batch points at the run that produced it.
    let (order_id,): (Uuid,) =
        sqlx::query_as("select production_order_id from product_batches where id = $1")
            .bind(batch_id)
            .fetch_one(&pool)
            .await
            .expect("order link");
    assert_eq!(
        order_id,
        order["id"]
            .as_str()
            .expect("id")
            .parse::<Uuid>()
            .expect("uuid")
    );

    // One inbound movement, with the material batch reference left null.
    let (movement_type, qty, material_batch): (String, Decimal, Option<Uuid>) = sqlx::query_as(
        "select type, qty, batch_id from stock_movements where product_batch_id = $1",
    )
    .bind(batch_id)
    .fetch_one(&pool)
    .await
    .expect("movement");
    assert_eq!(movement_type, "production");
    assert_eq!(qty.to_string(), "10.000");
    assert_eq!(material_batch, None);

    let (cache,): (Decimal,) = sqlx::query_as("select stock_qty from products where id = $1")
        .bind(cake)
        .fetch_one(&pool)
        .await
        .expect("cache");
    assert_eq!(cache.to_string(), "10.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_without_shelf_life_gets_a_batch_without_expiry(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let sauce = product(&app, token, "Saus sambal", None).await;
    let chilli = material(&app, token, "Cabe").await;
    create_recipe(&app, token, sauce, chilli).await;
    stocked_batch(&app, token, chilli, "5", "2026-06-01", "10000").await;

    let (status, order) = produce(
        &app,
        token,
        json!({ "product_id": sauce, "qty": "10", "produced_on": "2026-01-01" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let rows = batches(&pool, sauce).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_sale_takes_the_earliest_expiry_batch_first(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", None).await;

    // The later-produced batch expires first, so FEFO takes it first.
    let first = add_batch(
        &pool,
        cake,
        "2026-01-01",
        Some("2026-01-10"),
        "5",
        Some("10000"),
    )
    .await;
    let second = add_batch(
        &pool,
        cake,
        "2026-01-02",
        Some("2026-01-05"),
        "5",
        Some("20000"),
    )
    .await;

    let (status, sale) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "3" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sale}");

    let sale_id: Uuid = sale["id"].as_str().expect("id").parse().expect("uuid");
    assert_eq!(
        sale_unit_cost(&pool, sale_id).await,
        Some(dec("20000.0000"))
    );

    let rows = batches(&pool, cake).await;
    let remaining: std::collections::HashMap<Uuid, Decimal> =
        rows.into_iter().map(|(id, qty, _, _)| (id, qty)).collect();
    assert_eq!(remaining[&first].to_string(), "5.000");
    assert_eq!(remaining[&second].to_string(), "2.000");

    let (cache,): (Decimal,) = sqlx::query_as("select stock_qty from products where id = $1")
        .bind(cake)
        .fetch_one(&pool)
        .await
        .expect("cache");
    assert_eq!(cache.to_string(), "7.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_sale_spanning_two_batches_snapshots_the_weighted_cost(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", None).await;

    add_batch(
        &pool,
        cake,
        "2026-01-01",
        Some("2026-01-05"),
        "2",
        Some("10000"),
    )
    .await;
    add_batch(
        &pool,
        cake,
        "2026-01-02",
        Some("2026-01-10"),
        "5",
        Some("20000"),
    )
    .await;

    let (status, sale) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "4" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sale}");

    let sale_id: Uuid = sale["id"].as_str().expect("id").parse().expect("uuid");
    // (2 x 10.000 + 2 x 20.000) / 4 = 15.000.
    assert_eq!(
        sale_unit_cost(&pool, sale_id).await,
        Some(dec("15000.0000"))
    );

    let rows = batches(&pool, cake).await;
    let mut remaining: Vec<Decimal> = rows.into_iter().map(|(_, qty, _, _)| qty).collect();
    remaining.sort();
    assert_eq!(remaining, vec![dec("0.000"), dec("3.000")]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_without_a_known_cost_marks_the_sale_incomplete(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", None).await;
    add_batch(&pool, cake, "2026-01-01", None, "5", None).await;

    let (status, sale) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sale}");

    let sale_id: Uuid = sale["id"].as_str().expect("id").parse().expect("uuid");
    assert_eq!(sale_unit_cost(&pool, sale_id).await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_batches_never_leak_across_tenants(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let cake = product(&app, first_token, "Bolu pandan", None).await;
    let flour = material(&app, first_token, "Tepung terigu").await;
    create_recipe(&app, first_token, cake, flour).await;
    stocked_batch(&app, first_token, flour, "5", "2026-06-01", "10000").await;

    let (status, order) = produce(
        &app,
        first_token,
        json!({ "product_id": cake, "qty": "10", "produced_on": "2026-01-01" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    // The foreign product is invisible in the list and cannot be sold.
    let (status, products) = call(&app, "GET", "/products", None, Some(second_token)).await;
    assert_eq!(status, StatusCode::OK, "{products}");
    assert!(products.as_array().expect("products").is_empty());

    let (status, body) = sell(
        &app,
        second_token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // The batch is untouched.
    let rows = batches(&pool, cake).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1.to_string(), "10.000");
}

fn dec(value: &str) -> Decimal {
    value.parse().expect("decimal")
}
