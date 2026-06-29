use axum::{Json, extract::State, http::StatusCode};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::{AppError, AppState, models::AccountInfo, rates, virtual_accounts};

const DEFAULT_POLL_INTERVAL_SECONDS: i64 = 600;
const FUND_SCAN_INTERVAL_SECONDS: u64 = 60;

#[derive(Debug, Deserialize)]
pub struct CreateFundRequest {
    pub id: Option<String>,
    pub name: String,
    pub account_id: String,
    pub enabled: bool,
    pub target_currency: Option<String>,
    pub poll_interval_seconds: Option<i64>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct FundConfig {
    pub id: String,
    pub name: String,
    pub account_id: String,
    pub enabled: bool,
    pub target_currency: String,
    pub poll_interval_seconds: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_sampled_at: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundNavSnapshot {
    pub id: String,
    pub fund_id: String,
    pub account_id: String,
    pub equity: f64,
    pub target_currency: String,
    pub positions_count: i64,
    pub unpriced_positions: i64,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct FundNavQuery {
    fund_id: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct FundStatementQuery {
    fund_id: String,
}

#[derive(Deserialize)]
pub struct SampleFundQuery {
    fund_id: String,
}

#[derive(Debug, Serialize)]
pub struct FundStatementSummary {
    totals: FundStatementTotals,
    investors: Vec<FundStatementInvestor>,
    recent_orders: Vec<FundStatementOrder>,
    latest_equity: Option<FundStatementEquity>,
    tax_modes: Vec<FundStatementTaxMode>,
}

#[derive(Debug, Serialize)]
pub struct FundStatementTotals {
    events: i64,
    orders: i64,
    order_deposit: f64,
    equity_points: i64,
    investors: i64,
    tax_modes: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundStatementInvestor {
    name: String,
    referrer: Option<String>,
    tax_rate: Option<f64>,
    referrer_rebate_rate: Option<f64>,
    tax_threshold: Option<f64>,
    updated_at: String,
    source_event_index: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundStatementOrder {
    event_index: i64,
    investor_name: String,
    deposit: f64,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundStatementEquity {
    event_index: i64,
    equity: f64,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundStatementTaxMode {
    event_index: i64,
    mode: String,
    comment: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct FundValuation {
    equity: f64,
    positions_count: i64,
    unpriced_positions: i64,
}

pub async fn list_funds(State(state): State<AppState>) -> Result<Json<Vec<FundConfig>>, AppError> {
    Ok(Json(list_fund_configs(&state.db).await?))
}

pub async fn create_fund(
    State(state): State<AppState>,
    Json(request): Json<CreateFundRequest>,
) -> Result<(StatusCode, Json<FundConfig>), AppError> {
    validate_fund_request(&request)?;

    let id = request
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let target_currency = request
        .target_currency
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("USD")
        .to_uppercase();
    let poll_interval_seconds = request
        .poll_interval_seconds
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS);

    sqlx::query(
        r#"
        INSERT INTO funds (id, name, account_id, enabled, target_currency, poll_interval_seconds)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            account_id = excluded.account_id,
            enabled = excluded.enabled,
            target_currency = excluded.target_currency,
            poll_interval_seconds = excluded.poll_interval_seconds,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(&id)
    .bind(request.name.trim())
    .bind(request.account_id.trim())
    .bind(request.enabled)
    .bind(target_currency)
    .bind(poll_interval_seconds)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(get_fund_config(&state.db, &id).await?),
    ))
}

pub async fn list_fund_nav(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FundNavQuery>,
) -> Result<Json<Vec<FundNavSnapshot>>, AppError> {
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let rows = if let Some(fund_id) = query.fund_id {
        sqlx::query_as::<_, FundNavSnapshot>(
            r#"
            SELECT id, fund_id, account_id, equity, target_currency, positions_count,
                   unpriced_positions, created_at
            FROM fund_nav_snapshots
            WHERE fund_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .bind(fund_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, FundNavSnapshot>(
            r#"
            SELECT id, fund_id, account_id, equity, target_currency, positions_count,
                   unpriced_positions, created_at
            FROM fund_nav_snapshots
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(rows))
}

pub async fn get_fund_statement_summary(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FundStatementQuery>,
) -> Result<Json<FundStatementSummary>, AppError> {
    get_fund_config(&state.db, &query.fund_id).await?;

    let (events,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM fund_statement_events WHERE fund_id = ?1")
            .bind(&query.fund_id)
            .fetch_one(&state.db)
            .await?;
    let (orders, order_deposit): (i64, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(*), SUM(deposit) FROM fund_statement_orders WHERE fund_id = ?1",
    )
    .bind(&query.fund_id)
    .fetch_one(&state.db)
    .await?;
    let (equity_points,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM fund_statement_equity WHERE fund_id = ?1")
            .bind(&query.fund_id)
            .fetch_one(&state.db)
            .await?;
    let (investor_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM fund_statement_investors WHERE fund_id = ?1")
            .bind(&query.fund_id)
            .fetch_one(&state.db)
            .await?;
    let (tax_mode_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM fund_statement_tax_modes WHERE fund_id = ?1")
            .bind(&query.fund_id)
            .fetch_one(&state.db)
            .await?;

    let investors = sqlx::query_as::<_, FundStatementInvestor>(
        r#"
        SELECT name, referrer, tax_rate, referrer_rebate_rate, tax_threshold,
               updated_at, source_event_index
        FROM fund_statement_investors
        WHERE fund_id = ?1
        ORDER BY name ASC
        "#,
    )
    .bind(&query.fund_id)
    .fetch_all(&state.db)
    .await?;

    let recent_orders = sqlx::query_as::<_, FundStatementOrder>(
        r#"
        SELECT event_index, investor_name, deposit, updated_at
        FROM fund_statement_orders
        WHERE fund_id = ?1
        ORDER BY updated_at DESC, event_index DESC
        LIMIT 50
        "#,
    )
    .bind(&query.fund_id)
    .fetch_all(&state.db)
    .await?;

    let latest_equity = sqlx::query_as::<_, FundStatementEquity>(
        r#"
        SELECT event_index, equity, updated_at
        FROM fund_statement_equity
        WHERE fund_id = ?1
        ORDER BY updated_at DESC, event_index DESC
        LIMIT 1
        "#,
    )
    .bind(&query.fund_id)
    .fetch_optional(&state.db)
    .await?;

    let tax_modes = sqlx::query_as::<_, FundStatementTaxMode>(
        r#"
        SELECT event_index, mode, comment, updated_at
        FROM fund_statement_tax_modes
        WHERE fund_id = ?1
        ORDER BY updated_at ASC, event_index ASC
        "#,
    )
    .bind(&query.fund_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(FundStatementSummary {
        totals: FundStatementTotals {
            events,
            orders,
            order_deposit: order_deposit.unwrap_or(0.0),
            equity_points,
            investors: investor_count,
            tax_modes: tax_mode_count,
        },
        investors,
        recent_orders,
        latest_equity,
        tax_modes,
    }))
}

pub async fn sample_fund_now(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SampleFundQuery>,
) -> Result<Json<FundNavSnapshot>, AppError> {
    let config = get_fund_config(&state.db, &query.fund_id).await?;
    Ok(Json(sample_fund(&state.db, &config).await?))
}

pub fn spawn_fund_polling(state: AppState) {
    tokio::spawn(async move {
        sample_due_funds_at_boundary(&state.db).await;
        let mut ticker = interval(Duration::from_secs(FUND_SCAN_INTERVAL_SECONDS));
        loop {
            ticker.tick().await;
            sample_due_funds_at_boundary(&state.db).await;
        }
    });
}

pub async fn list_fund_configs(db: &SqlitePool) -> Result<Vec<FundConfig>, AppError> {
    Ok(sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.created_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_nav_snapshots s ON s.fund_id = f.id
        GROUP BY f.id
        ORDER BY f.created_at DESC
        "#,
    )
    .fetch_all(db)
    .await?)
}

async fn sample_due_funds_at_boundary(db: &SqlitePool) {
    if let Err(error) = sample_due_funds(db).await {
        eprintln!("fund polling failed: {error:?}");
    }
}

async fn sample_due_funds(db: &SqlitePool) -> Result<(), AppError> {
    for config in due_fund_configs(db).await? {
        if let Err(error) = sample_fund(db, &config).await {
            eprintln!("fund sample failed for {}: {error:?}", config.id);
        }
    }

    Ok(())
}

async fn due_fund_configs(db: &SqlitePool) -> Result<Vec<FundConfig>, AppError> {
    Ok(sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.created_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_nav_snapshots s ON s.fund_id = f.id
        WHERE f.enabled = 1
        GROUP BY f.id
        HAVING last_sampled_at IS NULL
            OR unixepoch('now') - unixepoch(last_sampled_at) >= f.poll_interval_seconds
        ORDER BY f.created_at ASC
        "#,
    )
    .fetch_all(db)
    .await?)
}

async fn get_fund_config(db: &SqlitePool, fund_id: &str) -> Result<FundConfig, AppError> {
    sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.created_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_nav_snapshots s ON s.fund_id = f.id
        WHERE f.id = ?1
        GROUP BY f.id
        "#,
    )
    .bind(fund_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

async fn sample_fund(db: &SqlitePool, config: &FundConfig) -> Result<FundNavSnapshot, AppError> {
    let account = virtual_accounts::compose_virtual_account_by_id(db, &config.account_id)
        .await?
        .ok_or_else(|| AppError::bad_request("fund account must be an enabled virtual account"))?;
    let valuation = value_account(&account, &config.target_currency);
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO fund_nav_snapshots (
            id, fund_id, account_id, equity, target_currency, positions_count,
            unpriced_positions, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&id)
    .bind(&config.id)
    .bind(&config.account_id)
    .bind(valuation.equity)
    .bind(&config.target_currency)
    .bind(valuation.positions_count)
    .bind(valuation.unpriced_positions)
    .bind(&created_at)
    .execute(db)
    .await?;

    Ok(FundNavSnapshot {
        id,
        fund_id: config.id.clone(),
        account_id: config.account_id.clone(),
        equity: valuation.equity,
        target_currency: config.target_currency.clone(),
        positions_count: valuation.positions_count,
        unpriced_positions: valuation.unpriced_positions,
        created_at,
    })
}

fn value_account(account: &AccountInfo, target_currency: &str) -> FundValuation {
    let snapshot = rates::snapshot(target_currency);
    account.positions.iter().fold(
        FundValuation {
            equity: 0.0,
            positions_count: 0,
            unpriced_positions: 0,
        },
        |mut valuation, position| {
            valuation.positions_count += 1;
            if position.volume > 0.0 && position.valuation == 0.0 && position.closable_price == 0.0
            {
                valuation.unpriced_positions += 1;
                return valuation;
            }
            let currency = position
                .settlement_currency
                .as_deref()
                .or(position.notional_currency.as_deref())
                .or(position.quote_currency.as_deref())
                .unwrap_or(target_currency);
            if let Some(rate) = rates::convert_rate(&snapshot.edges, currency, target_currency) {
                valuation.equity += position.valuation * rate;
            } else {
                valuation.unpriced_positions += 1;
            }
            valuation
        },
    )
}

fn validate_fund_request(request: &CreateFundRequest) -> Result<(), AppError> {
    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("missing fund name"));
    }
    if request.account_id.trim().is_empty() {
        return Err(AppError::bad_request("missing fund virtual account id"));
    }
    if let Some(value) = request.poll_interval_seconds {
        if value < 60 {
            return Err(AppError::bad_request(
                "fund poll interval must be at least 60 seconds",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::models::Position;

    use super::*;

    #[test]
    fn values_account_in_target_currency() {
        let account = AccountInfo {
            account_id: "virtual/fund".to_string(),
            positions: vec![
                Position {
                    valuation: 100.0,
                    notional_currency: Some("USDT".to_string()),
                    ..Position::default()
                },
                Position {
                    valuation: -25.0,
                    notional_currency: Some("USDC".to_string()),
                    ..Position::default()
                },
            ],
            orders: Vec::new(),
            timestamp_in_us: 0,
        };

        let valuation = value_account(&account, "USD");

        assert_eq!(valuation.equity, 75.0);
        assert_eq!(valuation.positions_count, 2);
        assert_eq!(valuation.unpriced_positions, 0);
    }

    #[test]
    fn marks_unpriced_positions() {
        let account = AccountInfo {
            account_id: "virtual/fund".to_string(),
            positions: vec![Position {
                valuation: 100.0,
                notional_currency: Some("BTC".to_string()),
                ..Position::default()
            }],
            orders: Vec::new(),
            timestamp_in_us: 0,
        };

        let valuation = value_account(&account, "USD");

        assert_eq!(valuation.equity, 0.0);
        assert_eq!(valuation.positions_count, 1);
        assert_eq!(valuation.unpriced_positions, 1);
    }

    #[test]
    fn marks_zero_price_positions_as_unpriced() {
        let account = AccountInfo {
            account_id: "virtual/fund".to_string(),
            positions: vec![Position {
                volume: 1.0,
                valuation: 0.0,
                closable_price: 0.0,
                notional_currency: Some("USDT".to_string()),
                ..Position::default()
            }],
            orders: Vec::new(),
            timestamp_in_us: 0,
        };

        let valuation = value_account(&account, "USD");

        assert_eq!(valuation.equity, 0.0);
        assert_eq!(valuation.positions_count, 1);
        assert_eq!(valuation.unpriced_positions, 1);
    }

    #[test]
    fn rejects_short_poll_intervals() {
        let request = CreateFundRequest {
            id: None,
            name: "Fund".to_string(),
            account_id: "virtual/fund".to_string(),
            enabled: true,
            target_currency: None,
            poll_interval_seconds: Some(59),
        };

        assert!(validate_fund_request(&request).is_err());
    }
}
