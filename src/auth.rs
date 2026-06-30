use axum::http::{HeaderMap, header};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{AppError, AppState};

pub const ADMIN_USER_ID_META_KEY: &str = "admin_user_id";
pub const LEGACY_OWNER_ID: &str = "00000000-0000-4000-8000-000000000000";

#[derive(Clone, Debug, Serialize)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SetupState {
    pub initialized: bool,
    pub admin_user_id: Option<String>,
    pub is_admin: bool,
    pub legacy_records: i64,
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    user_id: String,
    email: String,
}

pub async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, AppError> {
    let token = bearer_token(headers)?;
    let response = reqwest::Client::new()
        .get(format!(
            "{}/me",
            state.auth_mini_origin.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::unauthorized("invalid auth-mini access token"));
    }
    if !response.status().is_success() {
        return Err(AppError::bad_gateway(anyhow::anyhow!(
            "auth-mini /me returned {}",
            response.status()
        )));
    }

    let me = response.json::<MeResponse>().await?;
    if Uuid::parse_str(&me.user_id).is_err() {
        return Err(AppError::unauthorized("auth-mini user_id is not a UUID"));
    }

    Ok(AuthUser {
        user_id: me.user_id,
        email: me.email,
    })
}

pub async fn require_initialized_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthUser, AppError> {
    let user = require_user(state, headers).await?;
    if admin_user_id(&state.db).await?.is_none() {
        return Err(AppError::conflict("1Exchange setup is required"));
    }

    Ok(user)
}

pub async fn setup_state(db: &SqlitePool, user: &AuthUser) -> Result<SetupState, AppError> {
    let admin_user_id = admin_user_id(db).await?;
    let legacy_records = legacy_record_count(db).await?;
    let is_admin = admin_user_id
        .as_deref()
        .map(|admin_user_id| admin_user_id == user.user_id)
        .unwrap_or(false);

    Ok(SetupState {
        initialized: admin_user_id.is_some(),
        admin_user_id,
        is_admin,
        legacy_records,
    })
}

pub async fn initialize_setup(db: &SqlitePool, user: &AuthUser) -> Result<SetupState, AppError> {
    let mut tx = db.begin().await?;
    let changed = sqlx::query(
        r#"
        INSERT OR IGNORE INTO app_meta (key, value, updated_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(ADMIN_USER_ID_META_KEY)
    .bind(&user.user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if changed == 0 {
        return Err(AppError::conflict("1Exchange is already initialized"));
    }

    for table in [
        "credentials",
        "virtual_accounts",
        "custom_account_sources",
        "funds",
    ] {
        let statement = format!("UPDATE {table} SET owner_id = ?1 WHERE owner_id = ?2");
        sqlx::query(&statement)
            .bind(&user.user_id)
            .bind(LEGACY_OWNER_ID)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    setup_state(db, user).await
}

async fn admin_user_id(db: &SqlitePool) -> Result<Option<String>, AppError> {
    let row = sqlx::query("SELECT value FROM app_meta WHERE key = ?1")
        .bind(ADMIN_USER_ID_META_KEY)
        .fetch_optional(db)
        .await?;

    Ok(row.map(|row| row.get::<String, _>("value")))
}

async fn legacy_record_count(db: &SqlitePool) -> Result<i64, AppError> {
    let mut count = 0;
    for table in [
        "credentials",
        "virtual_accounts",
        "custom_account_sources",
        "funds",
    ] {
        let statement = format!("SELECT COUNT(*) AS count FROM {table} WHERE owner_id = ?1");
        let (table_count,): (i64,) = sqlx::query_as(&statement)
            .bind(LEGACY_OWNER_ID)
            .fetch_one(db)
            .await?;
        count += table_count;
    }

    Ok(count)
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, AppError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("missing Authorization bearer token"))?;
    let token = value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AppError::unauthorized("missing Authorization bearer token"))?;

    Ok(token)
}
