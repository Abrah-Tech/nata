//! End to end: stock-in → production → HPP → sale → report.
//!
//! Every module has its own tests, but they prove each step in isolation. These
//! tests walk one business flow and check that the numbers still agree at the
//! end, which is what a user actually experiences and what a regression in the
//! hand-off between modules would break.

mod common;

use axum::{http::StatusCode, Router};
use common::{call, date_from_now, signup, test_app, CaptureOtp};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

const PRODUCT_NAME: &str = "Kue lapis";
const MATERIAL_NAME: &str = "Tepung terigu";
/// Seven days from production, so the batch is still sellable today.
const SHELF_LIFE_DAYS: i64 = 7;
/// Half an hour of labor at 30,000 an hour, for one batch of ten.
const LABOR_RATE_PER_HOUR: &str = "30000";
const SELL_PRICE: &str = "15000";

async fn material(app: &Router, token: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;

    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": MATERIAL_NAME, "base_unit_id": kg })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn product(app: &Router, token: &str) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/products",
        Some(json!({
            "name": PRODUCT_NAME,
            "sell_price": SELL_PRICE,
            "shelf_life_days": SHELF_LIFE_DAYS,
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

/// The recipe: 2.5 kg of flour per batch of ten, 5% waste, half an hour of work.
async fn recipe(app: &Router, token: &str, product_id: Uuid, material_id: Uuid) {
    let (status, body) = call(
        app,
        "POST",
        &format!("/products/{product_id}/recipes"),
        Some(json!({
            "yield_qty": "10",
            "labor_minutes": 30,
            "items": [{ "material_id": material_id, "qty": "2.5", "waste_pct": "5" }],
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["version"], 1, "{body}");
}

async fn stock_in(
    app: &Router,
    token: &str,
    material_id: Uuid,
    qty: &str,
    expires_in_days: i64,
    unit_cost: &str,
) -> Value {
    let (status, batch) = call(
        app,
        "POST",
        "/stock/batches",
        Some(json!({
            "material_id": material_id,
            "qty_received": qty,
            "expires_at": date_from_now(expires_in_days),
            "unit_cost": unit_cost,
        })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");

    batch
}

/// The active batches of a material, which is also how a test finds one to
/// correct.
async fn material_batches(app: &Router, token: &str, material_id: Uuid) -> Vec<Value> {
    let (status, batches) = call(
        app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batches}");

    batches.as_array().expect("batches").clone()
}

async fn produce(app: &Router, token: &str, product_id: Uuid, qty: &str) -> Value {
    let (status, order) = call(
        app,
        "POST",
        "/production-orders",
        Some(json!({ "product_id": product_id, "qty": qty })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    order
}

async fn sell(app: &Router, token: &str, product_id: Uuid, qty: &str) -> Value {
    let (status, sale) = call(
        app,
        "POST",
        "/sales",
        Some(json!({ "items": [{ "product_id": product_id, "qty": qty }] })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sale}");

    sale
}

async fn product_row(app: &Router, token: &str, product_id: Uuid) -> Value {
    let (status, products) = call(app, "GET", "/products", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{products}");

    products
        .as_array()
        .expect("products")
        .iter()
        .find(|row| row["id"].as_str() == Some(&product_id.to_string()))
        .cloned()
        .expect("product in list")
}

async fn finished_batches(app: &Router, token: &str, product_id: Uuid) -> Vec<Value> {
    let (status, batches) = call(
        app,
        "GET",
        &format!("/stock/product-batches?product_id={product_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batches}");

    batches.as_array().expect("batches").clone()
}

async fn report(app: &Router, token: &str) -> Value {
    let (status, body) = call(app, "GET", "/reports", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    body
}

async fn margin_of(report: &Value, product_id: Uuid) -> Value {
    report["products"]
        .as_array()
        .expect("products")
        .iter()
        .find(|row| row["product_id"].as_str() == Some(&product_id.to_string()))
        .cloned()
        .expect("product margin")
}

/// Reads a JSON number as a decimal, so an assertion does not depend on the
/// scale Postgres happened to return.
fn decimal(value: &Value) -> Decimal {
    value
        .as_str()
        .expect("a decimal encoded as a string")
        .parse()
        .expect("decimal")
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_to_hpp_to_sale_ties_out(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token).await;
    let cake = product(&app, token).await;
    recipe(&app, token, cake, flour).await;

    let (status, settings) = call(
        &app,
        "PUT",
        "/settings",
        Some(json!({ "labor_rate_per_hour": LABOR_RATE_PER_HOUR })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{settings}");

    // Two batches at different prices and expiries, so FEFO has a decision to
    // make and the weighted cost is not simply one of the two.
    let near = stock_in(&app, token, flour, "1", 5, "12000").await;
    let far = stock_in(&app, token, flour, "5", 30, "9000").await;

    // 2.5 kg × 1.05 waste = 2.625 kg, and the near batch expires first.
    let (status, preview) = call(
        &app,
        "POST",
        "/production-orders/preview",
        Some(json!({ "product_id": cake, "qty": "10" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["can_produce"], true);
    let planned = preview["materials"][0]["batches"]
        .as_array()
        .expect("batches");
    assert_eq!(
        planned[0]["batch_id"], near["id"],
        "FEFO takes the near batch"
    );
    assert_eq!(decimal(&preview["materials"][0]["required"]), dec("2.625"));
    assert_eq!(decimal(&preview["materials"][0]["available"]), dec("2.625"));
    assert_eq!(decimal(&preview["materials"][0]["shortfall"]), dec("0"));
    assert_eq!(decimal(&planned[0]["qty"]), dec("1.000"));
    assert_eq!(planned[1]["batch_id"], far["id"]);
    assert_eq!(decimal(&planned[1]["qty"]), dec("1.625"));

    let order = produce(&app, token, cake, "10").await;

    // 1 × 12000 + 1.625 × 9000 = 26625 of material, plus 0.5 h × 30000 of
    // labor, over ten units.
    assert_eq!(decimal(&order["material_cost"]), dec("26625.00"));
    assert_eq!(decimal(&order["labor_cost"]), dec("15000.00"));
    assert_eq!(decimal(&order["unit_cost"]), dec("4162.5000"));
    assert_eq!(decimal(&order["actual_qty"]), dec("10.000"));
    assert_eq!(order["status"], "done");

    // The material ledger moved by exactly what the recipe asked for.
    assert_eq!(order["consumptions"][0]["batch_id"], near["id"]);
    assert_eq!(decimal(&order["consumptions"][0]["qty"]), dec("1.000"));
    assert_eq!(order["consumptions"][1]["batch_id"], far["id"]);
    assert_eq!(decimal(&order["consumptions"][1]["qty"]), dec("1.625"));

    // The finished batch carries the cost a later sale will snapshot.
    let batches = finished_batches(&app, token, cake).await;
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["production_order_id"], order["id"]);
    assert_eq!(decimal(&batches[0]["qty_received"]), dec("10.000"));
    assert_eq!(decimal(&batches[0]["qty_remaining"]), dec("10.000"));
    assert_eq!(decimal(&batches[0]["unit_cost"]), dec("4162.5000"));
    assert_eq!(
        batches[0]["expires_at"].as_str(),
        Some(date_from_now(SHELF_LIFE_DAYS).as_str()),
        "produced today, so it expires shelf_life_days from now"
    );

    let row = product_row(&app, token, cake).await;
    assert_eq!(decimal(&row["stock_qty"]), dec("10.000"));
    assert_eq!(decimal(&row["sellable_qty"]), dec("10.000"));

    // Four of the ten go out at the shelf price.
    let sale = sell(&app, token, cake, "4").await;
    assert_eq!(decimal(&sale["total"]), dec("60000.00"));
    assert_eq!(decimal(&sale["items"][0]["price"]), dec("15000.00"));

    let batches = finished_batches(&app, token, cake).await;
    assert_eq!(decimal(&batches[0]["qty_remaining"]), dec("6.000"));
    assert_eq!(
        decimal(&product_row(&app, token, cake).await["stock_qty"]),
        dec("6.000")
    );

    // The report is where the whole flow has to agree: what is left, at what
    // cost, and what the four sold units earned.
    let business = report(&app, token).await;
    assert_eq!(business["window_days"], 30, "the tenant default");

    assert_eq!(decimal(&business["stock"]["materials"]), dec("30375.00"));
    assert_eq!(decimal(&business["stock"]["products"]), dec("24975.00"));
    assert_eq!(decimal(&business["stock"]["total"]), dec("55350.00"));

    let margin = margin_of(&business, cake).await;
    assert_eq!(decimal(&margin["sell_price"]), dec("15000.00"));
    assert_eq!(decimal(&margin["unit_cost"]), dec("4162.5000"));
    assert_eq!(decimal(&margin["margin_per_unit"]), dec("10837.50"));
    assert_eq!(decimal(&margin["margin_pct"]), dec("72.25"));
    assert_eq!(decimal(&margin["sold_qty"]), dec("4.000"));
    assert_eq!(decimal(&margin["revenue"]), dec("60000.00"));
    assert_eq!(decimal(&margin["cost_of_goods"]), dec("16650.00"));
    assert_eq!(decimal(&margin["margin"]), dec("43350.00"));
    assert_eq!(margin["cost_incomplete"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_write_off_after_production_moves_the_valuation_only(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token).await;
    let cake = product(&app, token).await;
    recipe(&app, token, cake, flour).await;

    stock_in(&app, token, flour, "5", 30, "9000").await;
    let order = produce(&app, token, cake, "10").await;

    // 5 kg in, 2.625 kg consumed: 2.375 kg is left to write off against.
    let batches = material_batches(&app, token, flour).await;
    assert_eq!(batches.len(), 1);
    let batch_id = batches[0]["id"].as_str().expect("batch id").to_string();

    let (status, correction) = call(
        &app,
        "POST",
        "/stock/corrections",
        Some(json!({ "batch_id": batch_id, "type": "waste", "qty": "-1" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{correction}");
    assert_eq!(decimal(&correction["batch_qty_remaining"]), dec("1.375"));

    // The write-off changes what the business owns and reports as a loss, but
    // not the cost already snapshot into the order or the finished batch.
    let business = report(&app, token).await;
    assert_eq!(decimal(&business["stock"]["materials"]), dec("12375.00"));
    assert_eq!(
        decimal(&business["stock"]["products"]),
        dec("23625.00"),
        "ten units at the cost they were produced at"
    );
    assert_eq!(decimal(&business["waste"]["value"]), dec("9000.00"));
    assert_eq!(business["waste"]["movements"], 1);

    // 2.625 kg × 9000 = 23625 of material, no labor rate set on this tenant.
    let finished = finished_batches(&app, token, cake).await;
    assert_eq!(decimal(&finished[0]["unit_cost"]), dec("2362.5000"));

    let (snapshot,): (Decimal,) =
        sqlx::query_as("select unit_cost from production_orders where id = $1")
            .bind(
                order["id"]
                    .as_str()
                    .expect("order id")
                    .parse::<Uuid>()
                    .expect("uuid"),
            )
            .fetch_one(&pool)
            .await
            .expect("production order");

    assert_eq!(snapshot, dec("2362.5000"));
}

fn dec(value: &str) -> Decimal {
    value.parse().expect("decimal")
}
