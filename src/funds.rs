use std::collections::HashMap;

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
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
pub struct FundSettlementQuery {
    fund_id: String,
}

#[derive(Deserialize)]
pub struct FundSettlementRunQuery {
    run_id: String,
}

#[derive(Deserialize)]
pub struct CreateFundSettlementRunRequest {
    fund_id: String,
}

#[derive(Deserialize)]
pub struct UpdateFundSettlementRunRequest {
    run_id: String,
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
    reconciliation: Option<FundEquityReconciliation>,
    tax_modes: Vec<FundStatementTaxMode>,
    tax_threshold_adjustments: Vec<FundTaxThresholdAdjustment>,
}

#[derive(Debug, Serialize)]
pub struct FundStatementTotals {
    events: i64,
    orders: i64,
    order_deposit: f64,
    inflow_count: i64,
    inflow_amount: f64,
    outflow_count: i64,
    outflow_amount: f64,
    equity_points: i64,
    investors: i64,
    tax_modes: i64,
    tax_threshold_adjustments: i64,
    tax_threshold_amount: f64,
    overdrawn_cash_flows: i64,
    overdrawn_investors: i64,
    capped_cash_flows: i64,
    capped_units: f64,
    capped_cash_amount: f64,
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

#[derive(Clone, Debug, FromRow)]
struct SettlementOrder {
    event_index: i64,
    investor_name: String,
    deposit: f64,
    updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct FundStatementOrder {
    event_index: i64,
    investor_name: String,
    deposit: f64,
    effective_deposit: f64,
    capped_cash_amount: f64,
    direction: String,
    nav_per_unit: f64,
    requested_unit_delta: f64,
    unit_delta: f64,
    capped_units: f64,
    investor_units_after: f64,
    total_units_after: f64,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
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

#[derive(Debug, Serialize)]
pub struct FundTaxThresholdAdjustment {
    event_index: i64,
    investor_name: String,
    amount: f64,
    comment: Option<String>,
    updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct FundEquityReconciliation {
    legacy_equity: f64,
    legacy_updated_at: String,
    nav_equity: f64,
    nav_created_at: String,
    delta: f64,
    delta_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct FundSettlementPreview {
    fund_id: String,
    latest_equity: Option<FundStatementEquity>,
    basis: Option<FundSettlementBasis>,
    total_deposit: f64,
    total_units: f64,
    total_tax: f64,
    total_referrer_rebate: f64,
    totals: FundSettlementTotals,
    investor_taxes: Vec<FundInvestorTax>,
    referrer_rebates: Vec<FundReferrerRebate>,
    investors: Vec<FundInvestorSettlement>,
}

#[derive(Debug, Serialize)]
pub struct FundInvestorSettlement {
    name: String,
    referrer: Option<String>,
    deposit: f64,
    units: f64,
    ownership: f64,
    gross_equity: f64,
    profit: f64,
    tax_threshold: f64,
    tax_rate: f64,
    tax: f64,
    referrer_rebate_rate: f64,
    referrer_rebate: f64,
    net_equity: f64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundSettlementRun {
    id: String,
    fund_id: String,
    equity_event_index: i64,
    equity: f64,
    equity_updated_at: String,
    basis_source: String,
    basis_id: String,
    basis_updated_at: String,
    total_deposit: f64,
    total_units: f64,
    total_tax: f64,
    total_referrer_rebate: f64,
    capped_cash_flows: i64,
    capped_units: f64,
    capped_cash_amount: f64,
    investor_count: i64,
    status: String,
    status_updated_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub struct FundSettlementRunDetail {
    run: FundSettlementRun,
    investors: Vec<FundInvestorSettlement>,
    totals: FundSettlementTotals,
    investor_taxes: Vec<FundInvestorTax>,
    referrer_rebates: Vec<FundReferrerRebate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FundSettlementTotals {
    gross_equity: f64,
    net_equity: f64,
    tax: f64,
    referrer_rebate: f64,
    retained_tax: f64,
    overdrawn_investors: i64,
    capped_cash_flows: i64,
    capped_units: f64,
    capped_cash_amount: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FundSettlementBasis {
    source: String,
    id: String,
    equity: f64,
    updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct FundInvestorTax {
    investor: String,
    tax: f64,
}

#[derive(Debug, Serialize)]
pub struct FundReferrerRebate {
    referrer: String,
    rebate: f64,
}

#[derive(Debug, FromRow)]
struct FundSettlementInvestorRow {
    name: String,
    referrer: Option<String>,
    deposit: f64,
    units: f64,
    ownership: f64,
    gross_equity: f64,
    profit: f64,
    tax_threshold: f64,
    tax_rate: f64,
    tax: f64,
    referrer_rebate_rate: f64,
    referrer_rebate: f64,
    net_equity: f64,
}

impl From<FundSettlementInvestorRow> for FundInvestorSettlement {
    fn from(row: FundSettlementInvestorRow) -> Self {
        Self {
            name: row.name,
            referrer: row.referrer,
            deposit: row.deposit,
            units: row.units,
            ownership: row.ownership,
            gross_equity: row.gross_equity,
            profit: row.profit,
            tax_threshold: row.tax_threshold,
            tax_rate: row.tax_rate,
            tax: row.tax,
            referrer_rebate_rate: row.referrer_rebate_rate,
            referrer_rebate: row.referrer_rebate,
            net_equity: row.net_equity,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SettlementInvestorState {
    name: String,
    referrer: Option<String>,
    deposit: f64,
    units: f64,
    tax_threshold: f64,
    tax_rate: f64,
    referrer_rebate_rate: f64,
}

#[derive(Debug, FromRow)]
struct FundStatementEventPayloadRow {
    event_index: i64,
    updated_at: String,
    payload: String,
}

#[derive(Debug, Deserialize)]
struct FundStatementEventPayload {
    comment: Option<String>,
    investor: Option<FundStatementInvestorPayload>,
}

#[derive(Debug, Deserialize)]
struct FundStatementInvestorPayload {
    name: String,
    add_tax_threshold: Option<f64>,
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
    let (orders, order_deposit, inflow_count, inflow_amount, outflow_count, outflow_amount): (
        i64,
        Option<f64>,
        i64,
        Option<f64>,
        i64,
        Option<f64>,
    ) = sqlx::query_as(
        r#"
        SELECT COUNT(*),
               SUM(deposit),
               SUM(CASE WHEN deposit > 0 THEN 1 ELSE 0 END),
               SUM(CASE WHEN deposit > 0 THEN deposit ELSE 0 END),
               SUM(CASE WHEN deposit < 0 THEN 1 ELSE 0 END),
               SUM(CASE WHEN deposit < 0 THEN -deposit ELSE 0 END)
        FROM fund_statement_orders
        WHERE fund_id = ?1
        "#,
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

    let statement_orders = sqlx::query_as::<_, SettlementOrder>(
        r#"
        SELECT event_index, investor_name, deposit, updated_at
        FROM fund_statement_orders
        WHERE fund_id = ?1
        ORDER BY updated_at ASC, event_index ASC
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
    let statement_equity = sqlx::query_as::<_, FundStatementEquity>(
        r#"
        SELECT event_index, equity, updated_at
        FROM fund_statement_equity
        WHERE fund_id = ?1
        ORDER BY updated_at ASC, event_index ASC
        "#,
    )
    .bind(&query.fund_id)
    .fetch_all(&state.db)
    .await?;
    let mut recent_orders = build_cash_flow_ledger(&statement_orders, &statement_equity);
    let overdrawn_cash_flows = recent_orders
        .iter()
        .filter(|item| item.investor_units_after < -1e-9)
        .count() as i64;
    let mut final_investor_units = HashMap::<&str, f64>::new();
    for item in &recent_orders {
        final_investor_units.insert(item.investor_name.as_str(), item.investor_units_after);
    }
    let overdrawn_investors = final_investor_units
        .values()
        .filter(|units| **units < -1e-9)
        .count() as i64;
    let capped_cash_flows = recent_orders
        .iter()
        .filter(|item| item.capped_units > 1e-9)
        .count() as i64;
    let capped_units = recent_orders.iter().map(|item| item.capped_units).sum();
    let capped_cash_amount = recent_orders
        .iter()
        .map(|item| item.capped_cash_amount)
        .sum();
    recent_orders.reverse();
    recent_orders.truncate(50);
    let latest_nav = sqlx::query_as::<_, FundNavSnapshot>(
        r#"
        SELECT id, fund_id, account_id, equity, target_currency, positions_count,
               unpriced_positions, created_at
        FROM fund_nav_snapshots
        WHERE fund_id = ?1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(&query.fund_id)
    .fetch_optional(&state.db)
    .await?;
    let reconciliation = latest_equity
        .as_ref()
        .zip(latest_nav.as_ref())
        .map(|(legacy, nav)| reconcile_fund_equity(legacy, nav));

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
    let tax_threshold_adjustments =
        load_tax_threshold_adjustments(&state.db, &query.fund_id).await?;
    let tax_threshold_amount = tax_threshold_adjustments
        .iter()
        .map(|item| item.amount)
        .sum();

    Ok(Json(FundStatementSummary {
        totals: FundStatementTotals {
            events,
            orders,
            order_deposit: order_deposit.unwrap_or(0.0),
            inflow_count,
            inflow_amount: inflow_amount.unwrap_or(0.0),
            outflow_count,
            outflow_amount: outflow_amount.unwrap_or(0.0),
            equity_points,
            investors: investor_count,
            tax_modes: tax_mode_count,
            tax_threshold_adjustments: tax_threshold_adjustments.len() as i64,
            tax_threshold_amount,
            overdrawn_cash_flows,
            overdrawn_investors,
            capped_cash_flows,
            capped_units,
            capped_cash_amount,
        },
        investors,
        recent_orders,
        latest_equity,
        reconciliation,
        tax_modes,
        tax_threshold_adjustments,
    }))
}

pub async fn get_fund_settlement_preview(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FundSettlementQuery>,
) -> Result<Json<FundSettlementPreview>, AppError> {
    get_fund_config(&state.db, &query.fund_id).await?;
    Ok(Json(
        load_fund_settlement_preview(&state.db, query.fund_id).await?,
    ))
}

pub async fn list_fund_settlement_runs(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FundSettlementQuery>,
) -> Result<Json<Vec<FundSettlementRun>>, AppError> {
    get_fund_config(&state.db, &query.fund_id).await?;

    Ok(Json(
        sqlx::query_as::<_, FundSettlementRun>(
            r#"
            SELECT id, fund_id, equity_event_index, equity, equity_updated_at,
                   basis_source, basis_id, basis_updated_at,
                   total_deposit, total_units, total_tax, total_referrer_rebate,
                   capped_cash_flows, capped_units, capped_cash_amount,
                   investor_count, status, status_updated_at, created_at
            FROM fund_settlement_runs
            WHERE fund_id = ?1
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .bind(&query.fund_id)
        .fetch_all(&state.db)
        .await?,
    ))
}

pub async fn get_fund_settlement_run_detail(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FundSettlementRunQuery>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    get_fund_settlement_run_detail_by_id(&state.db, &query.run_id)
        .await
        .map(Json)
}

pub async fn export_fund_settlement_run_csv(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FundSettlementRunQuery>,
) -> Result<Response, AppError> {
    let detail = get_fund_settlement_run_detail_by_id(&state.db, &query.run_id).await?;
    let filename = format!("fund-settlement-{}.csv", detail.run.id);
    let body = settlement_run_csv(&detail);

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        body,
    )
        .into_response())
}

async fn get_fund_settlement_run_detail_by_id(
    db: &SqlitePool,
    run_id: &str,
) -> Result<FundSettlementRunDetail, AppError> {
    let run = sqlx::query_as::<_, FundSettlementRun>(
        r#"
        SELECT id, fund_id, equity_event_index, equity, equity_updated_at,
               basis_source, basis_id, basis_updated_at,
               total_deposit, total_units, total_tax, total_referrer_rebate,
               capped_cash_flows, capped_units, capped_cash_amount,
               investor_count, status, status_updated_at, created_at
        FROM fund_settlement_runs
        WHERE id = ?1
        "#,
    )
    .bind(run_id)
    .fetch_one(db)
    .await?;
    let investors = sqlx::query_as::<_, FundSettlementInvestorRow>(
        r#"
        SELECT investor_name AS name, referrer, deposit, units, ownership,
               gross_equity, profit, tax_threshold, tax_rate, tax,
               referrer_rebate_rate, referrer_rebate, net_equity
        FROM fund_settlement_investor_rows
        WHERE run_id = ?1
        ORDER BY gross_equity DESC, investor_name ASC
        "#,
    )
    .bind(run_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(FundInvestorSettlement::from)
    .collect::<Vec<_>>();
    let totals = summarize_settlement_totals(&investors);
    let totals = settlement_totals_with_capped_run(totals, &run);
    let investor_taxes = summarize_investor_taxes(&investors);
    let referrer_rebates = summarize_referrer_rebates(&investors);

    Ok(FundSettlementRunDetail {
        run,
        investors,
        totals,
        investor_taxes,
        referrer_rebates,
    })
}

pub async fn create_fund_settlement_run(
    State(state): State<AppState>,
    Json(request): Json<CreateFundSettlementRunRequest>,
) -> Result<(StatusCode, Json<FundSettlementRunDetail>), AppError> {
    get_fund_config(&state.db, &request.fund_id).await?;
    let preview = load_fund_settlement_preview(&state.db, request.fund_id).await?;
    let equity = preview
        .latest_equity
        .as_ref()
        .ok_or_else(|| AppError::bad_request("fund settlement requires legacy equity history"))?;
    let basis = preview
        .basis
        .as_ref()
        .ok_or_else(|| AppError::bad_request("fund settlement requires an equity basis"))?;
    let run_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO fund_settlement_runs (
            id, fund_id, equity_event_index, equity, equity_updated_at,
            basis_source, basis_id, basis_updated_at,
            total_deposit, total_units, total_tax, total_referrer_rebate,
            capped_cash_flows, capped_units, capped_cash_amount,
            investor_count, status, status_updated_at, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'draft', ?17, ?18)
        "#,
    )
    .bind(&run_id)
    .bind(&preview.fund_id)
    .bind(equity.event_index)
    .bind(basis.equity)
    .bind(&basis.updated_at)
    .bind(&basis.source)
    .bind(&basis.id)
    .bind(&basis.updated_at)
    .bind(preview.total_deposit)
    .bind(preview.total_units)
    .bind(preview.total_tax)
    .bind(preview.total_referrer_rebate)
    .bind(preview.totals.capped_cash_flows)
    .bind(preview.totals.capped_units)
    .bind(preview.totals.capped_cash_amount)
    .bind(preview.investors.len() as i64)
    .bind(&created_at)
    .bind(&created_at)
    .execute(&mut *tx)
    .await?;

    for investor in &preview.investors {
        sqlx::query(
            r#"
            INSERT INTO fund_settlement_investor_rows (
                run_id, fund_id, investor_name, referrer, deposit, units, ownership,
                gross_equity, profit, tax_threshold, tax_rate, tax,
                referrer_rebate_rate, referrer_rebate, net_equity
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
        )
        .bind(&run_id)
        .bind(&preview.fund_id)
        .bind(&investor.name)
        .bind(&investor.referrer)
        .bind(investor.deposit)
        .bind(investor.units)
        .bind(investor.ownership)
        .bind(investor.gross_equity)
        .bind(investor.profit)
        .bind(investor.tax_threshold)
        .bind(investor.tax_rate)
        .bind(investor.tax)
        .bind(investor.referrer_rebate_rate)
        .bind(investor.referrer_rebate)
        .bind(investor.net_equity)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let totals = preview.totals.clone();
    let investor_taxes = summarize_investor_taxes(&preview.investors);
    let referrer_rebates = summarize_referrer_rebates(&preview.investors);

    Ok((
        StatusCode::CREATED,
        Json(FundSettlementRunDetail {
            run: FundSettlementRun {
                id: run_id,
                fund_id: preview.fund_id,
                equity_event_index: equity.event_index,
                equity: basis.equity,
                equity_updated_at: basis.updated_at.clone(),
                basis_source: basis.source.clone(),
                basis_id: basis.id.clone(),
                basis_updated_at: basis.updated_at.clone(),
                total_deposit: preview.total_deposit,
                total_units: preview.total_units,
                total_tax: preview.total_tax,
                total_referrer_rebate: preview.total_referrer_rebate,
                capped_cash_flows: preview.totals.capped_cash_flows,
                capped_units: preview.totals.capped_units,
                capped_cash_amount: preview.totals.capped_cash_amount,
                investor_count: preview.investors.len() as i64,
                status: "draft".to_string(),
                status_updated_at: Some(created_at.clone()),
                created_at,
            },
            totals,
            investor_taxes,
            referrer_rebates,
            investors: preview.investors,
        }),
    ))
}

pub async fn confirm_fund_settlement_run(
    State(state): State<AppState>,
    Json(request): Json<UpdateFundSettlementRunRequest>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    confirm_fund_settlement_run_status(&state.db, &request.run_id).await?;
    get_fund_settlement_run_detail_by_id(&state.db, &request.run_id)
        .await
        .map(Json)
}

pub async fn void_fund_settlement_run(
    State(state): State<AppState>,
    Json(request): Json<UpdateFundSettlementRunRequest>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    update_fund_settlement_run_status(&state.db, &request.run_id, "voided").await?;
    get_fund_settlement_run_detail_by_id(&state.db, &request.run_id)
        .await
        .map(Json)
}

async fn update_fund_settlement_run_status(
    db: &SqlitePool,
    run_id: &str,
    status: &str,
) -> Result<(), AppError> {
    let status_updated_at = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET status = ?1, status_updated_at = ?2
        WHERE id = ?3 AND status = 'draft'
        "#,
    )
    .bind(status)
    .bind(&status_updated_at)
    .bind(run_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request(
            "settlement run must exist and be in draft status",
        ));
    }

    Ok(())
}

async fn confirm_fund_settlement_run_status(db: &SqlitePool, run_id: &str) -> Result<(), AppError> {
    let status_updated_at = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET status = 'confirmed', status_updated_at = ?1
        WHERE id = ?2
          AND status = 'draft'
          AND NOT EXISTS (
              SELECT 1
              FROM fund_settlement_runs confirmed
              WHERE confirmed.fund_id = fund_settlement_runs.fund_id
                AND confirmed.basis_source = fund_settlement_runs.basis_source
                AND confirmed.basis_id = fund_settlement_runs.basis_id
                AND confirmed.status = 'confirmed'
          )
          AND NOT EXISTS (
              SELECT 1
              FROM fund_settlement_investor_rows investor
              WHERE investor.run_id = fund_settlement_runs.id
                AND investor.units < -1e-9
          )
        "#,
    )
    .bind(&status_updated_at)
    .bind(run_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request(
            "settlement run must be draft, non-duplicate, and have no negative investor units",
        ));
    }

    Ok(())
}

async fn load_fund_settlement_preview(
    db: &SqlitePool,
    fund_id: String,
) -> Result<FundSettlementPreview, AppError> {
    let orders = sqlx::query_as::<_, SettlementOrder>(
        r#"
        SELECT event_index, investor_name, deposit, updated_at
        FROM fund_statement_orders
        WHERE fund_id = ?1
        ORDER BY updated_at ASC, event_index ASC
        "#,
    )
    .bind(&fund_id)
    .fetch_all(db)
    .await?;

    let equity_points = sqlx::query_as::<_, FundStatementEquity>(
        r#"
        SELECT event_index, equity, updated_at
        FROM fund_statement_equity
        WHERE fund_id = ?1
        ORDER BY updated_at ASC, event_index ASC
        "#,
    )
    .bind(&fund_id)
    .fetch_all(db)
    .await?;

    let investors = sqlx::query_as::<_, FundStatementInvestor>(
        r#"
        SELECT name, referrer, tax_rate, referrer_rebate_rate, tax_threshold,
               updated_at, source_event_index
        FROM fund_statement_investors
        WHERE fund_id = ?1
        "#,
    )
    .bind(&fund_id)
    .fetch_all(db)
    .await?;
    let latest_nav = sqlx::query_as::<_, FundNavSnapshot>(
        r#"
        SELECT id, fund_id, account_id, equity, target_currency, positions_count,
               unpriced_positions, created_at
        FROM fund_nav_snapshots
        WHERE fund_id = ?1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(&fund_id)
    .fetch_optional(db)
    .await?;

    Ok(build_settlement_preview(
        fund_id,
        orders,
        equity_points,
        investors,
        latest_nav,
    ))
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

fn build_settlement_preview(
    fund_id: String,
    orders: Vec<SettlementOrder>,
    equity_points: Vec<FundStatementEquity>,
    investor_settings: Vec<FundStatementInvestor>,
    latest_nav: Option<FundNavSnapshot>,
) -> FundSettlementPreview {
    let latest_equity = equity_points.last().cloned();
    let basis = settlement_basis(latest_equity.as_ref(), latest_nav.as_ref());
    let final_equity = basis.as_ref().map(|item| item.equity).unwrap_or(0.0);
    let mut investors = investor_settings
        .into_iter()
        .map(|item| {
            (
                item.name.clone(),
                SettlementInvestorState {
                    name: item.name,
                    referrer: item.referrer,
                    tax_threshold: item.tax_threshold.unwrap_or(0.0),
                    tax_rate: item.tax_rate.unwrap_or(0.0),
                    referrer_rebate_rate: item.referrer_rebate_rate.unwrap_or(0.0),
                    ..SettlementInvestorState::default()
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut total_deposit = 0.0;
    let cash_flows = build_cash_flow_ledger(&orders, &equity_points);
    let capped_cash_flows = cash_flows
        .iter()
        .filter(|item| item.capped_units > 1e-9)
        .count() as i64;
    let capped_units = cash_flows.iter().map(|item| item.capped_units).sum();
    let capped_cash_amount = cash_flows.iter().map(|item| item.capped_cash_amount).sum();

    for order in cash_flows {
        let investor = investors
            .entry(order.investor_name.clone())
            .or_insert_with(|| SettlementInvestorState {
                name: order.investor_name,
                ..SettlementInvestorState::default()
            });

        investor.deposit += order.effective_deposit;
        investor.units += order.unit_delta;
        total_deposit += order.effective_deposit;
    }

    let total_units = investors.values().map(|item| item.units).sum::<f64>();
    let mut rows = investors
        .into_values()
        .map(|investor| {
            let ownership = if total_units > 0.0 {
                investor.units / total_units
            } else {
                0.0
            };
            let gross_equity = final_equity * ownership;
            let profit = gross_equity - investor.deposit;
            let taxable_profit = (profit - investor.tax_threshold).max(0.0);
            let tax = taxable_profit * investor.tax_rate;
            let referrer_rebate = tax * investor.referrer_rebate_rate;

            FundInvestorSettlement {
                name: investor.name,
                referrer: investor.referrer,
                deposit: investor.deposit,
                units: investor.units,
                ownership,
                gross_equity,
                profit,
                tax_threshold: investor.tax_threshold,
                tax_rate: investor.tax_rate,
                tax,
                referrer_rebate_rate: investor.referrer_rebate_rate,
                referrer_rebate,
                net_equity: gross_equity - tax,
            }
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        right
            .gross_equity
            .total_cmp(&left.gross_equity)
            .then_with(|| left.name.cmp(&right.name))
    });

    let totals = settlement_totals_with_capped_values(
        summarize_settlement_totals(&rows),
        capped_cash_flows,
        capped_units,
        capped_cash_amount,
    );

    FundSettlementPreview {
        fund_id,
        latest_equity,
        basis,
        total_deposit,
        total_units,
        total_tax: rows.iter().map(|item| item.tax).sum(),
        total_referrer_rebate: rows.iter().map(|item| item.referrer_rebate).sum(),
        totals,
        investor_taxes: summarize_investor_taxes(&rows),
        referrer_rebates: summarize_referrer_rebates(&rows),
        investors: rows,
    }
}

fn reconcile_fund_equity(
    legacy: &FundStatementEquity,
    nav: &FundNavSnapshot,
) -> FundEquityReconciliation {
    let delta = nav.equity - legacy.equity;
    let delta_rate = if legacy.equity != 0.0 {
        Some(delta / legacy.equity)
    } else {
        None
    };

    FundEquityReconciliation {
        legacy_equity: legacy.equity,
        legacy_updated_at: legacy.updated_at.clone(),
        nav_equity: nav.equity,
        nav_created_at: nav.created_at.clone(),
        delta,
        delta_rate,
    }
}

fn build_cash_flow_ledger(
    orders: &[SettlementOrder],
    equity_points: &[FundStatementEquity],
) -> Vec<FundStatementOrder> {
    let mut investor_units = HashMap::<String, f64>::new();
    let mut total_units = 0.0;
    let mut latest_order_equity = 0.0;
    let mut equity_index = 0usize;

    orders
        .iter()
        .map(|order| {
            while equity_index < equity_points.len()
                && equity_points[equity_index].updated_at <= order.updated_at
            {
                latest_order_equity = equity_points[equity_index].equity;
                equity_index += 1;
            }

            // ASSUMPTION: Legacy fund statements store investor cash flows and
            // fund equity, but not explicit share issuance records. If the
            // approximation issues too many or too few units for an old flow,
            // only derived ledgers and settlement previews change; raw imported
            // events remain unchanged for audit and re-derivation.
            let nav_per_unit = if total_units > 0.0 && latest_order_equity > 0.0 {
                latest_order_equity / total_units
            } else {
                1.0
            };
            let requested_unit_delta = order.deposit / nav_per_unit;
            let investor_units_after = investor_units
                .entry(order.investor_name.clone())
                .or_default();
            let current_investor_units = *investor_units_after;
            let unit_delta = capped_unit_delta(requested_unit_delta, current_investor_units);
            let capped_units = (unit_delta - requested_unit_delta).max(0.0);
            let effective_deposit = unit_delta * nav_per_unit;
            let capped_cash_amount = (effective_deposit - order.deposit).max(0.0);
            *investor_units_after += unit_delta;
            total_units += unit_delta;

            FundStatementOrder {
                event_index: order.event_index,
                investor_name: order.investor_name.clone(),
                deposit: order.deposit,
                effective_deposit,
                capped_cash_amount,
                direction: cash_flow_direction(order.deposit).to_string(),
                nav_per_unit,
                requested_unit_delta,
                unit_delta,
                capped_units,
                investor_units_after: *investor_units_after,
                total_units_after: total_units,
                updated_at: order.updated_at.clone(),
            }
        })
        .collect()
}

fn capped_unit_delta(requested_unit_delta: f64, current_investor_units: f64) -> f64 {
    if requested_unit_delta < -current_investor_units {
        -current_investor_units
    } else {
        requested_unit_delta
    }
}

fn cash_flow_direction(deposit: f64) -> &'static str {
    if deposit < 0.0 { "outflow" } else { "inflow" }
}

async fn load_tax_threshold_adjustments(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<FundTaxThresholdAdjustment>, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, updated_at, payload
        FROM fund_statement_events
        WHERE fund_id = ?1 AND event_type = 'investor'
        ORDER BY updated_at ASC, event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .filter_map(tax_threshold_adjustment_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn tax_threshold_adjustment_from_row(
    row: FundStatementEventPayloadRow,
) -> Option<Result<FundTaxThresholdAdjustment, serde_json::Error>> {
    let payload = match serde_json::from_str::<FundStatementEventPayload>(&row.payload) {
        Ok(payload) => payload,
        Err(error) => return Some(Err(error)),
    };
    payload.investor.and_then(|investor| {
        investor
            .add_tax_threshold
            .map(|amount| FundTaxThresholdAdjustment {
                event_index: row.event_index,
                investor_name: investor.name,
                amount,
                comment: payload.comment,
                updated_at: row.updated_at,
            })
            .map(Ok)
    })
}

fn settlement_basis(
    legacy: Option<&FundStatementEquity>,
    nav: Option<&FundNavSnapshot>,
) -> Option<FundSettlementBasis> {
    if let Some(nav) = nav {
        return Some(FundSettlementBasis {
            source: "live_nav".to_string(),
            id: nav.id.clone(),
            equity: nav.equity,
            updated_at: nav.created_at.clone(),
        });
    }

    legacy.map(|item| FundSettlementBasis {
        source: "legacy_statement".to_string(),
        id: item.event_index.to_string(),
        equity: item.equity,
        updated_at: item.updated_at.clone(),
    })
}

fn settlement_run_csv(detail: &FundSettlementRunDetail) -> String {
    let mut rows = vec![
        vec![
            "run_id".to_string(),
            detail.run.id.clone(),
            "fund_id".to_string(),
            detail.run.fund_id.clone(),
            "status".to_string(),
            detail.run.status.clone(),
            "status_updated_at".to_string(),
            detail.run.status_updated_at.clone().unwrap_or_default(),
            "basis_source".to_string(),
            detail.run.basis_source.clone(),
            "basis_id".to_string(),
            detail.run.basis_id.clone(),
            "capped_cash_flows".to_string(),
            detail.run.capped_cash_flows.to_string(),
        ],
        vec![
            "equity".to_string(),
            detail.run.equity.to_string(),
            "equity_updated_at".to_string(),
            detail.run.equity_updated_at.clone(),
            "created_at".to_string(),
            detail.run.created_at.clone(),
            "capped_units".to_string(),
            detail.run.capped_units.to_string(),
            "capped_cash_amount".to_string(),
            detail.run.capped_cash_amount.to_string(),
        ],
        vec![
            "gross_equity".to_string(),
            detail.totals.gross_equity.to_string(),
            "net_equity".to_string(),
            detail.totals.net_equity.to_string(),
            "retained_tax".to_string(),
            detail.totals.retained_tax.to_string(),
        ],
        Vec::new(),
        vec![
            "investor".to_string(),
            "referrer".to_string(),
            "deposit".to_string(),
            "units".to_string(),
            "ownership".to_string(),
            "gross_equity".to_string(),
            "profit".to_string(),
            "tax_threshold".to_string(),
            "tax_rate".to_string(),
            "tax".to_string(),
            "referrer_rebate_rate".to_string(),
            "referrer_rebate".to_string(),
            "net_equity".to_string(),
        ],
    ];

    rows.extend(detail.investors.iter().map(|investor| {
        vec![
            investor.name.clone(),
            investor.referrer.clone().unwrap_or_default(),
            investor.deposit.to_string(),
            investor.units.to_string(),
            investor.ownership.to_string(),
            investor.gross_equity.to_string(),
            investor.profit.to_string(),
            investor.tax_threshold.to_string(),
            investor.tax_rate.to_string(),
            investor.tax.to_string(),
            investor.referrer_rebate_rate.to_string(),
            investor.referrer_rebate.to_string(),
            investor.net_equity.to_string(),
        ]
    }));
    rows.push(Vec::new());
    rows.push(vec!["tax_investor".to_string(), "tax".to_string()]);
    rows.extend(
        detail
            .investor_taxes
            .iter()
            .map(|tax| vec![tax.investor.clone(), tax.tax.to_string()]),
    );
    rows.push(Vec::new());
    rows.push(vec!["referrer".to_string(), "rebate".to_string()]);
    rows.extend(
        detail
            .referrer_rebates
            .iter()
            .map(|rebate| vec![rebate.referrer.clone(), rebate.rebate.to_string()]),
    );

    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| csv_cell(&cell))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn summarize_settlement_totals(investors: &[FundInvestorSettlement]) -> FundSettlementTotals {
    let gross_equity = investors.iter().map(|item| item.gross_equity).sum();
    let net_equity = investors.iter().map(|item| item.net_equity).sum();
    let tax = investors.iter().map(|item| item.tax).sum();
    let referrer_rebate = investors.iter().map(|item| item.referrer_rebate).sum();
    let overdrawn_investors = investors.iter().filter(|item| item.units < -1e-9).count() as i64;

    FundSettlementTotals {
        gross_equity,
        net_equity,
        tax,
        referrer_rebate,
        retained_tax: tax - referrer_rebate,
        overdrawn_investors,
        capped_cash_flows: 0,
        capped_units: 0.0,
        capped_cash_amount: 0.0,
    }
}

fn settlement_totals_with_capped_run(
    totals: FundSettlementTotals,
    run: &FundSettlementRun,
) -> FundSettlementTotals {
    settlement_totals_with_capped_values(
        totals,
        run.capped_cash_flows,
        run.capped_units,
        run.capped_cash_amount,
    )
}

fn settlement_totals_with_capped_values(
    mut totals: FundSettlementTotals,
    capped_cash_flows: i64,
    capped_units: f64,
    capped_cash_amount: f64,
) -> FundSettlementTotals {
    totals.capped_cash_flows = capped_cash_flows;
    totals.capped_units = capped_units;
    totals.capped_cash_amount = capped_cash_amount;
    totals
}

fn summarize_investor_taxes(investors: &[FundInvestorSettlement]) -> Vec<FundInvestorTax> {
    let mut rows = investors
        .iter()
        .filter(|investor| investor.tax > 0.0)
        .map(|investor| FundInvestorTax {
            investor: investor.name.clone(),
            tax: investor.tax,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .tax
            .total_cmp(&left.tax)
            .then_with(|| left.investor.cmp(&right.investor))
    });
    rows
}

fn summarize_referrer_rebates(investors: &[FundInvestorSettlement]) -> Vec<FundReferrerRebate> {
    let mut rebates = HashMap::<String, f64>::new();

    for investor in investors {
        if let Some(referrer) = &investor.referrer {
            if investor.referrer_rebate > 0.0 {
                *rebates.entry(referrer.clone()).or_default() += investor.referrer_rebate;
            }
        }
    }

    let mut rows = rebates
        .into_iter()
        .map(|(referrer, rebate)| FundReferrerRebate { referrer, rebate })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .rebate
            .total_cmp(&left.rebate)
            .then_with(|| left.referrer.cmp(&right.referrer))
    });
    rows
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
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
    fn previews_settlement_with_nav_issued_units_and_tax() {
        let preview = build_settlement_preview(
            "fund".to_string(),
            vec![
                SettlementOrder {
                    event_index: 1,
                    investor_name: "Alice".to_string(),
                    deposit: 100.0,
                    updated_at: "2025-01-01T00:00:00+00:00".to_string(),
                },
                SettlementOrder {
                    event_index: 2,
                    investor_name: "Bob".to_string(),
                    deposit: 120.0,
                    updated_at: "2025-01-02T00:00:00+00:00".to_string(),
                },
            ],
            vec![
                FundStatementEquity {
                    event_index: 1,
                    equity: 100.0,
                    updated_at: "2025-01-01T12:00:00+00:00".to_string(),
                },
                FundStatementEquity {
                    event_index: 2,
                    equity: 240.0,
                    updated_at: "2025-01-03T00:00:00+00:00".to_string(),
                },
            ],
            vec![FundStatementInvestor {
                name: "Bob".to_string(),
                referrer: Some("Alice".to_string()),
                tax_rate: Some(0.2),
                referrer_rebate_rate: Some(0.25),
                tax_threshold: Some(10.0),
                updated_at: "2025-01-02T00:00:00+00:00".to_string(),
                source_event_index: 2,
            }],
            None,
        );

        let alice = preview
            .investors
            .iter()
            .find(|item| item.name == "Alice")
            .unwrap();
        let bob = preview
            .investors
            .iter()
            .find(|item| item.name == "Bob")
            .unwrap();

        assert_close(preview.total_units, 220.0);
        assert_eq!(
            preview.basis.as_ref().map(|item| item.source.as_str()),
            Some("legacy_statement")
        );
        assert_close(alice.gross_equity, 240.0 * 100.0 / 220.0);
        assert_close(bob.gross_equity, 240.0 * 120.0 / 220.0);
        assert_close(bob.tax, (bob.profit - 10.0) * 0.2);
        assert_close(bob.referrer_rebate, bob.tax * 0.25);
        assert_close(bob.net_equity, bob.gross_equity - bob.tax);
    }

    #[test]
    fn previews_settlement_against_latest_live_nav_basis() {
        let preview = build_settlement_preview(
            "fund".to_string(),
            vec![SettlementOrder {
                event_index: 1,
                investor_name: "Alice".to_string(),
                deposit: 100.0,
                updated_at: "2025-01-01T00:00:00+00:00".to_string(),
            }],
            vec![FundStatementEquity {
                event_index: 1,
                equity: 100.0,
                updated_at: "2025-01-01T12:00:00+00:00".to_string(),
            }],
            Vec::new(),
            Some(FundNavSnapshot {
                id: "nav-1".to_string(),
                fund_id: "fund".to_string(),
                account_id: "virtual/fund".to_string(),
                equity: 125.0,
                target_currency: "USD".to_string(),
                positions_count: 1,
                unpriced_positions: 0,
                created_at: "2025-01-02T00:00:00+00:00".to_string(),
            }),
        );

        assert_eq!(
            preview.basis.as_ref().map(|item| item.source.as_str()),
            Some("live_nav")
        );
        assert_close(preview.totals.gross_equity, 125.0);
    }

    #[test]
    fn derives_cash_flow_units_for_outflows() {
        let ledger = build_cash_flow_ledger(
            &[
                SettlementOrder {
                    event_index: 1,
                    investor_name: "Alice".to_string(),
                    deposit: 100.0,
                    updated_at: "2025-01-01T00:00:00+00:00".to_string(),
                },
                SettlementOrder {
                    event_index: 2,
                    investor_name: "Alice".to_string(),
                    deposit: -20.0,
                    updated_at: "2025-01-02T00:00:00+00:00".to_string(),
                },
                SettlementOrder {
                    event_index: 3,
                    investor_name: "Alice".to_string(),
                    deposit: -400.0,
                    updated_at: "2025-01-03T00:00:00+00:00".to_string(),
                },
            ],
            &[FundStatementEquity {
                event_index: 1,
                equity: 200.0,
                updated_at: "2025-01-01T12:00:00+00:00".to_string(),
            }],
        );

        assert_close(ledger[0].nav_per_unit, 1.0);
        assert_close(ledger[0].unit_delta, 100.0);
        assert_eq!(ledger[0].direction, "inflow");
        assert_close(ledger[1].nav_per_unit, 2.0);
        assert_close(ledger[1].unit_delta, -10.0);
        assert_eq!(ledger[1].direction, "outflow");
        assert_close(ledger[1].investor_units_after, 90.0);
        assert_close(ledger[1].total_units_after, 90.0);
        assert_close(ledger[2].requested_unit_delta, -180.0);
        assert_close(ledger[2].unit_delta, -90.0);
        assert_close(ledger[2].capped_units, 90.0);
        assert_close(ledger[2].effective_deposit, -200.0);
        assert_close(ledger[2].capped_cash_amount, 200.0);
        assert_close(ledger[2].investor_units_after, 0.0);
        assert_close(ledger[2].total_units_after, 0.0);
    }

    #[test]
    fn escapes_csv_cells() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn extracts_tax_threshold_adjustment_from_statement_event() {
        let adjustment = tax_threshold_adjustment_from_row(FundStatementEventPayloadRow {
            event_index: 1514,
            updated_at: "2025-09-30T15:59:59.999000+00:00".to_string(),
            payload: r#"{"comment":"快捷申报免税 张秦 108.06756441281664","investor":{"name":"张秦","add_tax_threshold":108.06756441281664}}"#.to_string(),
        })
        .unwrap()
        .unwrap();

        assert_eq!(adjustment.event_index, 1514);
        assert_eq!(adjustment.investor_name, "张秦");
        assert_close(adjustment.amount, 108.06756441281664);
        assert_eq!(
            adjustment.comment.as_deref(),
            Some("快捷申报免税 张秦 108.06756441281664")
        );
    }

    #[test]
    fn summarizes_referrer_rebates() {
        let rows = summarize_referrer_rebates(&[
            investor_rebate("Alice", Some("Carol"), 3.0),
            investor_rebate("Bob", Some("Carol"), 5.0),
            investor_rebate("Dan", Some("Eve"), 0.0),
            investor_rebate("Finn", Some("Grace"), 7.0),
            investor_rebate("Heidi", None, 11.0),
        ]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].referrer, "Carol");
        assert_close(rows[0].rebate, 8.0);
        assert_eq!(rows[1].referrer, "Grace");
        assert_close(rows[1].rebate, 7.0);
    }

    #[test]
    fn summarizes_investor_taxes() {
        let rows = summarize_investor_taxes(&[
            investor_tax("Alice", 2.0),
            investor_tax("Bob", 0.0),
            investor_tax("Carol", 5.0),
        ]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].investor, "Carol");
        assert_close(rows[0].tax, 5.0);
        assert_eq!(rows[1].investor, "Alice");
        assert_close(rows[1].tax, 2.0);
    }

    #[test]
    fn summarizes_settlement_totals() {
        let rows = [
            investor_total("Alice", 10.0, 8.0, 2.0, 0.5),
            investor_total("Bob", 20.0, 17.0, 3.0, 1.0),
            FundInvestorSettlement {
                units: -1.0,
                ..investor_total("Carol", 0.0, 0.0, 0.0, 0.0)
            },
        ];
        let totals = summarize_settlement_totals(&rows);

        assert_close(totals.gross_equity, 30.0);
        assert_close(totals.net_equity, 25.0);
        assert_close(totals.tax, 5.0);
        assert_close(totals.referrer_rebate, 1.5);
        assert_close(totals.retained_tax, 3.5);
        assert_eq!(totals.overdrawn_investors, 1);
    }

    #[test]
    fn reconciles_legacy_equity_with_latest_nav() {
        let reconciliation = reconcile_fund_equity(
            &FundStatementEquity {
                event_index: 1,
                equity: 100.0,
                updated_at: "2025-01-01T00:00:00+00:00".to_string(),
            },
            &FundNavSnapshot {
                id: "nav".to_string(),
                fund_id: "fund".to_string(),
                account_id: "virtual/fund".to_string(),
                equity: 112.5,
                target_currency: "USD".to_string(),
                positions_count: 1,
                unpriced_positions: 0,
                created_at: "2025-01-02T00:00:00+00:00".to_string(),
            },
        );

        assert_close(reconciliation.delta, 12.5);
        assert_eq!(reconciliation.delta_rate, Some(0.125));
    }

    fn investor_rebate(
        name: &str,
        referrer: Option<&str>,
        referrer_rebate: f64,
    ) -> FundInvestorSettlement {
        FundInvestorSettlement {
            name: name.to_string(),
            referrer: referrer.map(str::to_string),
            deposit: 0.0,
            units: 0.0,
            ownership: 0.0,
            gross_equity: 0.0,
            profit: 0.0,
            tax_threshold: 0.0,
            tax_rate: 0.0,
            tax: 0.0,
            referrer_rebate_rate: 0.0,
            referrer_rebate,
            net_equity: 0.0,
        }
    }

    fn investor_tax(name: &str, tax: f64) -> FundInvestorSettlement {
        FundInvestorSettlement {
            tax,
            ..investor_rebate(name, None, 0.0)
        }
    }

    fn investor_total(
        name: &str,
        gross_equity: f64,
        net_equity: f64,
        tax: f64,
        referrer_rebate: f64,
    ) -> FundInvestorSettlement {
        FundInvestorSettlement {
            gross_equity,
            net_equity,
            tax,
            referrer_rebate,
            ..investor_rebate(name, None, referrer_rebate)
        }
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "{left} != {right}");
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
