use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::{AppError, AppState, auth, credentials, exchanges, models::AccountInfo};

#[derive(Debug, Deserialize)]
pub struct CreateVirtualAccountRequest {
    pub account_id: String,
    pub name: String,
    pub enabled: bool,
    pub sources: Vec<VirtualAccountSource>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VirtualAccountSource {
    pub credential_id: String,
    pub coefficient: f64,
    pub enabled: bool,
    pub force_zero: bool,
}

#[derive(Debug, Serialize)]
pub struct VirtualAccountConfig {
    pub account_id: String,
    pub name: String,
    pub enabled: bool,
    pub sources: Vec<VirtualAccountSource>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
struct VirtualAccountRow {
    account_id: String,
    name: String,
    enabled: i64,
    sources: String,
    created_at: String,
    updated_at: String,
}

pub async fn list_virtual_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<VirtualAccountConfig>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    Ok(Json(
        list_virtual_account_configs(&state.db, &user.user_id).await?,
    ))
}

pub async fn list_virtual_account_configs(
    db: &SqlitePool,
    owner_id: &str,
) -> Result<Vec<VirtualAccountConfig>, AppError> {
    let rows = sqlx::query_as::<_, VirtualAccountRow>(
        r#"
        SELECT account_id, owner_id, name, enabled, sources, created_at, updated_at
        FROM virtual_accounts
        WHERE owner_id = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(owner_id)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(VirtualAccountConfig::try_from)
        .collect::<Result<Vec<_>, _>>()
}

pub async fn create_virtual_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateVirtualAccountRequest>,
) -> Result<(StatusCode, Json<VirtualAccountConfig>), AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    validate_request(&request)?;

    let sources = serde_json::to_string(&request.sources)?;
    sqlx::query(
        r#"
        INSERT INTO virtual_accounts (account_id, owner_id, name, enabled, sources)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(account_id) DO UPDATE SET
            name = excluded.name,
            enabled = excluded.enabled,
            sources = excluded.sources,
            updated_at = CURRENT_TIMESTAMP
        WHERE virtual_accounts.owner_id = excluded.owner_id
        "#,
    )
    .bind(&request.account_id)
    .bind(&user.user_id)
    .bind(&request.name)
    .bind(if request.enabled { 1 } else { 0 })
    .bind(sources)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(get_virtual_account_config(&state.db, &user.user_id, &request.account_id).await?),
    ))
}

pub async fn compose_virtual_account_by_id(
    db: &SqlitePool,
    owner_id: &str,
    account_id: &str,
) -> Result<Option<AccountInfo>, AppError> {
    let Some(config) = find_virtual_account_config(db, owner_id, account_id).await? else {
        return Ok(None);
    };
    if !config.enabled {
        return Err(AppError::bad_request("virtual account is disabled"));
    }

    Ok(Some(
        compose_virtual_account_config(db, owner_id, config).await?,
    ))
}

pub async fn require_virtual_account(
    db: &SqlitePool,
    owner_id: &str,
    account_id: &str,
) -> Result<(), AppError> {
    find_virtual_account_config(db, owner_id, account_id)
        .await?
        .ok_or_else(|| {
            AppError::bad_request(
                "fund account must be a virtual account owned by the current user",
            )
        })?;

    Ok(())
}

async fn find_virtual_account_config(
    db: &SqlitePool,
    owner_id: &str,
    account_id: &str,
) -> Result<Option<VirtualAccountConfig>, AppError> {
    let row = sqlx::query_as::<_, VirtualAccountRow>(
        r#"
        SELECT account_id, owner_id, name, enabled, sources, created_at, updated_at
        FROM virtual_accounts
        WHERE account_id = ?1 AND owner_id = ?2
        "#,
    )
    .bind(account_id)
    .bind(owner_id)
    .fetch_optional(db)
    .await?;

    row.map(VirtualAccountConfig::try_from).transpose()
}

async fn get_virtual_account_config(
    db: &SqlitePool,
    owner_id: &str,
    account_id: &str,
) -> Result<VirtualAccountConfig, AppError> {
    let row = sqlx::query_as::<_, VirtualAccountRow>(
        r#"
        SELECT account_id, owner_id, name, enabled, sources, created_at, updated_at
        FROM virtual_accounts
        WHERE account_id = ?1 AND owner_id = ?2
        "#,
    )
    .bind(account_id)
    .bind(owner_id)
    .fetch_one(db)
    .await?;

    VirtualAccountConfig::try_from(row)
}

async fn compose_virtual_account_config(
    db: &SqlitePool,
    owner_id: &str,
    config: VirtualAccountConfig,
) -> Result<AccountInfo, AppError> {
    let mut positions = Vec::new();

    for source in config.sources.into_iter().filter(|source| source.enabled) {
        let credential =
            credentials::get_stored_credential(db, owner_id, &source.credential_id).await?;
        let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
            AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
        })?;
        let account = adapter
            .get_account(&credential.payload)
            .await
            .map_err(AppError::bad_gateway)?;
        let coefficient = if source.force_zero {
            0.0
        } else {
            source.coefficient
        };

        let source_account_id = account.account_id.clone();
        positions.extend(
            account
                .normalized()
                .positions
                .into_iter()
                .map(|mut position| {
                    position.position_id = format!("{source_account_id}:{}", position.position_id);
                    position.scale(coefficient);
                    position
                }),
        );
    }

    Ok(AccountInfo {
        account_id: config.account_id,
        positions,
        orders: Vec::new(),
        timestamp_in_us: Utc::now().timestamp_micros(),
    }
    .normalized())
}

fn validate_request(request: &CreateVirtualAccountRequest) -> Result<(), AppError> {
    if request.account_id.trim().is_empty() {
        return Err(AppError::bad_request("missing virtual account id"));
    }
    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("missing virtual account name"));
    }
    if request.sources.is_empty() {
        return Err(AppError::bad_request(
            "virtual account requires at least one source",
        ));
    }
    for source in &request.sources {
        if source.credential_id.trim().is_empty() {
            return Err(AppError::bad_request("missing source credential id"));
        }
        if !source.coefficient.is_finite() {
            return Err(AppError::bad_request("source coefficient must be finite"));
        }
    }

    Ok(())
}

impl TryFrom<VirtualAccountRow> for VirtualAccountConfig {
    type Error = AppError;

    fn try_from(row: VirtualAccountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            account_id: row.account_id,
            name: row.name,
            enabled: row.enabled != 0,
            sources: serde_json::from_str(&row.sources)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite_coefficients() {
        let request = CreateVirtualAccountRequest {
            account_id: "virtual/test".to_string(),
            name: "test".to_string(),
            enabled: true,
            sources: vec![VirtualAccountSource {
                credential_id: "credential".to_string(),
                coefficient: f64::NAN,
                enabled: true,
                force_zero: false,
            }],
        };

        assert!(validate_request(&request).is_err());
    }
}
