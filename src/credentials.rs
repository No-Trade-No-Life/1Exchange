use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    AppError, AppState, auth,
    exchanges::{credential_required_fields, is_supported_exchange},
};

#[derive(Debug, Deserialize)]
pub struct CreateCredentialRequest {
    pub exchange: String,
    pub name: String,
    pub payload: Value,
}

#[derive(Debug, Deserialize)]
pub struct DeleteCredentialQuery {
    pub credential_id: String,
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

pub struct StoredCredential {
    pub id: String,
    pub exchange: String,
    pub payload: Value,
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
    headers: HeaderMap,
) -> Result<Json<Vec<CredentialMeta>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query_as::<_, CredentialRow>(
        r#"
        SELECT id, owner_id, exchange, name, payload, created_at, updated_at
        FROM credentials
        WHERE owner_id = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(CredentialMeta::from).collect()))
}

pub async fn create_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCredentialRequest>,
) -> Result<(StatusCode, Json<CredentialMeta>), AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    if !is_supported_exchange(&request.exchange) {
        return Err(AppError::bad_request("unsupported exchange"));
    }

    validate_payload(&request.exchange, &request.payload)?;

    let id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&request.payload)?;

    sqlx::query(
        r#"
        INSERT INTO credentials (id, owner_id, exchange, name, payload)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&id)
    .bind(&user.user_id)
    .bind(&request.exchange)
    .bind(&request.name)
    .bind(payload)
    .execute(&state.db)
    .await?;

    let credential = get_credential_meta(&state.db, &user.user_id, &id).await?;
    Ok((StatusCode::CREATED, Json(credential)))
}

pub async fn delete_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DeleteCredentialQuery>,
) -> Result<StatusCode, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    sqlx::query(
        r#"
        DELETE FROM credentials
        WHERE id = ?1 AND owner_id = ?2
        "#,
    )
    .bind(query.credential_id)
    .bind(&user.user_id)
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_stored_credential(
    db: &SqlitePool,
    owner_id: &str,
    id: &str,
) -> Result<StoredCredential, AppError> {
    let row = sqlx::query_as::<_, CredentialRow>(
        r#"
        SELECT id, owner_id, exchange, name, payload, created_at, updated_at
        FROM credentials
        WHERE id = ?1 AND owner_id = ?2
        "#,
    )
    .bind(id)
    .bind(owner_id)
    .fetch_one(db)
    .await?;

    Ok(StoredCredential {
        id: row.id,
        exchange: row.exchange,
        payload: serde_json::from_str(&row.payload)?,
    })
}

pub async fn list_stored_credentials(
    db: &SqlitePool,
    owner_id: &str,
) -> Result<Vec<StoredCredential>, AppError> {
    let rows = sqlx::query_as::<_, CredentialRow>(
        r#"
        SELECT id, owner_id, exchange, name, payload, created_at, updated_at
        FROM credentials
        WHERE owner_id = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(owner_id)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(StoredCredential {
                id: row.id,
                exchange: row.exchange,
                payload: serde_json::from_str(&row.payload)?,
            })
        })
        .collect()
}

fn validate_payload(exchange: &str, payload: &Value) -> Result<(), AppError> {
    let required_fields = credential_required_fields(exchange)
        .ok_or_else(|| AppError::bad_request("unsupported exchange"))?;

    for field in required_fields {
        let value = payload
            .get(*field)
            .and_then(Value::as_str)
            .unwrap_or_default();
        if value.is_empty() {
            return Err(AppError::bad_request(format!(
                "missing credential field: {field}"
            )));
        }
    }

    Ok(())
}

async fn get_credential_meta(
    db: &SqlitePool,
    owner_id: &str,
    id: &str,
) -> Result<CredentialMeta, AppError> {
    let row = sqlx::query_as::<_, CredentialRow>(
        r#"
        SELECT id, owner_id, exchange, name, payload, created_at, updated_at
        FROM credentials
        WHERE id = ?1 AND owner_id = ?2
        "#,
    )
    .bind(id)
    .bind(owner_id)
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
