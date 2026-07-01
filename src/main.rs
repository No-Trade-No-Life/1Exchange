mod auth;
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
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use models::{AccountInfo, Position, Product, TradeFill};
use serde::{Serialize, ser::SerializeStruct};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
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
        .route("/auth/config", get(auth_config))
        .route("/me", get(me))
        .route("/setup/status", get(setup_status))
        .route("/setup/initialize", post(initialize_setup))
        .route("/exchanges", get(list_exchanges))
        .route(
            "/credentials",
            get(credentials::list_credentials)
                .post(credentials::create_credential)
                .delete(credentials::delete_credential),
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
        .route(
            "/fund-statement-events",
            get(funds::list_fund_statement_events)
                .put(funds::update_fund_statement_event)
                .delete(funds::delete_fund_statement_event),
        )
        .route("/fund-statements", get(funds::get_fund_statement_summary))
        .route(
            "/fund-settlement-preview",
            get(funds::get_fund_settlement_preview),
        )
        .route(
            "/fund-settlement-confirm",
            post(funds::confirm_fund_settlement),
        )
        .route(
            "/fund-settlement-runs",
            get(funds::list_fund_settlement_runs).post(funds::create_fund_settlement_run),
        )
        .route(
            "/fund-settlement-runs/detail",
            get(funds::get_fund_settlement_run_detail),
        )
        .route(
            "/fund-settlement-runs/export",
            get(funds::export_fund_settlement_run_csv),
        )
        .route(
            "/fund-settlement-runs/confirm",
            post(funds::confirm_fund_settlement_run),
        )
        .route(
            "/fund-settlement-runs/void",
            post(funds::void_fund_settlement_run),
        )
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

fn auth_mini_origin() -> String {
    std::env::var("ONE_EXCHANGE_AUTH_MINI_ORIGIN")
        .or_else(|_| std::env::var("VITE_AUTH_MINI_ORIGIN"))
        .unwrap_or_else(|_| auth::DEFAULT_AUTH_MINI_BASE_URL.to_string())
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
        .env("VITE_AUTH_MINI_ORIGIN", auth_mini_origin())
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
        INSERT OR IGNORE INTO app_meta (key, value, updated_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(auth::AUTH_MINI_BASE_URL_META_KEY)
    .bind(auth_mini_origin())
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS credentials (
            id TEXT PRIMARY KEY NOT NULL,
            owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000',
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
    ensure_column(
        db,
        "credentials",
        "owner_id",
        "ALTER TABLE credentials ADD COLUMN owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000'",
    )
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
        CREATE INDEX IF NOT EXISTS idx_credentials_owner_exchange
        ON credentials (owner_id, exchange)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS virtual_accounts (
            account_id TEXT PRIMARY KEY NOT NULL,
            owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000',
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
    ensure_column(
        db,
        "virtual_accounts",
        "owner_id",
        "ALTER TABLE virtual_accounts ADD COLUMN owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000'",
    )
    .await?;
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_virtual_accounts_owner_created
        ON virtual_accounts (owner_id, created_at DESC)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS custom_account_sources (
            id TEXT PRIMARY KEY NOT NULL,
            owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000',
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
    ensure_column(
        db,
        "custom_account_sources",
        "owner_id",
        "ALTER TABLE custom_account_sources ADD COLUMN owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000'",
    )
    .await?;
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_custom_account_sources_owner_created
        ON custom_account_sources (owner_id, created_at DESC)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS funds (
            id TEXT PRIMARY KEY NOT NULL,
            owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000',
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
    ensure_column(
        db,
        "funds",
        "owner_id",
        "ALTER TABLE funds ADD COLUMN owner_id TEXT NOT NULL DEFAULT '00000000-0000-4000-8000-000000000000'",
    )
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
        CREATE INDEX IF NOT EXISTS idx_funds_owner_created
        ON funds (owner_id, created_at DESC)
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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_statement_events (
            fund_id TEXT NOT NULL,
            event_index INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            payload TEXT NOT NULL,
            PRIMARY KEY (fund_id, event_index)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_statement_events_fund_updated
        ON fund_statement_events (fund_id, updated_at)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_events (
            fund_id TEXT NOT NULL,
            event_index INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            occurred_at TEXT NOT NULL,
            investor_id TEXT,
            comment TEXT,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (fund_id, event_index)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_events_fund_type_index
        ON fund_events (fund_id, event_type, event_index)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_events_fund_investor_index
        ON fund_events (fund_id, investor_id, event_index)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_reducer_snapshots (
            fund_id TEXT PRIMARY KEY NOT NULL,
            last_event_index INTEGER NOT NULL,
            state_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;

    migrate_fund_events(db).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_statement_orders (
            fund_id TEXT NOT NULL,
            event_index INTEGER NOT NULL,
            investor_name TEXT NOT NULL,
            deposit REAL NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (fund_id, event_index)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_statement_orders_fund_investor
        ON fund_statement_orders (fund_id, investor_name, updated_at)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_statement_equity (
            fund_id TEXT NOT NULL,
            event_index INTEGER NOT NULL,
            equity REAL NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (fund_id, event_index)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_statement_equity_fund_updated
        ON fund_statement_equity (fund_id, updated_at)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_statement_investors (
            fund_id TEXT NOT NULL,
            name TEXT NOT NULL,
            referrer TEXT,
            tax_rate REAL,
            referrer_rebate_rate REAL,
            tax_threshold REAL,
            updated_at TEXT NOT NULL,
            source_event_index INTEGER NOT NULL,
            PRIMARY KEY (fund_id, name)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_statement_tax_modes (
            fund_id TEXT NOT NULL,
            event_index INTEGER NOT NULL,
            mode TEXT NOT NULL,
            comment TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (fund_id, event_index)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_settlement_runs (
            id TEXT PRIMARY KEY NOT NULL,
            fund_id TEXT NOT NULL,
            settlement_model TEXT NOT NULL DEFAULT 'event_state_v1',
            equity_event_index INTEGER NOT NULL,
            equity REAL NOT NULL,
            equity_updated_at TEXT NOT NULL,
            basis_source TEXT NOT NULL DEFAULT 'legacy_statement',
            basis_id TEXT,
            basis_updated_at TEXT,
            total_deposit REAL NOT NULL,
            total_units REAL NOT NULL,
            total_tax REAL NOT NULL,
            total_referrer_rebate REAL NOT NULL,
            capped_cash_flows INTEGER NOT NULL DEFAULT 0,
            capped_units REAL NOT NULL DEFAULT 0,
            capped_cash_amount REAL NOT NULL DEFAULT 0,
            investor_count INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft',
            status_updated_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db)
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "settlement_model",
        "ALTER TABLE fund_settlement_runs ADD COLUMN settlement_model TEXT",
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET settlement_model = 'cash_flow_ledger_v1'
        WHERE settlement_model IS NULL OR settlement_model = ''
        "#,
    )
    .execute(db)
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "status_updated_at",
        "ALTER TABLE fund_settlement_runs ADD COLUMN status_updated_at TEXT",
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET status_updated_at = created_at
        WHERE status_updated_at IS NULL
        "#,
    )
    .execute(db)
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "basis_source",
        "ALTER TABLE fund_settlement_runs ADD COLUMN basis_source TEXT",
    )
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "basis_id",
        "ALTER TABLE fund_settlement_runs ADD COLUMN basis_id TEXT",
    )
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "basis_updated_at",
        "ALTER TABLE fund_settlement_runs ADD COLUMN basis_updated_at TEXT",
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET basis_source = 'legacy_statement'
        WHERE basis_source IS NULL OR basis_source = ''
        "#,
    )
    .execute(db)
    .await?;
    sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET basis_id = CAST(equity_event_index AS TEXT)
        WHERE basis_id IS NULL OR basis_id = ''
        "#,
    )
    .execute(db)
    .await?;
    sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET basis_updated_at = equity_updated_at
        WHERE basis_updated_at IS NULL OR basis_updated_at = ''
        "#,
    )
    .execute(db)
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "capped_cash_flows",
        "ALTER TABLE fund_settlement_runs ADD COLUMN capped_cash_flows INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "capped_units",
        "ALTER TABLE fund_settlement_runs ADD COLUMN capped_units REAL NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        db,
        "fund_settlement_runs",
        "capped_cash_amount",
        "ALTER TABLE fund_settlement_runs ADD COLUMN capped_cash_amount REAL NOT NULL DEFAULT 0",
    )
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_fund_settlement_runs_fund_created
        ON fund_settlement_runs (fund_id, created_at DESC)
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query("DROP INDEX IF EXISTS idx_fund_settlement_runs_one_confirmed")
        .execute(db)
        .await?;

    sqlx::query("DROP INDEX IF EXISTS idx_fund_settlement_runs_one_confirmed_basis")
        .execute(db)
        .await?;

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_fund_settlement_runs_one_confirmed_model_basis
        ON fund_settlement_runs (fund_id, settlement_model, basis_source, basis_id)
        WHERE status = 'confirmed'
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_fund_settlement_runs_one_active_event_state_basis
        ON fund_settlement_runs (fund_id, settlement_model, basis_source, basis_id)
        WHERE settlement_model = 'event_state_v1' AND status IN ('draft', 'confirmed')
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS fund_settlement_investor_rows (
            run_id TEXT NOT NULL,
            fund_id TEXT NOT NULL,
            investor_name TEXT NOT NULL,
            referrer TEXT,
            deposit REAL NOT NULL,
            units REAL NOT NULL,
            ownership REAL NOT NULL,
            gross_equity REAL NOT NULL,
            profit REAL NOT NULL,
            tax_threshold REAL NOT NULL,
            tax_rate REAL NOT NULL,
            tax REAL NOT NULL,
            referrer_rebate_rate REAL NOT NULL,
            referrer_rebate REAL NOT NULL,
            referrer_rebate_received REAL NOT NULL DEFAULT 0,
            tax_account_credit REAL NOT NULL DEFAULT 0,
            capped_cash_amount REAL NOT NULL DEFAULT 0,
            net_equity REAL NOT NULL,
            PRIMARY KEY (run_id, investor_name)
        )
        "#,
    )
    .execute(db)
    .await?;
    ensure_column(
        db,
        "fund_settlement_investor_rows",
        "referrer_rebate_received",
        "ALTER TABLE fund_settlement_investor_rows ADD COLUMN referrer_rebate_received REAL NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        db,
        "fund_settlement_investor_rows",
        "tax_account_credit",
        "ALTER TABLE fund_settlement_investor_rows ADD COLUMN tax_account_credit REAL NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        db,
        "fund_settlement_investor_rows",
        "capped_cash_amount",
        "ALTER TABLE fund_settlement_investor_rows ADD COLUMN capped_cash_amount REAL NOT NULL DEFAULT 0",
    )
    .await?;

    drop_legacy_fund_tables(db).await?;

    Ok(())
}

async fn ensure_column(
    db: &SqlitePool,
    table: &str,
    column: &str,
    alter_statement: &str,
) -> anyhow::Result<()> {
    let pragma = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&pragma).fetch_all(db).await?;
    let exists = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    });

    if !exists {
        sqlx::query(alter_statement).execute(db).await?;
    }

    Ok(())
}

async fn migrate_fund_events(db: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO fund_events (
            fund_id, event_index, event_type, occurred_at, investor_id, comment, payload,
            created_at, updated_at
        )
        SELECT fund_id,
               event_index,
               CASE
                   WHEN json_extract(payload, '$.fund_equity.equity') IS NOT NULL THEN 'fund_equity_set'
                   WHEN json_extract(payload, '$.order.name') IS NOT NULL THEN 'cash_flow_recorded'
                   WHEN json_extract(payload, '$.investor.add_tax_threshold') IS NOT NULL THEN 'tax_threshold_adjusted'
                   WHEN json_extract(payload, '$.investor.name') IS NOT NULL THEN 'investor_profile_updated'
                   WHEN json_extract(payload, '$.type') = 'taxation/v2' OR event_type = 'taxation/v2' THEN 'taxation_v2_applied'
                   WHEN json_extract(payload, '$.type') = 'taxation' OR event_type = 'taxation' THEN 'taxation_v1_applied'
               END AS migrated_event_type,
               updated_at,
               COALESCE(json_extract(payload, '$.order.name'), json_extract(payload, '$.investor.name')),
               json_extract(payload, '$.comment'),
               CASE
                   WHEN json_extract(payload, '$.fund_equity.equity') IS NOT NULL
                       THEN json_object('equity', json_extract(payload, '$.fund_equity.equity'))
                   WHEN json_extract(payload, '$.order.name') IS NOT NULL
                       THEN json_object('amount', json_extract(payload, '$.order.deposit'))
                   WHEN json_extract(payload, '$.investor.add_tax_threshold') IS NOT NULL
                       THEN json_object('amount', json_extract(payload, '$.investor.add_tax_threshold'))
                   WHEN json_extract(payload, '$.investor.name') IS NOT NULL
                       THEN json_object(
                           'referrer_id', json_extract(payload, '$.investor.referrer'),
                           'tax_rate', json_extract(payload, '$.investor.tax_rate'),
                           'referrer_rebate_rate', json_extract(payload, '$.investor.referrer_rebate_rate')
                       )
                   ELSE '{}'
               END,
               CURRENT_TIMESTAMP,
               CURRENT_TIMESTAMP
        FROM fund_statement_events
        WHERE migrated_event_type IS NOT NULL
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        WITH max_events AS (
            SELECT fund_id, COALESCE(MAX(event_index), -1) AS max_event_index
            FROM fund_events
            GROUP BY fund_id
        ),
        nav_events AS (
            SELECT n.fund_id,
                   COALESCE(m.max_event_index, -1)
                       + ROW_NUMBER() OVER (PARTITION BY n.fund_id ORDER BY n.created_at, n.id) AS event_index,
                   n.created_at,
                   n.equity
            FROM fund_nav_snapshots n
            LEFT JOIN max_events m ON m.fund_id = n.fund_id
            WHERE NOT EXISTS (
                SELECT 1
                FROM fund_events e
                WHERE e.fund_id = n.fund_id
                  AND e.event_type = 'fund_equity_set'
                  AND e.occurred_at = n.created_at
                  AND json_extract(e.payload, '$.equity') = n.equity
            )
        )
        INSERT OR IGNORE INTO fund_events (
            fund_id, event_index, event_type, occurred_at, investor_id, comment, payload,
            created_at, updated_at
        )
        SELECT fund_id,
               event_index,
               'fund_equity_set',
               created_at,
               NULL,
               'Migrated NAV sample',
               json_object('equity', equity),
               CURRENT_TIMESTAMP,
               CURRENT_TIMESTAMP
        FROM nav_events
        "#,
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn drop_legacy_fund_tables(db: &SqlitePool) -> anyhow::Result<()> {
    for table in [
        "fund_nav_snapshots",
        "fund_statement_events",
        "fund_statement_orders",
        "fund_statement_equity",
        "fund_statement_investors",
        "fund_statement_tax_modes",
        "fund_settlement_runs",
        "fund_settlement_investor_rows",
    ] {
        let sql = format!("DROP TABLE IF EXISTS {table}");
        sqlx::query(&sql).execute(db).await?;
    }

    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let _db = &state.db;

    Json(HealthResponse {
        status: "ok",
        database: state.db_path.display().to_string(),
    })
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<auth::AuthUser>, AppError> {
    Ok(Json(auth::require_user(&state, &headers).await?))
}

async fn auth_config(State(state): State<AppState>) -> Result<Json<auth::AuthConfig>, AppError> {
    Ok(Json(auth::auth_config(&state.db).await?))
}

async fn setup_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<auth::SetupState>, AppError> {
    let user = auth::require_user(&state, &headers).await?;
    Ok(Json(auth::setup_state(&state.db, &user).await?))
}

async fn initialize_setup(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<auth::SetupState>, AppError> {
    let user = auth::require_user(&state, &headers).await?;
    Ok(Json(auth::initialize_setup(&state.db, &user).await?))
}

async fn list_exchanges() -> Json<Vec<exchanges::ExchangeInfo>> {
    Json(exchanges::list_exchanges())
}

async fn list_account_refs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AccountRef>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let mut refs = Vec::new();
    for credential in credentials::list_stored_credentials(&state.db, &user.user_id).await? {
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
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Vec<AccountInfo>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    if query.credential_id.is_none() && query.account_id.is_none() {
        return Ok(Json(read_all_accounts(&state.db, &user.user_id).await?));
    }

    Ok(Json(vec![
        read_account(&state.db, &user.user_id, query).await?,
    ]))
}

async fn list_positions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Vec<Position>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    Ok(Json(
        read_account(&state.db, &user.user_id, query)
            .await?
            .positions,
    ))
}

async fn read_account(
    db: &SqlitePool,
    owner_id: &str,
    query: AccountQuery,
) -> Result<AccountInfo, AppError> {
    if let Some(credential_id) = query.credential_id {
        return read_credential_account(db, owner_id, &credential_id).await;
    }
    if let Some(account_id) = query.account_id {
        return read_account_by_account_id(db, owner_id, &account_id).await;
    }

    Err(AppError::bad_request(
        "missing credential_id or account_id query",
    ))
}

async fn read_credential_account(
    db: &SqlitePool,
    owner_id: &str,
    credential_id: &str,
) -> Result<AccountInfo, AppError> {
    let credential = credentials::get_stored_credential(db, owner_id, credential_id).await?;
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
    owner_id: &str,
    account_id: &str,
) -> Result<AccountInfo, AppError> {
    if let Some(account) =
        virtual_accounts::compose_virtual_account_by_id(db, owner_id, account_id).await?
    {
        return Ok(account);
    }

    if let Some(account) = exchanges::special_account_by_id(account_id)
        .await
        .map_err(AppError::bad_gateway)?
    {
        return Ok(account);
    }

    for credential in credentials::list_stored_credentials(db, owner_id).await? {
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

    let sources = custom_account_sources::list_custom_account_source_configs(db, owner_id).await?;
    if let Some(account) = custom_account_sources::read_account(&sources, account_id).await? {
        return Ok(account);
    }

    Err(AppError::bad_request(format!(
        "account not found: {account_id}"
    )))
}

async fn read_all_accounts(db: &SqlitePool, owner_id: &str) -> Result<Vec<AccountInfo>, AppError> {
    let mut accounts = Vec::new();
    for credential in credentials::list_stored_credentials(db, owner_id).await? {
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

    for config in virtual_accounts::list_virtual_account_configs(db, owner_id)
        .await?
        .into_iter()
        .filter(|config| config.enabled)
    {
        if let Some(account) =
            virtual_accounts::compose_virtual_account_by_id(db, owner_id, &config.account_id)
                .await?
        {
            accounts.push(account);
        }
    }

    let sources = custom_account_sources::list_custom_account_source_configs(db, owner_id).await?;
    accounts.extend(
        custom_account_sources::discover_accounts(&sources)
            .await?
            .into_iter()
            .map(AccountInfo::normalized),
    );
    accounts.extend(
        exchanges::list_special_accounts()
            .await
            .map_err(AppError::bad_gateway)?
            .into_iter()
            .map(AccountInfo::normalized),
    );

    Ok(accounts)
}

async fn list_trades(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CredentialQuery>,
) -> Result<Json<Vec<TradeFill>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let credential =
        credentials::get_stored_credential(&state.db, &user.user_id, &query.credential_id).await?;
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
    Json(rates::live_snapshot(query.target.as_deref().unwrap_or("USD")).await)
}

async fn convert_rate(Query(query): Query<ConvertRateQuery>) -> Json<rates::CurrencyConversion> {
    let target = query.to.as_deref().unwrap_or("USD");
    let snapshot = rates::live_snapshot(target).await;

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
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

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
