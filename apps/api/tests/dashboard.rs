mod common;

use axum::{http::StatusCode, Router};
use chrono::{Duration, NaiveDate, NaiveDateTime};
use common::{call, signup, stock_in, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn material(app: &Router, token: &str, name: &str, min_stock: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;

    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg, "min_stock": min_stock })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn batch(app: &Router, token: &str, material_id: Uuid, qty: &str, cost: &str) -> Uuid {
    let (status, body) = stock_in(
        app,
        token,
        json!({ "material_id": material_id, "qty_received": qty, "unit_cost": cost }),
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

/// A minute after midnight today and the matching minute yesterday.
///
/// Both dashboard windows run to this time of day, so the two points are one
/// day apart and each lands inside its own window at any hour the suite runs.
/// Taken from the database clock, which is also what the endpoint compares
/// against, so a session in another zone cannot shift the boundary.
async fn today_and_yesterday(pool: &PgPool) -> (NaiveDateTime, NaiveDateTime) {
    let today: NaiveDate = sqlx::query_scalar("select now()::date")
        .fetch_one(pool)
        .await
        .expect("today");
    let midnight = today.and_hms_opt(0, 0, 0).expect("midnight");

    (
        midnight + Duration::minutes(1),
        midnight - Duration::days(1) + Duration::minutes(1),
    )
}

/// Writes a sale straight into the ledger. The overview reads the snapshots
/// only, so the write path does not have to run. A null `cost` reproduces a
/// sale written before `sale_items.unit_cost` existed.
async fn sale(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
    qty: &str,
    price: &str,
    cost: Option<&str>,
    sold_at: NaiveDateTime,
) {
    let (sale_id,): (Uuid,) = sqlx::query_as(
        "insert into sales (tenant_id, sold_at, channel, total)
         values ($1, $2, 'toko', $3::numeric * $4::numeric)
         returning id",
    )
    .bind(tenant_id)
    .bind(sold_at)
    .bind(qty)
    .bind(price)
    .fetch_one(pool)
    .await
    .expect("sale");

    sqlx::query(
        "insert into sale_items (sale_id, tenant_id, product_id, qty, price, unit_cost)
         values ($1, $2, $3, $4::numeric, $5::numeric, $6::numeric)",
    )
    .bind(sale_id)
    .bind(tenant_id)
    .bind(product_id)
    .bind(qty)
    .bind(price)
    .bind(cost)
    .execute(pool)
    .await
    .expect("sale item");
}

/// Expiry is set relative to the database clock, the same clock the endpoint
/// compares against, so the assertions do not depend on the machine date.
async fn set_expiry(pool: &PgPool, batch_id: Uuid, days: i32) {
    sqlx::query("update material_batches set expires_at = current_date + $2::int where id = $1")
        .bind(batch_id)
        .bind(days)
        .execute(pool)
        .await
        .expect("set expiry");
}

fn names(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            row["name"]
                .as_str()
                .or_else(|| row["material_name"].as_str())
                .expect("name")
        })
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_ranks_shortages_and_sums_stock_value(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung", "5").await;
    let chili = material(&app, token, "Cabai", "3").await;
    let salt = material(&app, token, "Garam", "2").await;
    let sugar = material(&app, token, "Gula", "0").await;

    batch(&app, token, flour, "10", "12000").await;
    batch(&app, token, salt, "1", "3000").await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let summary = &body["summary"];
    assert_eq!(summary["materials"], 4);
    assert_eq!(summary["below_min"], 2, "cabai and garam are under minimum");
    assert_eq!(summary["out_of_stock"], 1, "cabai has nothing on hand");
    assert_eq!(summary["stock_value"], "123000.00");
    assert_eq!(summary["active_batches"], 2);
    assert_eq!(summary["horizon_days"], 7);

    // Worst ratio first: cabai is empty against a minimum of 3, garam is half.
    let ids: Vec<String> = body["critical"]
        .as_array()
        .expect("critical")
        .iter()
        .map(|row| row["material_id"].as_str().expect("id").to_string())
        .collect();
    assert_eq!(ids, vec![chili.to_string(), salt.to_string()]);
    assert!(
        !ids.contains(&flour.to_string()),
        "a stocked material is safe"
    );
    assert!(
        !ids.contains(&sugar.to_string()),
        "a material with no minimum is never critical"
    );

    let first = &body["critical"][0];
    assert_eq!(first["on_hand"], "0");
    assert_eq!(first["shortfall"], "3.000");
    assert_eq!(first["base_unit"]["code"], "kg");
    assert_eq!(first["active_batches"], 0);
    assert_eq!(first["nearest_expiry"], Value::Null);

    let second = &body["critical"][1];
    assert_eq!(second["on_hand"], "1.000");
    assert_eq!(second["shortfall"], "1.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_values_and_counts_only_usable_stock(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung", "5").await;
    let fresh = batch(&app, token, flour, "10", "12000").await;
    let expired = batch(&app, token, flour, "4", "12000").await;
    set_expiry(&pool, fresh, 10).await;
    set_expiry(&pool, expired, -1).await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        body["summary"]["stock_value"], "120000.00",
        "the expired batch is not an asset at cost"
    );
    assert_eq!(
        body["summary"]["below_min"], 0,
        "ten usable units are enough"
    );
    assert_eq!(body["summary"]["out_of_stock"], 0);
    assert_eq!(
        body["summary"]["active_batches"], 2,
        "the expired batch is still on the shelf"
    );
    assert_eq!(body["summary"]["expired"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_lists_the_expiry_window_in_fefo_order(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let milk = material(&app, token, "Susu", "0").await;
    let cheese = material(&app, token, "Keju", "0").await;

    let late = batch(&app, token, milk, "1", "18000").await;
    let soon = batch(&app, token, cheese, "2", "90000").await;
    let today = batch(&app, token, milk, "3", "18000").await;
    let past = batch(&app, token, cheese, "4", "90000").await;
    let outside = batch(&app, token, milk, "5", "18000").await;

    for (batch_id, days) in [(past, -1), (today, 0), (soon, 3), (late, 7), (outside, 8)] {
        set_expiry(&pool, batch_id, days).await;
    }

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["summary"]["expiring"], 3, "today, +3, and +7");
    assert_eq!(body["summary"]["expired"], 1);
    assert_eq!(body["summary"]["active_batches"], 5);

    let rows = body["expiring"].as_array().expect("expiring");
    assert_eq!(rows.len(), 4, "an expired batch is still worth acting on");
    assert_eq!(
        names(&body["expiring"]),
        vec!["Keju", "Susu", "Keju", "Susu"],
        "earliest expiry first"
    );

    let days: Vec<i64> = rows
        .iter()
        .map(|row| row["days_left"].as_i64().expect("days_left"))
        .collect();
    assert_eq!(days, vec![-1, 0, 3, 7]);
    assert_eq!(rows[0]["qty_remaining"], "4.000");
    assert_eq!(rows[0]["unit_cost"], "90000.0000");
    assert_eq!(rows[0]["base_unit"]["code"], "kg");
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_skips_depleted_batches_and_other_tenants(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let mine = material(&app, token, "Tepung", "5").await;
    let depleted = batch(&app, token, mine, "2", "12000").await;
    set_expiry(&pool, depleted, 2).await;

    sqlx::query("update material_batches set qty_remaining = 0 where id = $1")
        .bind(depleted)
        .execute(&pool)
        .await
        .expect("deplete");

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let other_material = material(&app, other_token, "Cabai", "5").await;
    let other_batch = batch(&app, other_token, other_material, "1", "1000").await;
    set_expiry(&pool, other_batch, 1).await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["summary"]["materials"], 1, "only this tenant");
    assert_eq!(body["summary"]["active_batches"], 0);
    assert_eq!(body["summary"]["expiring"], 0);
    assert!(body["expiring"].as_array().expect("expiring").is_empty());
    assert_eq!(body["summary"]["below_min"], 1, "tepung is empty");

    let other_tenant = tenant_of(&app, other_token).await;
    assert_ne!(other_tenant, tenant_of(&app, token).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_caps_the_lists_but_not_the_counts(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    for index in 0..10 {
        material(&app, token, &format!("Bahan {index}"), "5").await;
    }

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["summary"]["below_min"], 10);
    assert_eq!(
        body["critical"].as_array().expect("critical").len(),
        8,
        "the list is capped so the payload stays small"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_requires_a_session(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp);

    let (status, _) = call(&app, "GET", "/dashboard", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_totals_today_against_the_same_stretch_of_yesterday(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant = tenant_of(&app, token).await;

    let bread = product(&app, token, "Roti sobek", "15000").await;
    let cake = product(&app, token, "Kue coklat", "40000").await;

    let (today, yesterday) = today_and_yesterday(&pool).await;

    sale(&pool, tenant, bread, "3", "15000", Some("9000"), today).await;
    sale(&pool, tenant, cake, "1", "40000", Some("25000"), today).await;
    sale(&pool, tenant, bread, "2", "15000", Some("9000"), yesterday).await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let sales = &body["sales"];
    assert_eq!(sales["revenue"], "85000.00");
    assert_eq!(sales["cost_of_goods"], "52000.00");
    assert_eq!(sales["profit"], "33000.00");
    assert_eq!(sales["cost_incomplete"], false);
    assert_eq!(sales["yesterday_revenue"], "30000.00");
    assert_eq!(sales["yesterday_profit"], "12000.00");

    let best = sales["best_sellers"].as_array().expect("best sellers");
    let ranked: Vec<(&str, &str, &str)> = best
        .iter()
        .map(|row| {
            (
                row["name"].as_str().expect("name"),
                row["qty"].as_str().expect("qty"),
                row["revenue"].as_str().expect("revenue"),
            )
        })
        .collect();
    assert_eq!(
        ranked,
        vec![
            ("Roti sobek", "3.000", "45000.00"),
            ("Kue coklat", "1.000", "40000.00")
        ],
        "most units first, revenue breaks a tie"
    );
    assert_eq!(best[0]["product_id"], bread.to_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_withholds_profit_when_a_sale_has_no_cost(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant = tenant_of(&app, token).await;

    let bread = product(&app, token, "Roti sobek", "15000").await;
    let (today, _yesterday) = today_and_yesterday(&pool).await;

    sale(&pool, tenant, bread, "2", "15000", None, today).await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let sales = &body["sales"];
    assert_eq!(sales["revenue"], "30000.00", "revenue needs no cost");
    assert_eq!(sales["cost_of_goods"], Value::Null);
    assert_eq!(sales["profit"], Value::Null, "an unknown cost is not zero");
    assert_eq!(sales["cost_incomplete"], true);
    assert_eq!(
        sales["yesterday_revenue"], "0.00",
        "a tenant that sold nothing yesterday"
    );
    assert_eq!(sales["yesterday_profit"], Value::Null);
    assert_eq!(
        sales["best_sellers"]
            .as_array()
            .expect("best sellers")
            .len(),
        1,
        "the ranking is units, so it does not depend on cost"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_counts_only_the_tenants_own_sales(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant = tenant_of(&app, token).await;

    let mine = product(&app, token, "Roti sobek", "15000").await;
    let (today, _yesterday) = today_and_yesterday(&pool).await;
    sale(&pool, tenant, mine, "1", "15000", Some("9000"), today).await;

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let other_tenant = tenant_of(&app, other_token).await;
    let theirs = product(&app, other_token, "Kue coklat", "40000").await;
    sale(
        &pool,
        other_tenant,
        theirs,
        "5",
        "40000",
        Some("25000"),
        today,
    )
    .await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_ne!(tenant, other_tenant);
    assert_eq!(body["sales"]["revenue"], "15000.00", "only this tenant");
    let best = body["sales"]["best_sellers"]
        .as_array()
        .expect("best sellers");
    assert_eq!(best.len(), 1);
    assert_eq!(best[0]["name"], "Roti sobek");
}
