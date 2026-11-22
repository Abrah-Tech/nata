mod common;

use axum::{http::StatusCode, Router};
use common::{call, create_material, signup, tenant_of, test_app, CaptureOtp};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// A warehouse row, written directly: phase 4 is what exposes the create, and
/// the limit is only meaningful if the count reads the table.
async fn add_warehouse(pool: &PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("insert into warehouses (tenant_id, name) values ($1, $2) returning id")
        .bind(tenant_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("warehouse")
}

async fn subscription(app: &Router, token: &str) -> Value {
    let (status, body) = call(app, "GET", "/subscription", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[sqlx::test(migrations = "../../migrations")]
async fn warehouse_usage_counts_the_tenants_own_rows(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let body = subscription(&app, token).await;
    assert_eq!(body["limits"]["warehouses"], 1, "the trial allows one");
    assert_eq!(body["usage"]["warehouses"], 0);

    add_warehouse(&pool, tenant_id, "Gudang Utama").await;

    let body = subscription(&app, token).await;
    assert_eq!(body["usage"]["warehouses"], 1);

    // Another tenant's location is not this tenant's consumption.
    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let other_tenant = tenant_of(&app, other_token).await;
    add_warehouse(&pool, other_tenant, "Gudang Utama").await;

    let body = subscription(&app, token).await;
    assert_eq!(body["usage"]["warehouses"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_warehouse_name_is_unique_within_a_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;
    add_warehouse(&pool, tenant_id, "Gudang Utama").await;

    let duplicate = sqlx::query("insert into warehouses (tenant_id, name) values ($1, $2)")
        .bind(tenant_id)
        .bind("Gudang Utama")
        .execute(&pool)
        .await;
    assert!(duplicate.is_err(), "a name is used once per tenant");

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let other_tenant = tenant_of(&app, other_token).await;

    sqlx::query("insert into warehouses (tenant_id, name) values ($1, $2)")
        .bind(other_tenant)
        .bind("Gudang Utama")
        .execute(&pool)
        .await
        .expect("the same name is free in another tenant");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_cannot_point_at_another_tenants_warehouse(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let other_tenant = tenant_of(&app, other_token).await;

    let own = add_warehouse(&pool, tenant_id, "Gudang Satu").await;
    let foreign = add_warehouse(&pool, other_tenant, "Gudang Dua").await;

    let material_id = create_material(&app, token, "Tepung terigu").await;

    let batch_id = Uuid::new_v4();
    sqlx::query(
        "insert into material_batches
             (id, tenant_id, material_id, received_at, qty_received, qty_remaining, unit_cost,
              warehouse_id)
         values ($1, $2, $3, current_date, 1, 1, 1000, $4)",
    )
    .bind(batch_id)
    .bind(tenant_id)
    .bind(material_id)
    .bind(own)
    .execute(&pool)
    .await
    .expect("batch in its own warehouse");

    let rejected = sqlx::query("update material_batches set warehouse_id = $1 where id = $2")
        .bind(foreign)
        .bind(batch_id)
        .execute(&pool)
        .await;
    assert!(
        rejected.is_err(),
        "the composite key must refuse another tenant's warehouse"
    );

    // The movement trail carries the same reference and the same protection.
    let rejected = sqlx::query(
        "insert into stock_movements (tenant_id, batch_id, type, qty, unit_cost, warehouse_id)
         values ($1, $2, 'purchase', 1, 1000, $3)",
    )
    .bind(tenant_id)
    .bind(batch_id)
    .bind(foreign)
    .execute(&pool)
    .await;
    assert!(
        rejected.is_err(),
        "a movement cannot name another tenant's warehouse"
    );

    sqlx::query(
        "insert into stock_movements (tenant_id, batch_id, type, qty, unit_cost, warehouse_id)
         values ($1, $2, 'purchase', 1, 1000, $3)",
    )
    .bind(tenant_id)
    .bind(batch_id)
    .bind(own)
    .execute(&pool)
    .await
    .expect("a movement in its own warehouse");
}
