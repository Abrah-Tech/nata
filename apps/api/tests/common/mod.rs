#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use nata_api::{
    auth::{contact::Contact, jwt::JwtConfig, otp::OtpSender},
    router, AppState,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub const SECRET: &str = "test-secret";

/// A date `days` from today, formatted for both the API and the database.
///
/// A batch used in a test has to be inside its shelf life when the test runs,
/// so a fixed date quietly expires as the calendar moves and the test starts
/// failing for a reason that has nothing to do with the code.
pub fn date_from_now(days: i64) -> String {
    (chrono::Utc::now().date_naive() + chrono::Duration::days(days)).to_string()
}

#[derive(Clone, Default)]
pub struct CaptureOtp {
    codes: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl OtpSender for CaptureOtp {
    async fn send(&self, contact: &Contact, code: &str, _ttl: i64) -> Result<(), String> {
        self.codes
            .lock()
            .expect("lock")
            .push((contact.as_str().to_string(), code.to_string()));
        Ok(())
    }
}

impl CaptureOtp {
    /// How many messages were actually handed to the provider. A request that
    /// reuses a pending code must not add to this.
    pub fn sent(&self) -> usize {
        self.codes.lock().expect("lock").len()
    }

    pub fn last_code(&self) -> String {
        self.codes
            .lock()
            .expect("lock")
            .last()
            .expect("an OTP was sent")
            .1
            .clone()
    }
}

pub fn state(pool: PgPool, otp: CaptureOtp) -> AppState {
    state_with_secrets(pool, otp, SECRET, None)
}

/// State whose signing secret is `current` and, when given, still accepts a
/// token minted under `previous`. Used to test a rotation without restarting
/// anything: the client keeps the token it already holds.
pub fn state_with_secrets(
    pool: PgPool,
    otp: CaptureOtp,
    current: &str,
    previous: Option<&str>,
) -> AppState {
    let mut jwt = JwtConfig::new(current.to_string(), 3_600);

    if let Some(previous) = previous {
        jwt = jwt.with_previous(previous.to_string());
    }

    AppState {
        db: Some(pool),
        jwt,
        hash_pepper: SECRET.to_string(),
        otp: Arc::new(otp),
        otp_ttl_seconds: 300,
        jwt_ttl_seconds: 3_600,
        dev_otp_code: None,
        app_url: "https://nata.test".to_string(),
    }
}

pub fn test_app(pool: PgPool, otp: CaptureOtp) -> Router {
    router(state(pool, otp))
}

pub fn test_app_with_secrets(
    pool: PgPool,
    otp: CaptureOtp,
    current: &str,
    previous: Option<&str>,
) -> Router {
    router(state_with_secrets(pool, otp, current, previous))
}

pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .expect("request");

    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, value)
}

/// Raw body, for the endpoints that take a file instead of JSON.
pub async fn call_raw(
    app: &Router,
    uri: &str,
    body: &str,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "text/csv");

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let request = builder.body(Body::from(body.to_string())).expect("request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, value)
}

/// First login for a contact with no memberships: creates the tenant and the
/// owner membership, so the response carries a usable session token.
pub async fn signup(app: &Router, otp: &CaptureOtp, contact: &str) -> Value {
    let body = login(app, otp, contact).await;

    assert_eq!(
        body["pending_invitation"], false,
        "unexpected invitation for {contact}"
    );
    assert_eq!(body["requires_tenant_selection"], false);

    body
}

/// Requests a code and verifies it without asserting the outcome.
pub async fn login(app: &Router, otp: &CaptureOtp, contact: &str) -> Value {
    let (status, _) = call(
        app,
        "POST",
        "/auth/otp/request",
        Some(json!({ "contact": contact })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let code = otp.last_code();

    let (status, body) = call(
        app,
        "POST",
        "/auth/otp/verify",
        Some(json!({
            "contact": contact,
            "code": code,
            "name": "Test Owner",
            "tenant_name": "Test Tenant"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "verify failed: {body}");

    body
}

pub async fn unit_id(app: &Router, token: &str, code: &str) -> Uuid {
    let (_, units) = call(app, "GET", "/units", None, Some(token)).await;

    units
        .as_array()
        .expect("units")
        .iter()
        .find(|unit| unit["code"] == code)
        .expect("unit")["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

pub async fn create_material(app: &Router, token: &str, name: &str) -> Uuid {
    let kg = unit_id(app, token, "kg").await;

    let (status, material) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{material}");

    material["id"].as_str().expect("id").parse().expect("uuid")
}

pub async fn stock_in(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/stock/batches", Some(body), Some(token)).await
}

pub async fn tenant_of(app: &Router, token: &str) -> Uuid {
    let (status, me) = call(app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);

    me["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .parse()
        .expect("uuid")
}

pub async fn user_of(app: &Router, token: &str) -> Uuid {
    let (status, me) = call(app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);

    me["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("uuid")
}

/// Creates a user and an active membership without going through invitations.
pub async fn add_member(
    pool: &PgPool,
    tenant_id: Uuid,
    contact: &str,
    name: &str,
    role: &str,
) -> Uuid {
    let user_id = Uuid::new_v4();

    sqlx::query("insert into users (id, email, name) values ($1, $2, $3)")
        .bind(user_id)
        .bind(contact)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert user");

    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("insert membership");

    user_id
}
