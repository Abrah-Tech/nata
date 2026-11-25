mod common;

use axum::{http::StatusCode, Router};
use common::{add_member, call, login, signup, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

/// The trial caps a tenant at one seat, so a second member needs room.
async fn tenant_with_seats(app: &Router, otp: &CaptureOtp, pool: &PgPool) -> String {
    let owner = signup(app, otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(app, token).await;

    sqlx::query("update tenants set plan = 'business' where id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("plan");

    token.to_string()
}

async fn product(app: &Router, token: &str, name: &str, sell_price: &str) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/products",
        Some(json!({ "name": name, "sell_price": sell_price })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn set_stock(pool: &PgPool, product_id: Uuid, qty: &str) {
    let qty = qty.parse::<rust_decimal::Decimal>().expect("qty");

    sqlx::query(
        "insert into product_batches
             (tenant_id, product_id, produced_on, qty_received, qty_remaining, unit_cost)
         select tenant_id, $1, current_date, $2, $2, null
         from products where id = $1",
    )
    .bind(product_id)
    .bind(qty)
    .execute(pool)
    .await
    .expect("batch");

    sqlx::query("update products set stock_qty = $2 where id = $1")
        .bind(product_id)
        .bind(qty)
        .execute(pool)
        .await
        .expect("stock");
}

async fn permission_denied(app: &Router, method: &str, uri: &str, token: &str) -> Value {
    let (status, body) = call(app, method, uri, None, Some(token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{uri} returned {body}");
    assert_eq!(body["error"]["code"], "permission_denied", "{uri}");
    body
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_cashier_sells_and_reads_nothing_that_costs(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner_token = tenant_with_seats(&app, &otp, &pool).await;
    let bread = product(&app, &owner_token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let tenant_id = tenant_of(&app, &owner_token).await;
    add_member(&pool, tenant_id, "cashier@example.test", "Kasir", "cashier").await;

    let cashier = login(&app, &otp, "cashier@example.test").await;
    let token = cashier["access_token"].as_str().expect("token");

    let (status, me) = call(&app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["role"], "cashier");

    let mut permissions: Vec<String> = me["permissions"]
        .as_array()
        .expect("permissions")
        .iter()
        .map(|value| value.as_str().expect("permission").to_string())
        .collect();
    permissions.sort();
    assert_eq!(permissions, ["product_read", "sales_read", "sales_write"]);

    // The register and its catalogue are the cashier's whole surface.
    let (status, products) = call(&app, "GET", "/products", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{products}");
    assert_eq!(products.as_array().expect("products").len(), 1);

    let (status, sold) = call(
        &app,
        "POST",
        "/sales",
        Some(json!({ "items": [{ "product_id": bread, "qty": "2" }] })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sold}");
    assert_eq!(sold["total"], "30000.00");

    let (status, sales) = call(&app, "GET", "/sales", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{sales}");
    assert_eq!(sales["items"].as_array().expect("sales").len(), 1);

    // Every route that carries a cost or a margin is closed, and the cashier
    // cannot hand the role out either.
    permission_denied(&app, "GET", "/materials", token).await;
    permission_denied(&app, "GET", &format!("/products/{bread}/recipes"), token).await;
    permission_denied(&app, "GET", "/reports", token).await;
    permission_denied(&app, "GET", "/dashboard", token).await;
    permission_denied(&app, "GET", "/stock", token).await;
    permission_denied(&app, "GET", "/purchasing/recommendations", token).await;
    permission_denied(&app, "GET", "/members", token).await;
    permission_denied(&app, "POST", "/members/invitations", token).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_admin_may_invite_a_cashier(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner_token = tenant_with_seats(&app, &otp, &pool).await;
    let tenant_id = tenant_of(&app, &owner_token).await;
    add_member(&pool, tenant_id, "admin@example.test", "Admin", "admin").await;

    let admin = login(&app, &otp, "admin@example.test").await;
    let token = admin["access_token"].as_str().expect("token");

    let (status, invited) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "newcashier@example.test", "role": "cashier" })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{invited}");
    assert_eq!(invited["role"], "cashier");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_owner_may_demote_a_member_to_cashier(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let staff = add_member(&pool, tenant_id, "staff@example.test", "Staff", "viewer").await;

    let (status, changed) = call(
        &app,
        "PATCH",
        &format!("/members/{staff}/role"),
        Some(json!({ "role": "cashier" })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["role"], "cashier");
}
