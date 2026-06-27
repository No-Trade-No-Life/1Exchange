use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{AppError, AppState, models::AccountInfo};

#[derive(Debug, Deserialize)]
pub struct CreateCustomAccountSourceRequest {
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomAccountSourceConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
struct CustomAccountSourceRow {
    id: String,
    name: String,
    base_url: String,
    enabled: i64,
    created_at: String,
    updated_at: String,
}

pub async fn list_custom_account_sources(
    State(state): State<AppState>,
) -> Result<Json<Vec<CustomAccountSourceConfig>>, AppError> {
    Ok(Json(list_custom_account_source_configs(&state.db).await?))
}

pub async fn create_custom_account_source(
    State(state): State<AppState>,
    Json(request): Json<CreateCustomAccountSourceRequest>,
) -> Result<(StatusCode, Json<CustomAccountSourceConfig>), AppError> {
    validate_request(&request)?;

    let id = Uuid::new_v4().to_string();
    let base_url = normalized_base_url(&request.base_url);
    sqlx::query(
        r#"
        INSERT INTO custom_account_sources (id, name, base_url, enabled)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(&id)
    .bind(&request.name)
    .bind(base_url)
    .bind(if request.enabled { 1 } else { 0 })
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(get_custom_account_source_config(&state.db, &id).await?),
    ))
}

pub async fn list_custom_account_source_configs(
    db: &SqlitePool,
) -> Result<Vec<CustomAccountSourceConfig>, AppError> {
    let rows = sqlx::query_as::<_, CustomAccountSourceRow>(
        r#"
        SELECT id, name, base_url, enabled, created_at, updated_at
        FROM custom_account_sources
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(CustomAccountSourceConfig::from)
        .collect())
}

pub async fn discover_accounts(
    sources: &[CustomAccountSourceConfig],
) -> Result<Vec<AccountInfo>, AppError> {
    let mut accounts = Vec::new();
    for source in sources.iter().filter(|source| source.enabled) {
        accounts.extend(fetch_accounts(&source.base_url).await?);
    }

    Ok(accounts)
}

pub async fn read_account(
    sources: &[CustomAccountSourceConfig],
    account_id: &str,
) -> Result<Option<AccountInfo>, AppError> {
    for source in sources.iter().filter(|source| source.enabled) {
        let accounts = fetch_account(&source.base_url, account_id).await?;
        if let Some(account) = accounts.into_iter().next() {
            return Ok(Some(account));
        }
    }

    Ok(None)
}

async fn get_custom_account_source_config(
    db: &SqlitePool,
    id: &str,
) -> Result<CustomAccountSourceConfig, AppError> {
    let row = sqlx::query_as::<_, CustomAccountSourceRow>(
        r#"
        SELECT id, name, base_url, enabled, created_at, updated_at
        FROM custom_account_sources
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await?;

    Ok(row.into())
}

async fn fetch_accounts(base_url: &str) -> Result<Vec<AccountInfo>, AppError> {
    fetch_remote_accounts(&format!("{}/api/accounts", base_url)).await
}

async fn fetch_account(base_url: &str, account_id: &str) -> Result<Vec<AccountInfo>, AppError> {
    fetch_remote_accounts(&format!(
        "{}/api/accounts?account_id={}",
        base_url,
        urlencoding::encode(account_id)
    ))
    .await
}

async fn fetch_remote_accounts(url: &str) -> Result<Vec<AccountInfo>, AppError> {
    let response = reqwest::get(url).await?;
    if !response.status().is_success() {
        return Err(AppError::bad_request(format!(
            "custom account source returned {}",
            response.status()
        )));
    }

    Ok(response
        .json::<Vec<AccountInfo>>()
        .await?
        .into_iter()
        .map(AccountInfo::normalized)
        .collect())
}

fn validate_request(request: &CreateCustomAccountSourceRequest) -> Result<(), AppError> {
    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("missing custom account source name"));
    }
    if request.base_url.trim().is_empty() {
        return Err(AppError::bad_request(
            "missing custom account source base_url",
        ));
    }
    let base_url = request.base_url.trim();
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(AppError::bad_request(
            "custom account source base_url must start with http:// or https://",
        ));
    }

    Ok(())
}

fn normalized_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

impl From<CustomAccountSourceRow> for CustomAccountSourceConfig {
    fn from(row: CustomAccountSourceRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            base_url: row.base_url,
            enabled: row.enabled != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
