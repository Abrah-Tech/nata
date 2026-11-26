mod common;

use axum::{http::StatusCode, Router};
use common::{add_member, call, create_material, login, signup, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
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

async fn create_warehouse(app: &Router, token: &str, name: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/warehouses",
        Some(json!({ "name": name })),
        Some(token),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn warehouses_round_trip_through_the_api(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    // A business plan leaves room for the duplicate-name check, which the
    // database raises and the plan limit would otherwise mask.
    sqlx::query("update tenants set plan = 'business' where id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("upgrade");

    let (status, warehouse) = create_warehouse(&app, token, "  Gudang Utama  ").await;
    assert_eq!(status, StatusCode::CREATED, "{warehouse}");
    assert_eq!(warehouse["name"], "Gudang Utama", "the name is trimmed");
    let id = warehouse["id"].as_str().expect("id");

    let (status, list) = call(&app, "GET", "/warehouses", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().expect("warehouses").len(), 1);

    let (status, error) = create_warehouse(&app, token, "Gudang Utama").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "warehouse_exists");

    let (status, error) = create_warehouse(&app, token, "   ").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "name_required");

    let (status, renamed) = call(
        &app,
        "PUT",
        &format!("/warehouses/{id}"),
        Some(json!({ "name": "Gudang Barat" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["name"], "Gudang Barat");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/warehouses/{id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, list) = call(&app, "GET", "/warehouses", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().expect("warehouses").is_empty());

    let (status, error) = call(
        &app,
        "DELETE",
        &format!("/warehouses/{id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["error"]["code"], "warehouse_not_found");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_tenant_cannot_rename_or_delete_another_tenants_warehouse(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, foreign) = create_warehouse(&app, second_token, "Gudang Lain").await;
    assert_eq!(status, StatusCode::CREATED, "{foreign}");
    let foreign_id = foreign["id"].as_str().expect("id");

    let (status, list) = call(&app, "GET", "/warehouses", None, Some(first_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().expect("warehouses").is_empty());

    let (status, error) = call(
        &app,
        "PUT",
        &format!("/warehouses/{foreign_id}"),
        Some(json!({ "name": "Dicuri" })),
        Some(first_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["error"]["code"], "warehouse_not_found");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/warehouses/{foreign_id}"),
        None,
        Some(first_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_tenant_manager_can_write_a_warehouse(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;

    add_member(&pool, tenant_id, "viewer@example.test", "Vera", "viewer").await;
    let viewer = login(&app, &otp, "viewer@example.test").await;
    let viewer_token = viewer["access_token"].as_str().expect("token");

    let (status, _) = call(&app, "GET", "/warehouses", None, Some(viewer_token)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, error) = create_warehouse(&app, viewer_token, "Gudang").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "permission_denied");

    let (status, warehouse) = create_warehouse(&app, owner_token, "Gudang").await;
    assert_eq!(status, StatusCode::CREATED, "{warehouse}");
    let id = warehouse["id"].as_str().expect("id");

    let (status, error) = call(
        &app,
        "PUT",
        &format!("/warehouses/{id}"),
        Some(json!({ "name": "Gudang Baru" })),
        Some(viewer_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "permission_denied");

    let (status, error) = call(
        &app,
        "DELETE",
        &format!("/warehouses/{id}"),
        None,
        Some(viewer_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_plan_limit_stops_the_second_warehouse(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, warehouse) = create_warehouse(&app, token, "Gudang Satu").await;
    assert_eq!(status, StatusCode::CREATED, "{warehouse}");

    let (status, error) = create_warehouse(&app, token, "Gudang Dua").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "plan_limit_reached");
    assert_eq!(error["error"]["details"]["resource"], "warehouses");
    assert_eq!(error["error"]["details"]["limit"], 1);
    assert_eq!(error["error"]["details"]["current"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_warehouse_that_still_holds_stock_cannot_be_deleted(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let (status, warehouse) = create_warehouse(&app, token, "Gudang").await;
    assert_eq!(status, StatusCode::CREATED, "{warehouse}");
    let warehouse_id = warehouse["id"]
        .as_str()
        .expect("id")
        .parse::<Uuid>()
        .expect("uuid");

    let material_id = create_material(&app, token, "Tepung").await;
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
    .bind(warehouse_id)
    .execute(&pool)
    .await
    .expect("batch");

    let (status, error) = call(
        &app,
        "DELETE",
        &format!("/warehouses/{warehouse_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    assert_eq!(error["error"]["code"], "warehouse_in_use");
}
