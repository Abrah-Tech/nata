mod common;

use axum::{http::StatusCode, Router};
use common::{call, create_material, signup, stock_in, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn warehouse(app: &Router, token: &str, name: &str) -> Value {
    let (status, body) = call(
        app,
        "POST",
        "/warehouses",
        Some(json!({ "name": name })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body
}

async fn warehouse_id(app: &Router, token: &str, name: &str) -> Uuid {
    warehouse(app, token, name).await["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

/// A tenant that may hold more than one location, so a filter has something to
/// separate.
async fn upgrade(pool: &PgPool, app: &Router, token: &str) -> Uuid {
    let tenant_id = tenant_of(app, token).await;
    sqlx::query("update tenants set plan = 'business' where id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("upgrade");

    tenant_id
}

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

async fn recipe(app: &Router, token: &str, product_id: Uuid, material_id: Uuid) {
    let (status, body) = call(
        app,
        "POST",
        &format!("/products/{product_id}/recipes"),
        Some(json!({
            "yield_qty": "10",
            "labor_minutes": 30,
            "items": [{ "material_id": material_id, "qty": "1", "waste_pct": "0" }]
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

/// The movements that matter, read back with their warehouse.
async fn movements(app: &Router, token: &str, filter: &str) -> Vec<Value> {
    let (status, body) = call(
        app,
        "GET",
        &format!("/stock/movements{filter}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    body.as_array().expect("movements").clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_in_puts_a_batch_in_a_warehouse_and_the_trail_shows_it(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    upgrade(&pool, &app, token).await;

    let material_id = create_material(&app, token, "Tepung").await;
    let first = warehouse_id(&app, token, "Gudang Satu").await;
    let second = warehouse_id(&app, token, "Gudang Dua").await;

    let (status, batch) = stock_in(
        &app,
        token,
        json!({
            "material_id": material_id,
            "warehouse_id": first,
            "qty_received": "5",
            "unit_cost": "1000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    assert_eq!(batch["warehouse_id"], first.to_string());

    let (status, batch) = stock_in(
        &app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": "7",
            "unit_cost": "1000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    assert!(
        batch["warehouse_id"].is_null(),
        "a receipt may be unassigned"
    );

    let (status, all) = call(&app, "GET", "/stock/batches", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().expect("batches").len(), 2);

    let (status, filtered) = call(
        &app,
        "GET",
        &format!("/stock/batches?warehouse_id={first}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = filtered.as_array().expect("batches");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["warehouse_id"], first.to_string());

    let (status, filtered) = call(
        &app,
        "GET",
        &format!("/stock/batches?warehouse_id={second}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(filtered.as_array().expect("batches").is_empty());

    // On-hand is a sum over batches, so the location filter decides which
    // batches are counted.
    let (status, on_hand) = call(
        &app,
        "GET",
        &format!("/stock?warehouse_id={first}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = on_hand.as_array().expect("stock");
    let flour = rows
        .iter()
        .find(|row| row["material_id"] == material_id.to_string())
        .expect("flour");
    assert_eq!(flour["on_hand"], "5.000");

    let trail = movements(&app, token, &format!("?warehouse_id={first}")).await;
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0]["type"], "purchase");
    assert_eq!(trail[0]["warehouse_name"], "Gudang Satu");
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_in_rejects_another_tenants_warehouse(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let material_id = create_material(&app, first_token, "Tepung").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");
    let foreign = warehouse_id(&app, second_token, "Gudang Lain").await;

    let (status, error) = stock_in(
        &app,
        first_token,
        json!({
            "material_id": material_id,
            "warehouse_id": foreign,
            "qty_received": "1",
            "unit_cost": "1000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["error"]["code"], "invalid_warehouse");
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_puts_finished_goods_away_and_records_where_materials_left(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    upgrade(&pool, &app, token).await;

    let material_id = create_material(&app, token, "Tepung").await;
    let product_id = product(&app, token, "Roti").await;
    recipe(&app, token, product_id, material_id).await;

    let store = warehouse_id(&app, token, "Gudang Bahan").await;
    let shelf = warehouse_id(&app, token, "Gudang Jadi").await;

    let (status, batch) = stock_in(
        &app,
        token,
        json!({
            "material_id": material_id,
            "warehouse_id": store,
            "qty_received": "10",
            "unit_cost": "1000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");

    let (status, order) = call(
        &app,
        "POST",
        "/production-orders",
        Some(json!({ "product_id": product_id, "qty": "10", "warehouse_id": shelf })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let (status, batches) = call(
        &app,
        "GET",
        &format!("/stock/product-batches?product_id={product_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = batches.as_array().expect("product batches");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["warehouse_id"], shelf.to_string());
    assert_eq!(rows[0]["warehouse_name"], "Gudang Jadi");

    let trail = movements(&app, token, "").await;
    let produced = trail
        .iter()
        .find(|row| row["type"] == "production")
        .expect("production movement");
    assert_eq!(produced["warehouse_id"], shelf.to_string());

    // A consumption movement belongs to a material batch, so it is not found
    // through the finished product; it is read from the whole trail.
    let used = trail
        .iter()
        .find(|row| row["type"] == "production_use")
        .expect("consumption movement");
    assert_eq!(
        used["warehouse_id"],
        store.to_string(),
        "the material left the warehouse its batch was stored in"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_sale_records_the_warehouse_the_batch_came_from(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    upgrade(&pool, &app, token).await;

    let material_id = create_material(&app, token, "Tepung").await;
    let product_id = product(&app, token, "Roti").await;
    recipe(&app, token, product_id, material_id).await;

    let store = warehouse_id(&app, token, "Gudang Bahan").await;
    let shelf = warehouse_id(&app, token, "Gudang Jadi").await;

    let (status, _) = stock_in(
        &app,
        token,
        json!({
            "material_id": material_id,
            "warehouse_id": store,
            "qty_received": "10",
            "unit_cost": "1000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, order) = call(
        &app,
        "POST",
        "/production-orders",
        Some(json!({ "product_id": product_id, "qty": "10", "warehouse_id": shelf })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let (status, sale) = call(
        &app,
        "POST",
        "/sales",
        Some(json!({ "items": [{ "product_id": product_id, "qty": "2" }] })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sale}");

    let trail = movements(&app, token, &format!("?product_id={product_id}")).await;
    let sold = trail
        .iter()
        .find(|row| row["type"] == "sale")
        .expect("sale movement");
    assert_eq!(sold["warehouse_id"], shelf.to_string());
    assert_eq!(sold["warehouse_name"], "Gudang Jadi");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_correction_and_a_count_record_the_batch_warehouse(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let material_id = create_material(&app, token, "Tepung").await;
    let store = warehouse_id(&app, token, "Gudang Bahan").await;

    let (status, batch) = stock_in(
        &app,
        token,
        json!({
            "material_id": material_id,
            "warehouse_id": store,
            "qty_received": "10",
            "unit_cost": "1000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let batch_id = batch["id"].as_str().expect("id");

    let (status, correction) = call(
        &app,
        "POST",
        "/stock/corrections",
        Some(json!({ "batch_id": batch_id, "type": "waste", "qty": "-1" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{correction}");
    assert_eq!(correction["warehouse_id"], store.to_string());
    assert_eq!(correction["warehouse_name"], "Gudang Bahan");

    let (status, count) = call(
        &app,
        "POST",
        "/stock/opname",
        Some(json!({ "counts": [{ "batch_id": batch_id, "qty": "8" }] })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{count}");

    let trail = movements(&app, token, &format!("?batch_id={batch_id}")).await;
    let adjustment = trail
        .iter()
        .find(|row| row["type"] == "adjustment")
        .expect("count adjustment");
    assert_eq!(adjustment["warehouse_id"], store.to_string());
}
