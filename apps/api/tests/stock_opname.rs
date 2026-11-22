mod common;

use axum::{http::StatusCode, Router};
use common::{
    add_member, call, create_material, login, signup, stock_in, tenant_of, test_app, CaptureOtp,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn batch(app: &Router, token: &str, material_id: Uuid, qty: &str) -> Uuid {
    let (status, body) = stock_in(
        app,
        token,
        json!({ "material_id": material_id, "qty_received": qty, "unit_cost": "12000" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn remaining(pool: &PgPool, batch_id: Uuid) -> String {
    let (qty,): (Decimal,) =
        sqlx::query_as("select qty_remaining from material_batches where id = $1")
            .bind(batch_id)
            .fetch_one(pool)
            .await
            .expect("batch");

    qty.to_string()
}

/// Only the count adjustments: a stock-in writes a `purchase` movement of its
/// own, which says nothing about whether the sheet was applied.
async fn movements(pool: &PgPool, batch_id: Uuid) -> Vec<(String, Decimal, Option<String>)> {
    sqlx::query_as(
        "select type, qty, note from stock_movements
         where batch_id = $1 and type = 'adjustment'
         order by created_at",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await
    .expect("movements")
}

async fn apply_count(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/stock/opname", Some(body), Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_count_writes_one_adjustment_per_changed_batch(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = create_material(&app, token, "Tepung").await;
    let first = batch(&app, token, flour, "10").await;
    let second = batch(&app, token, flour, "5").await;

    let (status, body) = apply_count(
        &app,
        token,
        json!({
            "note": "Opname 22 Nov",
            "counts": [
                { "batch_id": first, "qty": "7.25" },
                { "batch_id": second, "qty": "5" },
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], 1);
    assert_eq!(body["unchanged"], 1, "a row left alone writes nothing");

    assert_eq!(remaining(&pool, first).await, "7.250");
    assert_eq!(
        remaining(&pool, second).await,
        "5.000",
        "the counted row that already agreed keeps the ledger"
    );

    let written = movements(&pool, first).await;
    assert_eq!(written.len(), 1, "one movement per batch that differed");
    assert_eq!(written[0].0, "adjustment");
    assert_eq!(written[0].1.to_string(), "-2.750");
    assert_eq!(written[0].2.as_deref(), Some("Opname 22 Nov"));

    assert!(
        movements(&pool, second).await.is_empty(),
        "the matching row leaves no trail behind"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_count_that_matches_the_ledger_changes_nothing(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = create_material(&app, token, "Tepung").await;
    let first = batch(&app, token, flour, "10").await;
    let second = batch(&app, token, flour, "5").await;

    let (status, body) = apply_count(
        &app,
        token,
        json!({
            "counts": [
                { "batch_id": first, "qty": "10" },
                { "batch_id": second, "qty": "5" },
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], 0);
    assert_eq!(body["unchanged"], 2);
    assert!(movements(&pool, first).await.is_empty());
    assert!(movements(&pool, second).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_count_rejects_a_negative_or_repeated_batch(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = create_material(&app, token, "Tepung").await;
    let first = batch(&app, token, flour, "10").await;

    let (status, body) = apply_count(
        &app,
        token,
        json!({ "counts": [{ "batch_id": first, "qty": "-1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = apply_count(
        &app,
        token,
        json!({ "counts": [
            { "batch_id": first, "qty": "9" },
            { "batch_id": first, "qty": "8" },
        ] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, _) = apply_count(&app, token, json!({ "counts": [] })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(
        remaining(&pool, first).await,
        "10.000",
        "nothing was written"
    );
    assert!(movements(&pool, first).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_count_is_scoped_to_the_tenants_own_batches(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");

    let flour = create_material(&app, token, "Tepung").await;
    let mine = batch(&app, token, flour, "10").await;

    let theirs_material = create_material(&app, other_token, "Cabai").await;
    let theirs = batch(&app, other_token, theirs_material, "4").await;

    assert_ne!(
        tenant_of(&app, token).await,
        tenant_of(&app, other_token).await
    );

    let (status, body) = apply_count(
        &app,
        other_token,
        json!({ "counts": [{ "batch_id": mine, "qty": "2" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(remaining(&pool, mine).await, "10.000");

    let (status, body) = apply_count(
        &app,
        token,
        json!({ "counts": [{ "batch_id": theirs, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(remaining(&pool, theirs).await, "4.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_sheet_is_written_whole_or_not_at_all(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = create_material(&app, token, "Tepung").await;
    let first = batch(&app, token, flour, "10").await;
    let second = batch(&app, token, flour, "5").await;

    // The second row cannot be written, so the first one must not be either:
    // a half-recorded count is worse than no count, because the ledger would
    // disagree with a sheet the owner believes was applied.
    let (status, body) = apply_count(
        &app,
        token,
        json!({ "counts": [
            { "batch_id": first, "qty": "9" },
            { "batch_id": Uuid::new_v4(), "qty": "1" },
        ] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    assert_eq!(remaining(&pool, first).await, "10.000");
    assert_eq!(remaining(&pool, second).await, "5.000");
    assert!(movements(&pool, first).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_count_needs_the_stock_adjust_permission(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant = tenant_of(&app, token).await;

    let flour = create_material(&app, token, "Tepung").await;
    let first = batch(&app, token, flour, "10").await;

    add_member(&pool, tenant, "viewer@example.test", "Viewer", "viewer").await;
    let viewer = login(&app, &otp, "viewer@example.test").await;
    let viewer_token = viewer["access_token"].as_str().expect("token");

    let (status, body) = apply_count(
        &app,
        viewer_token,
        json!({ "counts": [{ "batch_id": first, "qty": "9" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(remaining(&pool, first).await, "10.000");

    let (status, _) = apply_count(&app, "", json!({ "counts": [] })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
