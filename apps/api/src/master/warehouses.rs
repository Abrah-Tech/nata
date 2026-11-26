//! Warehouse master data.
//!
//! A warehouse is a storage location a batch can be assigned to. It is tenant
//! setup rather than part of the material catalogue, so writing one needs the
//! tenant management permission while reading one only needs the material read
//! that the stock and production screens already hold.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::guard::{CanManageTenant, CanReadMaterials, Require};
use crate::subscription::ensure_capacity;
use crate::{error::ApiError, AppState};
use nata_domain::plan::Resource;

#[derive(Debug, Serialize)]
pub struct WarehouseView {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct WarehouseBody {
    pub name: String,
}

type WarehouseRow = (Uuid, String, DateTime<Utc>);

const SELECT: &str = "select id, name, created_at";

fn view(row: WarehouseRow) -> WarehouseView {
    let (id, name, created_at) = row;

    WarehouseView {
        id,
        name,
        created_at,
    }
}

fn validate(body: &WarehouseBody) -> Result<&str, ApiError> {
    let name = body.name.trim();

    if name.is_empty() {
        return Err(ApiError::BadRequest("name_required"));
    }

    Ok(name)
}

pub async fn list_warehouses(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<Vec<WarehouseView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<WarehouseRow> = sqlx::query_as(&format!(
        "{SELECT} from warehouses where tenant_id = $1 order by name"
    ))
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(rows.into_iter().map(view).collect()))
}

pub async fn create_warehouse(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Json(body): Json<WarehouseBody>,
) -> Result<(StatusCode, Json<WarehouseView>), ApiError> {
    let auth = guard.auth;
    let name = validate(&body)?;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    ensure_capacity(&mut tx, auth.tenant_id, Resource::Warehouses).await?;

    let row: WarehouseRow = sqlx::query_as(
        "insert into warehouses (tenant_id, name) values ($1, $2)
         returning id, name, created_at",
    )
    .bind(auth.tenant_id)
    .bind(name)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "warehouse_exists"))?;

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(view(row))))
}

pub async fn update_warehouse(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Path(id): Path<Uuid>,
    Json(body): Json<WarehouseBody>,
) -> Result<Json<WarehouseView>, ApiError> {
    let auth = guard.auth;
    let name = validate(&body)?;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let row: Option<WarehouseRow> = sqlx::query_as(
        "update warehouses set name = $3
         where id = $1 and tenant_id = $2
         returning id, name, created_at",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .bind(name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "warehouse_exists"))?;

    let row = row.ok_or(ApiError::NotFound("warehouse_not_found"))?;
    tx.commit().await?;

    Ok(Json(view(row)))
}

pub async fn remove_warehouse(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // A hard delete, not a flag: a warehouse with no stock is gone, and one
    // that still has a batch or a movement is refused by the database so the
    // stock is reassigned first rather than rewritten.
    let result = sqlx::query("delete from warehouses where id = $1 and tenant_id = $2")
        .bind(id)
        .bind(auth.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(database) if database.code().as_deref() == Some("23503") => {
                ApiError::Conflict("warehouse_in_use")
            }
            _ => ApiError::Internal(error.into()),
        })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("warehouse_not_found"));
    }

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}
