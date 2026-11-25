mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, test_app, CaptureOtp};
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
    let mut body = json!({ "name": name, "sell_price": "15000" });
    if let Some(days) = shelf_life_days {
        body["shelf_life_days"] = json!(days);
    }

    let (status, body) = call(app, "POST", "/products", Some(body), Some(token)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn create_recipe(app: &Router, token: &str, product_id: Uuid, material_id: Uuid) {
    let (status, body) = call(
        app,
        "POST",
        &format!("/products/{product_id}/recipes"),
        Some(json!({
            "yield_qty": "10",
            "labor_minutes": 30,
            "items": [{ "material_id": material_id, "qty": "0.5", "waste_pct": "0" }]
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

/// Finished goods exist only as a product batch, so stock is seeded the same
/// way production writes it: a batch, plus the product's cached total.
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

/// A batch past its date is still on the shelf, but it cannot be sold.
async fn expired_batch(pool: &PgPool, product_id: Uuid, qty: &str) {
    let qty = qty.parse::<rust_decimal::Decimal>().expect("qty");

    sqlx::query(
        "insert into product_batches
             (tenant_id, product_id, produced_on, expires_at, qty_received,
              qty_remaining, unit_cost)
         select tenant_id, $1, current_date - 5, current_date - 1, $2, $2, null
         from products where id = $1",
    )
    .bind(product_id)
    .bind(qty)
    .execute(pool)
    .await
    .expect("expired batch");
}

async fn sell(app: &Router, token: &str, product_id: Uuid, qty: &str) {
    let (status, body) = call(
        app,
        "POST",
        "/sales",
        Some(json!({ "items": [{ "product_id": product_id, "qty": qty }] })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn suggestions(app: &Router, token: &str, days: Option<i32>) -> (StatusCode, Value) {
    let query = days.map(|days| format!("?days={days}")).unwrap_or_default();
    call(
        app,
        "GET",
        &format!("/production/suggestions{query}"),
        None,
        Some(token),
    )
    .await
}

/// Finds one product's suggestion by id.
fn find<'a>(body: &'a Value, product_id: Uuid) -> &'a Value {
    body["products"]
        .as_array()
        .expect("products")
        .iter()
        .find(|row| row["product_id"].as_str() == Some(&product_id.to_string()))
        .expect("suggestion")
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_run_covers_the_days_ahead_net_of_sellable_stock(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    let cake = product(&app, token, "Kue lapis", Some(30)).await;
    create_recipe(&app, token, cake, flour).await;

    // 30 sold over the 30-day window is one a day; 10 are still sellable.
    set_stock(&pool, cake, "40").await;
    sell(&app, token, cake, "30").await;

    let (status, body) = suggestions(&app, token, Some(14)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["days_ahead"], 14);
    assert_eq!(body["window_days"], 30, "the tenant's own window");

    let row = find(&body, cake);
    assert_eq!(row["average_daily_sales"], "1.000");
    assert_eq!(row["sellable_stock"], "10.000");
    assert_eq!(row["target_qty"], "14.000");
    assert_eq!(row["recommended_qty"], "4.000");
    assert_eq!(row["capped_by_shelf_life"], false);
    assert_eq!(row["insufficient_history"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_that_already_covers_the_window_needs_no_run(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    let cake = product(&app, token, "Kue lapis", None).await;
    create_recipe(&app, token, cake, flour).await;

    // 90 sold over 30 days is three a day; 100 on hand cover the next week.
    set_stock(&pool, cake, "190").await;
    sell(&app, token, cake, "90").await;

    let (status, body) = suggestions(&app, token, Some(7)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = find(&body, cake);
    assert_eq!(row["average_daily_sales"], "3.000");
    assert_eq!(row["target_qty"], "21.000");
    assert_eq!(row["recommended_qty"], "0");
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_stock_does_not_reduce_the_run(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    // The shelf life is long, so the cap never bites; only the expiry date of
    // the stock decides what can still be sold.
    let cake = product(&app, token, "Kue lapis", Some(30)).await;
    create_recipe(&app, token, cake, flour).await;

    set_stock(&pool, cake, "40").await;
    sell(&app, token, cake, "30").await;
    expired_batch(&pool, cake, "10").await;

    let (status, body) = suggestions(&app, token, Some(14)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Ten fresh units are sellable; the ten past their date are not. Counting
    // them would say the shelf is full and nothing needs making.
    let row = find(&body, cake);
    assert_eq!(row["sellable_stock"], "10.000");
    assert_eq!(row["recommended_qty"], "4.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn shelf_life_caps_a_run_that_would_outlast_the_product(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    let cake = product(&app, token, "Kue lapis", Some(2)).await;
    create_recipe(&app, token, cake, flour).await;

    // Five a day, a two-day shelf life, one unit on hand. Two weeks of demand
    // is 70, but only 9 of them can be sold before the run goes past its date.
    set_stock(&pool, cake, "151").await;
    sell(&app, token, cake, "150").await;

    let (status, body) = suggestions(&app, token, Some(14)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = find(&body, cake);
    assert_eq!(row["average_daily_sales"], "5.000");
    assert_eq!(row["target_qty"], "70.000");
    assert_eq!(row["recommended_qty"], "9.000");
    assert_eq!(row["capped_by_shelf_life"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_that_never_sold_has_no_rate_to_plan_from(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    let cake = product(&app, token, "Kue lapis", Some(2)).await;
    create_recipe(&app, token, cake, flour).await;

    let (status, body) = suggestions(&app, token, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["days_ahead"], 1, "tomorrow is the default");

    let row = find(&body, cake);
    assert_eq!(row["average_daily_sales"], "0.000");
    assert_eq!(row["target_qty"], "0");
    assert_eq!(row["recommended_qty"], "0");
    assert_eq!(row["insufficient_history"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_without_a_recipe_is_not_suggested(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    // A suggestion is an action, and the app refuses to run a product with no
    // recipe, so it has no place on the list.
    product(&app, token, "Kue lapis", None).await;

    let (status, body) = suggestions(&app, token, Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["products"].as_array().expect("products").is_empty(),
        "{body}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_largest_gap_is_listed_first(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    let small = product(&app, token, "Kue keju", None).await;
    let large = product(&app, token, "Brownies", None).await;
    create_recipe(&app, token, small, flour).await;
    create_recipe(&app, token, large, flour).await;

    // Brownies sell two a day and have 10 left; a week needs 14, so 4 are
    // missing. Kue keju sells one a day and has 10 left, which already covers
    // the week.
    set_stock(&pool, small, "40").await;
    sell(&app, token, small, "30").await;
    set_stock(&pool, large, "70").await;
    sell(&app, token, large, "60").await;

    let (status, body) = suggestions(&app, token, Some(7)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let products = body["products"].as_array().expect("products");
    assert_eq!(products.len(), 2);
    assert_eq!(products[0]["product_name"], "Brownies");
    assert_eq!(products[0]["recommended_qty"], "4.000");
    assert_eq!(products[1]["product_name"], "Kue keju");
    assert_eq!(products[1]["recommended_qty"], "0");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_days_ahead_must_be_a_day_or_two_weeks(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    for days in [0, -1, 15, 100] {
        let (status, body) = suggestions(&app, token, Some(days)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "invalid_days");
    }

    let (status, body) = suggestions(&app, token, Some(14)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn suggestions_are_scoped_to_the_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    let cake = product(&app, token, "Kue lapis", None).await;
    create_recipe(&app, token, cake, flour).await;

    // Everything sold, one a day: tomorrow's run is exactly one unit.
    set_stock(&pool, cake, "30").await;
    sell(&app, token, cake, "30").await;

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");

    // The second tenant has no products, so an empty list rather than the
    // first tenant's.
    let (status, body) = suggestions(&app, other_token, Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["products"].as_array().expect("products").is_empty(),
        "{body}"
    );

    let (status, body) = suggestions(&app, token, Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(find(&body, cake)["recommended_qty"], "1.000");
    assert_eq!(find(&body, cake)["sellable_stock"], "0");
}
