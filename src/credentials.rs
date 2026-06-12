use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{AppError, AppState, exchanges::is_supported_exchange};

#[derive(Debug, Deserialize)]
pub struct CreateCredentialRequest {
    pub exchange: String,
    pub name: String,
    pub payload: Value,
}

#[derive(Debug, Serialize)]
pub struct CredentialMeta {
    pub id: String,
    pub exchange: String,
    pub name: String,
    pub has_payload: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
struct CredentialRow {
    id: String,
    exchange: String,
    name: String,
    payload: String,
    created_at: String,
    updated_at: String,
}

pub async fn list_credentials(
    State(state): State<AppState>,
) -> Result<Json<Vec<CredentialMeta>>, AppError> {
    let rows = sqlx::query_as::<_, CredentialRow>(
        r#"
        SELECT id, exchange, name, payload, created_at, updated_at
        FROM credentials
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(CredentialMeta::from).collect()))
}

pub async fn create_credential(
    State(state): State<AppState>,
    Json(request): Json<CreateCredentialRequest>,
) -> Result<(StatusCode, Json<CredentialMeta>), AppError> {
    if !is_supported_exchange(&request.exchange) {
        return Err(AppError::bad_request("unsupported exchange"));
    }

    let id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&request.payload)?;

    sqlx::query(
        r#"
        INSERT INTO credentials (id, exchange, name, payload)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(&id)
    .bind(&request.exchange)
    .bind(&request.name)
    .bind(payload)
    .execute(&state.db)
    .await?;

    let credential = get_credential_meta(&state.db, &id).await?;
    Ok((StatusCode::CREATED, Json(credential)))
}

async fn get_credential_meta(db: &SqlitePool, id: &str) -> Result<CredentialMeta, AppError> {
    let row = sqlx::query_as::<_, CredentialRow>(
        r#"
        SELECT id, exchange, name, payload, created_at, updated_at
        FROM credentials
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await?;

    Ok(row.into())
}

impl From<CredentialRow> for CredentialMeta {
    fn from(row: CredentialRow) -> Self {
        Self {
            id: row.id,
            exchange: row.exchange,
            name: row.name,
            has_payload: !row.payload.is_empty(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
