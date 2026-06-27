mod credentials;
mod custom_account_sources;
mod exchanges;
mod funds;
mod models;
mod rates;
mod virtual_accounts;

use std::{
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    str::FromStr,
};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{OriginalUri, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use models::{AccountInfo, Position, Product, TradeFill};
use serde::{Serialize, ser::SerializeStruct};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    db_path: PathBuf,
    pub db: SqlitePool,
    vite_origin: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    database: String,
}

#[derive(Serialize)]
struct AccountRef {
    credential_id: String,
    account_id: Option<String>,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct ProductsQuery {
    exchange: Option<String>,
}

#[derive(serde::Deserialize)]
struct AccountQuery {
    credential_id: Option<String>,
    account_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct CredentialQuery {
    credential_id: String,
}

#[derive(serde::Deserialize)]
struct RatesQuery {
    target: Option<String>,
}

#[derive(serde::Deserialize)]
struct ConvertRateQuery {
    from: String,
    to: Option<String>,
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
}

struct ViteDevServer {
    addr: SocketAddr,
    child: Child,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = data_dir();
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("1ex.sqlite3");
    let db = open_database(&db_path).await?;
    migrate(&db).await?;

    let requested_addr = listen_addr()?;
    let listener = TcpListener::bind(requested_addr).await?;
    let addr = listener.local_addr()?;
    let vite = start_vite_dev_server(addr)?;
    let state = AppState {
        db_path,
        db,
        vite_origin: vite
            .as_ref()
            .map(|server| format!("http://{}", server.addr)),
    };
    funds::spawn_fund_polling(state.clone());
    let api = Router::new()
        .route("/health", get(health))
        .route("/exchanges", get(list_exchanges))
        .route(
            "/credentials",
            get(credentials::list_credentials).post(credentials::create_credential),
        )
        .route(
            "/custom-account-sources",
            get(custom_account_sources::list_custom_account_sources)
                .post(custom_account_sources::create_custom_account_source),
        )
        .route("/account-refs", get(list_account_refs))
        .route("/accounts", get(list_accounts))
        .route(
            "/virtual-accounts",
            get(virtual_accounts::list_virtual_accounts)
                .post(virtual_accounts::create_virtual_account),
        )
        .route("/positions", get(list_positions))
        .route("/funds", get(funds::list_funds).post(funds::create_fund))
        .route("/funds/sample", post(funds::sample_fund_now))
        .route("/fund-nav", get(funds::list_fund_nav))
        .route("/trades", get(list_trades))
        .route("/rates", get(list_rates))
        .route("/rates/convert", get(convert_rate))
        .route("/products", get(list_products));
    let app = Router::new().nest("/api", api);
    let app = if vite.is_some() {
        app.fallback(redirect_to_vite)
    } else {
        app.fallback_service(ServeDir::new("web/dist"))
    }
    .with_state(state);

    println!("1Exchange listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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

fn vite_addr() -> anyhow::Result<SocketAddr> {
    if let Ok(value) = std::env::var("ONE_EXCHANGE_VITE_ADDR") {
        return Ok(value.parse()?);
    }

    free_loopback_addr()
}

fn free_loopback_addr() -> anyhow::Result<SocketAddr> {
    let listener = StdTcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?)
}

fn start_vite_dev_server(api_addr: SocketAddr) -> anyhow::Result<Option<ViteDevServer>> {
    if !cfg!(debug_assertions) || vite_dev_server_disabled() {
        return Ok(None);
    }

    let addr = vite_addr()?;
    let child = Command::new("node_modules/.bin/vite")
        .current_dir("web")
        .arg("--host")
        .arg(addr.ip().to_string())
        .arg("--port")
        .arg(addr.port().to_string())
        .arg("--strictPort")
        .env("VITE_API_TARGET", format!("http://{api_addr}"))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to start Vite dev server; run npm --prefix web install first")?;

    println!("Vite dev server starting on http://{addr}");
    Ok(Some(ViteDevServer { addr, child }))
}

fn vite_dev_server_disabled() -> bool {
    matches!(
        std::env::var("ONE_EXCHANGE_VITE").as_deref(),
        Ok("0") | Ok("false") | Ok("FALSE")
    )
}

async fn shutdown_signal() {
    if let Err(error) = wait_for_shutdown_signal().await {
        eprintln!("failed to listen for shutdown signal: {error}");
    }
}

async fn redirect_to_vite(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
) -> Result<Redirect, AppError> {
    let vite_origin = state
        .vite_origin
        .ok_or_else(|| AppError::bad_request("Vite dev server is not running"))?;
    let path = uri
        .path_and_query()
        .map(|path| path.as_str())
        .unwrap_or("/");

    Ok(Redirect::temporary(&format!("{vite_origin}{path}")))
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {},
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    tokio::signal::ctrl_c().await?;
    Ok(())
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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS virtual_accounts (
            account_id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            sources TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS custom_account_sources (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS funds (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            account_id TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            target_currency TEXT NOT NULL DEFAULT 'USD',
            poll_interval_seconds INTEGER NOT NULL DEFAULT 600,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_funds_account_id
        ON funds (account_id)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_nav_snapshots (
            id TEXT PRIMARY KEY NOT NULL,
            fund_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            equity REAL NOT NULL,
            target_currency TEXT NOT NULL,
            positions_count INTEGER NOT NULL,
            unpriced_positions INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_nav_snapshots_fund_created
        ON fund_nav_snapshots (fund_id, created_at DESC)
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

async fn list_account_refs(
    State(state): State<AppState>,
) -> Result<Json<Vec<AccountRef>>, AppError> {
    let mut refs = Vec::new();
    for credential in credentials::list_stored_credentials(&state.db).await? {
        let result = match exchanges::adapter(&credential.exchange) {
            Some(adapter) => adapter.get_account_id(&credential.payload).await,
            None => Err(anyhow::anyhow!(
                "unsupported exchange: {}",
                credential.exchange
            )),
        };
        let (account_id, error) = match result {
            Ok(account_id) => (Some(account_id), None),
            Err(error) => (None, Some(error.to_string())),
        };
        refs.push(AccountRef {
            credential_id: credential.id,
            account_id,
            error,
        });
    }

    Ok(Json(refs))
}

async fn list_accounts(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Vec<AccountInfo>>, AppError> {
    if query.credential_id.is_none() && query.account_id.is_none() {
        return Ok(Json(read_all_accounts(&state.db).await?));
    }

    Ok(Json(vec![read_account(&state.db, query).await?]))
}

async fn list_positions(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Vec<Position>>, AppError> {
    Ok(Json(read_account(&state.db, query).await?.positions))
}

async fn read_account(db: &SqlitePool, query: AccountQuery) -> Result<AccountInfo, AppError> {
    if let Some(credential_id) = query.credential_id {
        return read_credential_account(db, &credential_id).await;
    }
    if let Some(account_id) = query.account_id {
        return read_account_by_account_id(db, &account_id).await;
    }

    Err(AppError::bad_request(
        "missing credential_id or account_id query",
    ))
}

async fn read_credential_account(
    db: &SqlitePool,
    credential_id: &str,
) -> Result<AccountInfo, AppError> {
    let credential = credentials::get_stored_credential(db, credential_id).await?;
    let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
        AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
    })?;

    adapter
        .get_account(&credential.payload)
        .await
        .map(AccountInfo::normalized)
        .map_err(AppError::bad_gateway)
}

async fn read_account_by_account_id(
    db: &SqlitePool,
    account_id: &str,
) -> Result<AccountInfo, AppError> {
    if let Some(account) = virtual_accounts::compose_virtual_account_by_id(db, account_id).await? {
        return Ok(account);
    }

    for credential in credentials::list_stored_credentials(db).await? {
        let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
            AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
        })?;
        let local_account_id = adapter
            .get_account_id(&credential.payload)
            .await
            .map_err(AppError::bad_gateway)?;
        if local_account_id == account_id {
            return adapter
                .get_account(&credential.payload)
                .await
                .map(AccountInfo::normalized)
                .map_err(AppError::bad_gateway);
        }
    }

    let sources = custom_account_sources::list_custom_account_source_configs(db).await?;
    if let Some(account) = custom_account_sources::read_account(&sources, account_id).await? {
        return Ok(account);
    }

    Err(AppError::bad_request(format!(
        "account not found: {account_id}"
    )))
}

async fn read_all_accounts(db: &SqlitePool) -> Result<Vec<AccountInfo>, AppError> {
    let mut accounts = Vec::new();
    for credential in credentials::list_stored_credentials(db).await? {
        let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
            AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
        })?;
        accounts.push(
            adapter
                .get_account(&credential.payload)
                .await
                .map(AccountInfo::normalized)
                .map_err(AppError::bad_gateway)?,
        );
    }

    for config in virtual_accounts::list_virtual_account_configs(db)
        .await?
        .into_iter()
        .filter(|config| config.enabled)
    {
        if let Some(account) =
            virtual_accounts::compose_virtual_account_by_id(db, &config.account_id).await?
        {
            accounts.push(account);
        }
    }

    let sources = custom_account_sources::list_custom_account_source_configs(db).await?;
    accounts.extend(
        custom_account_sources::discover_accounts(&sources)
            .await?
            .into_iter()
            .map(AccountInfo::normalized),
    );

    Ok(accounts)
}

async fn list_trades(
    State(state): State<AppState>,
    Query(query): Query<CredentialQuery>,
) -> Result<Json<Vec<TradeFill>>, AppError> {
    let credential = credentials::get_stored_credential(&state.db, &query.credential_id).await?;
    let adapter = exchanges::adapter(&credential.exchange).ok_or_else(|| {
        AppError::bad_request(format!("unsupported exchange: {}", credential.exchange))
    })?;

    Ok(Json(
        adapter
            .list_trades(&credential.payload)
            .await
            .map_err(AppError::bad_gateway)?,
    ))
}

async fn list_rates(Query(query): Query<RatesQuery>) -> Json<rates::CurrencyRateSnapshot> {
    Json(rates::snapshot(query.target.as_deref().unwrap_or("USD")))
}

async fn convert_rate(Query(query): Query<ConvertRateQuery>) -> Json<rates::CurrencyConversion> {
    let target = query.to.as_deref().unwrap_or("USD");
    let snapshot = rates::snapshot(target);

    Json(rates::conversion(&snapshot.edges, &query.from, target))
}

async fn list_products(Query(query): Query<ProductsQuery>) -> Result<Json<Vec<Product>>, AppError> {
    let exchange = query
        .exchange
        .ok_or_else(|| AppError::bad_request("missing exchange query"))?;
    let adapter = exchanges::adapter(&exchange)
        .ok_or_else(|| AppError::bad_request(format!("unsupported exchange: {exchange}")))?;

    Ok(Json(
        adapter
            .list_products()
            .await
            .map_err(AppError::bad_gateway)?,
    ))
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn bad_gateway(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: error.to_string(),
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

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }
}

impl Drop for ViteDevServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
