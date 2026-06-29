use axum::http::{HeaderMap, header};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, AppState};

#[derive(Clone, Debug, Serialize)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
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
