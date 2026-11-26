mod common;

use axum::{http::StatusCode, Router};
use chrono::{Duration, NaiveDate, Utc};
use common::{call, signup, stock_in, test_app, CaptureOtp};
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

async fn stock(app: &Router, token: &str, material_id: Uuid, qty: &str, cost: &str) {
    let (status, body) = stock_in(
        app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": qty,
            "unit_cost": cost,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn set_stock(pool: &PgPool, product_id: Uuid, qty: &str) {
    let qty = qty.parse::<rust_decimal::Decimal>().expect("qty");

    // A batch with a null cost is the "never produced" case: the stock is
    // there, but its cost is unknown.
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

/// A production order with a cost, without running the recipe: the report only
/// reads the snapshot, and building a full BOM would test production instead.
async fn produced(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
    qty: &str,
    unit_cost: &str,
    produced_on: NaiveDate,
) -> Uuid {
    let existing: Option<Uuid> =
        sqlx::query_scalar("select id from recipes where product_id = $1 and tenant_id = $2")
            .bind(product_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .expect("recipe lookup");

    let recipe_id = match existing {
        Some(id) => id,
        None => sqlx::query_scalar(
            "insert into recipes (tenant_id, product_id, version, yield_qty)
             values ($1, $2, 1, 1)
             returning id",
        )
        .bind(tenant_id)
        .bind(product_id)
        .fetch_one(pool)
        .await
        .expect("recipe"),
    };

    let qty = qty.parse::<rust_decimal::Decimal>().expect("qty");
    let unit_cost = unit_cost.parse::<rust_decimal::Decimal>().expect("cost");

    let order_id: Uuid = sqlx::query_scalar(
        "insert into production_orders
             (tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on, unit_cost)
         values ($1, $2, $3, $4, $4, $5, $6)
         returning id",
    )
    .bind(tenant_id)
    .bind(product_id)
    .bind(recipe_id)
    .bind(qty)
    .bind(produced_on)
    .bind(unit_cost)
    .fetch_one(pool)
    .await
    .expect("production order");

    let batch_id: Uuid = sqlx::query_scalar(
        "insert into product_batches
             (tenant_id, product_id, production_order_id, produced_on,
              qty_received, qty_remaining, unit_cost)
         values ($1, $2, $3, $4, $5, $5, $6)
         returning id",
    )
    .bind(tenant_id)
    .bind(product_id)
    .bind(order_id)
    .bind(produced_on)
    .bind(qty)
    .bind(unit_cost)
    .fetch_one(pool)
    .await
    .expect("batch");

    sqlx::query(
        "insert into stock_movements
             (tenant_id, product_batch_id, type, qty, unit_cost)
         values ($1, $2, 'production', $3, $4)",
    )
    .bind(tenant_id)
    .bind(batch_id)
    .bind(qty)
    .bind(unit_cost)
    .execute(pool)
    .await
    .expect("movement");

    sqlx::query("update products set stock_qty = stock_qty + $2 where id = $1")
        .bind(product_id)
        .bind(qty)
        .execute(pool)
        .await
        .expect("stock cache");

    order_id
}

/// A run whose HPP carries an overhead component, which is what a tenant's
/// overhead rate produces. The components are split so a report that dropped
/// overhead from the cost of goods would be caught: material is the rest of the
/// unit cost, and `overhead_cost` is the part the rate added.
async fn produced_with_overhead(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
    qty: &str,
    unit_cost: &str,
    overhead_cost: &str,
) {
    let order_id = produced(
        pool,
        tenant_id,
        product_id,
        qty,
        unit_cost,
        Utc::now().date_naive(),
    )
    .await;

    let overhead = overhead_cost
        .parse::<rust_decimal::Decimal>()
        .expect("overhead");

    sqlx::query(
        "update production_orders
         set overhead_cost = $2,
             labor_cost = 0,
             material_cost = (actual_qty * unit_cost - $2)::numeric(18,2)
         where id = $1",
    )
    .bind(order_id)
    .bind(overhead)
    .execute(pool)
    .await
    .expect("order costs");
}

async fn sell(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/sales", Some(body), Some(token)).await
}

async fn report(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/reports", None, Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_is_valued_at_the_cost_each_batch_arrived_with(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    stock(&app, token, flour, "10", "10000").await;
    stock(&app, token, flour, "10", "20000").await;

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 10 x 10.000 + 10 x 20.000. Valuing at the material's latest price would
    // have said 400.000 and re-priced stock that was already on the shelf.
    assert_eq!(body["stock"]["materials"], "300000.00");
    assert_eq!(body["stock"]["products"], "0.00");
    assert_eq!(body["stock"]["total"], "300000.00");

    // The overview screen shows the same number.
    let (status, dashboard) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{dashboard}");
    assert_eq!(dashboard["summary"]["stock_value"], "300000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_stock_is_not_valued(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    stock(&app, token, flour, "10", "10000").await;

    let cake = product(&app, token, "Bolu", "15000").await;
    set_stock(&pool, cake, "5").await;

    sqlx::query("update material_batches set expires_at = current_date - 1")
        .execute(&pool)
        .await
        .expect("expire the material batch");
    sqlx::query("update product_batches set expires_at = current_date - 1, unit_cost = 10000")
        .execute(&pool)
        .await
        .expect("expire the product batch");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stock"]["materials"], "0.00");
    assert_eq!(body["stock"]["products"], "0.00");
    assert_eq!(body["stock"]["total"], "0.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn waste_is_valued_at_the_batch_it_left(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    stock(&app, token, flour, "10", "10000").await;

    let (status, batches) = call(
        &app,
        "GET",
        &format!("/stock/batches?material_id={flour}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batches}");

    let batch_id = batches
        .as_array()
        .expect("batches")
        .first()
        .expect("a batch")["id"]
        .as_str()
        .expect("batch id");

    let (status, body) = call(
        &app,
        "POST",
        "/stock/corrections",
        Some(json!({ "batch_id": batch_id, "type": "waste", "qty": "-3" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["waste"]["value"], "30000.00");
    assert_eq!(body["waste"]["movements"], 1);
    assert_eq!(
        body["waste"]["by_material"][0]["material_name"],
        "Tepung terigu"
    );
    assert_eq!(body["waste"]["by_material"][0]["qty"], "3.000");
    assert_eq!(body["waste"]["by_material"][0]["value"], "30000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn margin_uses_the_cost_snapshotted_at_sale_time(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let cake = product(&app, token, "Bolu pandan", "20000").await;

    produced(
        &pool,
        tenant_id,
        cake,
        "5",
        "12000",
        Utc::now().date_naive(),
    )
    .await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "2" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = &body["products"][0];
    assert_eq!(row["name"], "Bolu pandan");
    assert_eq!(row["unit_cost"], "12000.0000");
    assert_eq!(row["margin_per_unit"], "8000.00");
    assert_eq!(row["margin_pct"], "40.00");
    assert_eq!(row["sold_qty"], "2.000");
    assert_eq!(row["revenue"], "40000.00");
    assert_eq!(row["cost_of_goods"], "24000.00");
    assert_eq!(row["margin"], "16000.00");
    assert_eq!(row["cost_incomplete"], false);

    // A later run at a higher cost changes the forward-looking unit margin,
    // never the margin already earned.
    produced(
        &pool,
        tenant_id,
        cake,
        "1",
        "15000",
        Utc::now().date_naive() + Duration::days(1),
    )
    .await;

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = &body["products"][0];
    assert_eq!(row["unit_cost"], "15000.0000");
    assert_eq!(row["margin_per_unit"], "5000.00");
    assert_eq!(row["cost_of_goods"], "24000.00");
    assert_eq!(row["margin"], "16000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_never_produced_has_no_margin(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", "15000").await;
    set_stock(&pool, cake, "5").await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = &body["products"][0];
    assert_eq!(row["unit_cost"], Value::Null);
    assert_eq!(row["margin_per_unit"], Value::Null);
    assert_eq!(row["margin_pct"], Value::Null);
    assert_eq!(row["revenue"], "15000.00");
    assert_eq!(row["cost_of_goods"], Value::Null);
    assert_eq!(row["margin"], Value::Null);
    assert_eq!(row["cost_incomplete"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_tenant_window_counts(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "PUT",
        "/settings",
        Some(json!({ "usage_window_days": 7 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    set_stock(&pool, cake, "10").await;

    let old = (Utc::now() - Duration::days(20)).to_rfc3339();

    let (status, body) = sell(
        &app,
        token,
        json!({
            "sold_at": old,
            "items": [{ "product_id": cake, "qty": "4" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["window_days"], 7);
    assert_eq!(body["products"][0]["sold_qty"], "1.000");
    assert_eq!(body["products"][0]["revenue"], "20000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_can_read_the_report(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    material(&app, token, "Tepung terigu").await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["products"].as_array().expect("products").len(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn export_follows_the_plan(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    // A new tenant is on the trial: the report is readable, the file is not.
    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["can_export"], false);

    for (plan, expected) in [("starter", false), ("pro", true), ("business", true)] {
        sqlx::query("update tenants set plan = $2 where id = $1")
            .bind(tenant_id)
            .bind(plan)
            .execute(&pool)
            .await
            .expect("plan");

        let (status, body) = report(&app, token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["can_export"], expected, "plan {plan}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn profit_loss_is_revenue_minus_the_cost_of_goods(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    produced(
        &pool,
        tenant_id,
        cake,
        "5",
        "12000",
        Utc::now().date_naive(),
    )
    .await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "2" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["profit_loss"]["revenue"], "40000.00");
    assert_eq!(body["profit_loss"]["cost_of_goods"], "24000.00");
    assert_eq!(body["profit_loss"]["gross_profit"], "16000.00");
    assert_eq!(body["profit_loss"]["cost_incomplete"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn profit_loss_is_incomplete_when_a_sale_has_no_cost(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", "15000").await;
    set_stock(&pool, cake, "5").await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Revenue is still known, but an unknown cost is not a zero cost, so the
    // two figures that depend on it come back null.
    assert_eq!(body["profit_loss"]["revenue"], "15000.00");
    assert_eq!(body["profit_loss"]["cost_of_goods"], Value::Null);
    assert_eq!(body["profit_loss"]["gross_profit"], Value::Null);
    assert_eq!(body["profit_loss"]["cost_incomplete"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn overhead_is_charged_once_inside_the_cost_of_goods(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let cake = product(&app, token, "Bolu pandan", "20000").await;

    // Five units at 12 000, of which 10 000 is overhead. The cost of goods for
    // two units must be the full 24 000; valuing them at material alone would
    // say 20 000 and silently drop the overhead the tenant asked to recover.
    produced_with_overhead(&pool, tenant_id, cake, "5", "12000", "10000").await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "2" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["profit_loss"]["revenue"], "40000.00");
    assert_eq!(body["profit_loss"]["cost_of_goods"], "24000.00");
    assert_eq!(body["profit_loss"]["gross_profit"], "16000.00");
}

async fn set_plan(pool: &PgPool, tenant_id: Uuid, plan: &str) {
    sqlx::query("update tenants set plan = $2 where id = $1")
        .bind(tenant_id)
        .bind(plan)
        .execute(pool)
        .await
        .expect("plan");
}

/// Midday UTC, so a `sold_at` lands on the intended calendar day whatever the
/// database session timezone is.
fn noon(date: NaiveDate) -> String {
    date.and_hms_opt(12, 0, 0)
        .expect("midday")
        .and_utc()
        .to_rfc3339()
}

/// The first batch of a material, which is the one FEFO reaches for first.
async fn batch_of(app: &Router, token: &str, material_id: Uuid) -> Uuid {
    let (status, body) = call(
        app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    body.as_array().expect("batches").first().expect("a batch")["id"]
        .as_str()
        .expect("batch id")
        .parse()
        .expect("uuid")
}

/// Records what a run drew from a batch, at the cost it carried then.
async fn consume(pool: &PgPool, order_id: Uuid, batch_id: Uuid, qty: &str, unit_cost: &str) {
    sqlx::query(
        "insert into production_consumptions
             (production_order_id, batch_id, qty, unit_cost)
         values ($1, $2, $3, $4)",
    )
    .bind(order_id)
    .bind(batch_id)
    .bind(qty.parse::<rust_decimal::Decimal>().expect("qty"))
    .bind(unit_cost.parse::<rust_decimal::Decimal>().expect("cost"))
    .execute(pool)
    .await
    .expect("consumption");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_chosen_range_replaces_the_window(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    set_stock(&pool, cake, "10").await;

    let today = Utc::now().date_naive();
    let last_month = today - Duration::days(60);

    let (status, body) = sell(
        &app,
        token,
        json!({
            "sold_at": noon(last_month),
            "items": [{ "product_id": cake, "qty": "4" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The default window is thirty days, so last month is out of reach.
    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["range"]["days"], body["window_days"]);
    assert_eq!(body["profit_loss"]["revenue"], "0.00");

    // Asking for the month reaches it, and the echo says what was covered.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/reports?from={last_month}&to={today}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["range"]["from"], last_month.to_string());
    assert_eq!(body["range"]["to"], today.to_string());
    assert_eq!(body["range"]["days"], 61);
    assert_eq!(body["profit_loss"]["revenue"], "80000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_range_must_be_whole_in_order_and_short(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let today = Utc::now().date_naive();
    let last_year = today - Duration::days(400);

    let cases = [
        (format!("/reports?from={today}"), "invalid_range"),
        (format!("/reports?to={today}"), "invalid_range"),
        (
            format!(
                "/reports?from={}&to={}",
                today - Duration::days(9),
                today - Duration::days(14)
            ),
            "invalid_range",
        ),
        (
            "/reports?from=2026-13-01&to=2026-11-26".to_string(),
            "invalid_range",
        ),
        (
            format!("/reports?from={last_year}&to={today}"),
            "range_too_long",
        ),
    ];

    for (path, code) in cases {
        let (status, body) = call(&app, "GET", &path, None, Some(token)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        assert_eq!(body["error"]["code"], code, "{path}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn advanced_sections_belong_to_business(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    // A new tenant is on the trial: the report reads, the deeper sections do
    // not exist and cost nothing to compute.
    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["can_advanced"], false);
    assert_eq!(body["trend"].as_array().expect("trend").len(), 0);
    assert_eq!(body["usage"].as_array().expect("usage").len(), 0);
    assert_eq!(body["purchasing"].as_array().expect("purchasing").len(), 0);

    for (plan, expected) in [("starter", false), ("pro", false), ("business", true)] {
        set_plan(&pool, tenant_id, plan).await;

        let (status, body) = report(&app, token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["can_advanced"], expected, "plan {plan}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_trend_groups_sales_by_day(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;
    set_plan(&pool, tenant_id, "business").await;

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    produced(
        &pool,
        tenant_id,
        cake,
        "5",
        "12000",
        Utc::now().date_naive(),
    )
    .await;

    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);

    for (day, qty) in [(yesterday, "1"), (today, "2")] {
        let (status, body) = sell(
            &app,
            token,
            json!({
                "sold_at": noon(day),
                "items": [{ "product_id": cake, "qty": qty }],
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let (status, body) = call(
        &app,
        "GET",
        &format!("/reports?from={yesterday}&to={today}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let trend = body["trend"].as_array().expect("trend");
    assert_eq!(trend.len(), 2);
    assert_eq!(trend[0]["date"], yesterday.to_string());
    assert_eq!(trend[0]["revenue"], "20000.00");
    assert_eq!(trend[0]["gross_profit"], "8000.00");
    assert_eq!(trend[0]["cost_incomplete"], false);
    assert_eq!(trend[1]["date"], today.to_string());
    assert_eq!(trend[1]["revenue"], "40000.00");
    assert_eq!(trend[1]["gross_profit"], "16000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn usage_counts_what_production_drew(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;
    set_plan(&pool, tenant_id, "business").await;

    let flour = material(&app, token, "Tepung terigu").await;
    stock(&app, token, flour, "10", "10000").await;
    let batch_id = batch_of(&app, token, flour).await;

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    let today = Utc::now().date_naive();
    let last_month = today - Duration::days(60);

    let recent = produced(&pool, tenant_id, cake, "2", "12000", today).await;
    consume(&pool, recent, batch_id, "3", "10000").await;

    let older = produced(&pool, tenant_id, cake, "1", "12000", last_month).await;
    consume(&pool, older, batch_id, "5", "10000").await;

    // The thirty-day window sees only the recent run.
    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let usage = body["usage"].as_array().expect("usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0]["material_name"], "Tepung terigu");
    assert_eq!(usage[0]["qty"], "3.000");
    assert_eq!(usage[0]["unit_cost"], "10000.0000");
    assert_eq!(usage[0]["value"], "30000.00");
    assert_eq!(usage[0]["runs"], 1);

    // Widening the range pulls the older run in and counts both.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/reports?from={last_month}&to={today}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let usage = body["usage"].as_array().expect("usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0]["qty"], "8.000");
    assert_eq!(usage[0]["value"], "80000.00");
    assert_eq!(usage[0]["runs"], 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn purchasing_groups_by_supplier_and_material(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;
    set_plan(&pool, tenant_id, "business").await;

    let (status, body) = call(
        &app,
        "POST",
        "/suppliers",
        Some(json!({ "name": "Toko Sumber" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let supplier_id = body["id"].as_str().expect("supplier id");

    let flour = material(&app, token, "Tepung terigu").await;
    let sugar = material(&app, token, "Gula pasir").await;

    // Two batches of flour from the supplier, one of sugar with none recorded.
    for (qty, cost) in [("10", "10000"), ("4", "11000")] {
        let (status, body) = stock_in(
            &app,
            token,
            json!({
                "material_id": flour,
                "qty_received": qty,
                "unit_cost": cost,
                "supplier_id": supplier_id,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    stock(&app, token, sugar, "5", "9000").await;

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let purchasing = body["purchasing"].as_array().expect("purchasing");
    assert_eq!(purchasing.len(), 2);

    let flour_row = purchasing
        .iter()
        .find(|row| row["material_name"] == "Tepung terigu")
        .expect("flour");
    assert_eq!(flour_row["supplier_name"], "Toko Sumber");
    assert_eq!(flour_row["qty"], "14.000");
    assert_eq!(flour_row["value"], "144000.00");
    assert_eq!(flour_row["batches"], 2);

    let sugar_row = purchasing
        .iter()
        .find(|row| row["material_name"] == "Gula pasir")
        .expect("sugar");
    assert_eq!(sugar_row["supplier_name"], Value::Null);
    assert_eq!(sugar_row["qty"], "5.000");
    assert_eq!(sugar_row["value"], "45000.00");
}
