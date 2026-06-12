mod credentials;
mod exchanges;
mod models;

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use models::{AccountInfo, Position, Product};
use serde::{Serialize, ser::SerializeStruct};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    db_path: PathBuf,
    pub db: SqlitePool,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    database: String,
}

#[derive(serde::Deserialize)]
struct ProductsQuery {
    exchange: Option<String>,
}

#[derive(serde::Deserialize)]
struct CredentialQuery {
    credential_id: String,
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = data_dir();
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("1ex.sqlite3");
    let db = open_database(&db_path).await?;
    migrate(&db).await?;

    let state = AppState { db_path, db };
    let api = Router::new()
        .route("/health", get(health))
        .route("/exchanges", get(list_exchanges))
        .route(
            "/credentials",
            get(credentials::list_credentials).post(credentials::create_credential),
        )
        .route("/accounts", get(list_accounts))
        .route("/positions", get(list_positions))
        .route("/products", get(list_products))
        .with_state(state);
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("web/dist"));

    let addr = listen_addr()?;
    let listener = TcpListener::bind(addr).await?;
    println!("1Exchange listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

fn data_dir() -> PathBuf {
    std::env::var_os("ONE_EXCHANGE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("home directory is required to locate ~/.1ex")
                .join(".1ex")
        })
}

fn listen_addr() -> anyhow::Result<SocketAddr> {
    let value = std::env::var("ONE_EXCHANGE_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    Ok(value.parse()?)
}

async fn open_database(db_path: &Path) -> anyhow::Result<SqlitePool> {
    let database_url = format!("sqlite://{}", db_path.display());
    let options = SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true);
    Ok(SqlitePool::connect_with(options).await?)
}

async fn migrate(db: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS credentials (
            id TEXT PRIMARY KEY NOT NULL,
            exchange TEXT NOT NULL,
            name TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_credentials_exchange
        ON credentials (exchange)
        "#,
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let _db = &state.db;

    Json(HealthResponse {
        status: "ok",
        database: state.db_path.display().to_string(),
    })
}

async fn list_exchanges() -> Json<Vec<exchanges::ExchangeInfo>> {
    Json(exchanges::list_exchanges())
}

async fn list_accounts(
    State(state): State<AppState>,
    Query(query): Query<CredentialQuery>,
) -> Result<Json<Vec<AccountInfo>>, AppError> {
    let credential = credentials::get_stored_credential(&state.db, &query.credential_id).await?;
    let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
        AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
    })?;

    Ok(Json(vec![adapter.get_account(&credential.payload).await?]))
}

async fn list_positions(
    State(state): State<AppState>,
    Query(query): Query<CredentialQuery>,
) -> Result<Json<Vec<Position>>, AppError> {
    let credential = credentials::get_stored_credential(&state.db, &query.credential_id).await?;
    let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
        AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
    })?;

    Ok(Json(adapter.list_positions(&credential.payload).await?))
}

async fn list_products(Query(query): Query<ProductsQuery>) -> Result<Json<Vec<Product>>, AppError> {
    let exchange = query
        .exchange
        .ok_or_else(|| AppError::bad_request("missing exchange query"))?;
    let adapter = exchanges::adapter(&exchange)
        .ok_or_else(|| AppError::bad_request(format!("unsupported exchange: {exchange}")))?;

    Ok(Json(adapter.list_products().await?))
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(self)).into_response()
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("AppError", 1)?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}
