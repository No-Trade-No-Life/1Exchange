use std::collections::HashMap;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::{AppError, AppState, auth, models::AccountInfo, rates, virtual_accounts};

const DEFAULT_POLL_INTERVAL_SECONDS: i64 = 600;
const FUND_SCAN_INTERVAL_SECONDS: u64 = 60;
const FUND_SETTLEMENT_MODEL_EVENT_STATE: &str = "event_state_v1";

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
    pub owner_id: String,
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
pub struct FundStatementEventQuery {
    fund_id: String,
    event_index: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateFundStatementEventRequest {
    fund_id: String,
    event_index: i64,
    event_type: String,
    updated_at: String,
    payload: serde_json::Value,
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
pub struct ConfirmFundSettlementRequest {
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
    investor_ledger: Vec<FundStatementInvestorLedger>,
    event_state: FundStatementEventState,
    recent_orders: Vec<FundStatementOrder>,
    latest_equity: Option<FundStatementEquity>,
    reconciliation: Option<FundEquityReconciliation>,
    tax_modes: Vec<FundStatementTaxMode>,
    tax_threshold_adjustments: Vec<FundTaxThresholdAdjustment>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundStatementEvent {
    event_index: i64,
    event_type: String,
    updated_at: String,
    payload: String,
}

#[derive(Debug, Serialize)]
pub struct FundStatementEventPage {
    events: Vec<FundStatementEvent>,
    total: i64,
    limit: i64,
    offset: i64,
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

#[derive(Debug, Serialize)]
pub struct FundStatementInvestorLedger {
    investor_name: String,
    deposit: f64,
    effective_deposit: f64,
    inflow_amount: f64,
    outflow_amount: f64,
    capped_cash_amount: f64,
    units: f64,
    flow_count: i64,
    last_flow_at: String,
}

#[derive(Debug, Serialize)]
pub struct FundStatementEventState {
    total_assets: f64,
    total_deposit: f64,
    total_share: f64,
    unit_price: f64,
    total_tax: f64,
    total_taxed: f64,
    total_profit: f64,
    investors: Vec<FundStatementEventInvestor>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FundStatementEventInvestor {
    name: String,
    referrer: Option<String>,
    deposit: f64,
    share: f64,
    share_ratio: f64,
    tax_threshold: f64,
    tax_rate: f64,
    tax: f64,
    taxable: f64,
    pre_tax_assets: f64,
    after_tax_assets: f64,
    after_tax_share: f64,
    referrer_rebate_rate: f64,
    claimed_referrer_rebate: f64,
    taxed: f64,
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
    referrer_rebate_received: f64,
    tax_account_credit: f64,
    capped_cash_amount: f64,
    net_equity: f64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FundSettlementRun {
    id: String,
    fund_id: String,
    settlement_model: String,
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
    referrer_rebate_received: f64,
    tax_account_credit: f64,
    capped_cash_amount: f64,
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
            referrer_rebate_received: row.referrer_rebate_received,
            tax_account_credit: row.tax_account_credit,
            capped_cash_amount: row.capped_cash_amount,
            net_equity: row.net_equity,
        }
    }
}

#[derive(Debug, FromRow)]
struct FundStatementEventPayloadRow {
    event_index: i64,
    updated_at: String,
    payload: String,
}

#[derive(Debug, FromRow)]
struct FundStatementEventProjectionRow {
    event_index: i64,
    event_type: String,
    updated_at: String,
    payload: String,
}

#[derive(Debug, Deserialize)]
struct FundStatementEventPayload {
    #[serde(rename = "type")]
    event_type: Option<String>,
    comment: Option<String>,
    fund_equity: Option<FundStatementEquityPayload>,
    order: Option<FundStatementOrderPayload>,
    investor: Option<FundStatementInvestorPayload>,
}

#[derive(Debug, Deserialize)]
struct FundStatementEquityPayload {
    equity: f64,
}

#[derive(Debug, Deserialize)]
struct FundStatementOrderPayload {
    name: String,
    deposit: f64,
}

#[derive(Debug, Deserialize)]
struct FundStatementInvestorPayload {
    name: String,
    tax_rate: Option<f64>,
    add_tax_threshold: Option<f64>,
    referrer: Option<String>,
    referrer_rebate_rate: Option<f64>,
}

#[derive(Default)]
struct FundStatementInvestorProjection {
    referrer: Option<String>,
    tax_rate: Option<f64>,
    referrer_rebate_rate: Option<f64>,
    tax_threshold: Option<f64>,
    updated_at: String,
    source_event_index: i64,
}

#[derive(Clone, Debug, Default)]
struct YuanFundState {
    total_assets: f64,
    total_taxed: f64,
    investors: HashMap<String, YuanInvestor>,
    derived: HashMap<String, YuanInvestorDerived>,
}

#[derive(Clone, Debug, Default)]
struct YuanInvestor {
    name: String,
    referrer: Option<String>,
    deposit: f64,
    share: f64,
    tax_threshold: f64,
    tax_rate: f64,
    referrer_rebate_rate: f64,
    claimed_referrer_rebate: f64,
    taxed: f64,
    avg_cost_price: f64,
}

#[derive(Clone, Debug, Default)]
struct YuanInvestorDerived {
    share_ratio: f64,
    tax: f64,
    taxable: f64,
    pre_tax_assets: f64,
    after_tax_assets: f64,
    after_tax_share: f64,
}

#[derive(Debug)]
struct FundValuation {
    equity: f64,
    positions_count: i64,
    unpriced_positions: i64,
}

pub async fn list_funds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<FundConfig>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    Ok(Json(list_fund_configs(&state.db, &user.user_id).await?))
}

pub async fn create_fund(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateFundRequest>,
) -> Result<(StatusCode, Json<FundConfig>), AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    validate_fund_request(&request)?;
    virtual_accounts::require_virtual_account(&state.db, &user.user_id, request.account_id.trim())
        .await?;

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
        INSERT INTO funds (id, owner_id, name, account_id, enabled, target_currency, poll_interval_seconds)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            account_id = excluded.account_id,
            enabled = excluded.enabled,
            target_currency = excluded.target_currency,
            poll_interval_seconds = excluded.poll_interval_seconds,
            updated_at = CURRENT_TIMESTAMP
        WHERE funds.owner_id = excluded.owner_id
        "#,
    )
    .bind(&id)
    .bind(&user.user_id)
    .bind(request.name.trim())
    .bind(request.account_id.trim())
    .bind(request.enabled)
    .bind(target_currency)
    .bind(poll_interval_seconds)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(get_fund_config(&state.db, &user.user_id, &id).await?),
    ))
}

pub async fn list_fund_nav(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundNavQuery>,
) -> Result<Json<Vec<FundNavSnapshot>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let rows = if let Some(fund_id) = query.fund_id {
        get_fund_config(&state.db, &user.user_id, &fund_id).await?;
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
            WHERE fund_id IN (SELECT id FROM funds WHERE owner_id = ?1)
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .bind(&user.user_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(rows))
}

pub async fn list_fund_statement_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundStatementEventQuery>,
) -> Result<Json<FundStatementEventPage>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    let (total,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM fund_statement_events WHERE fund_id = ?1")
            .bind(&query.fund_id)
            .fetch_one(&state.db)
            .await?;
    let events = sqlx::query_as::<_, FundStatementEvent>(
        r#"
            SELECT event_index, event_type, updated_at, payload
            FROM fund_statement_events
            WHERE fund_id = ?1
            ORDER BY event_index DESC
            LIMIT ?2 OFFSET ?3
            "#,
    )
    .bind(&query.fund_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(FundStatementEventPage {
        events,
        total,
        limit,
        offset,
    }))
}

pub async fn update_fund_statement_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateFundStatementEventRequest>,
) -> Result<Json<FundStatementEvent>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
    validate_fund_statement_event_request(&request)?;

    let payload = serde_json::to_string(&request.payload)?;
    let mut tx = state.db.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE fund_statement_events
        SET event_type = ?1, updated_at = ?2, payload = ?3
        WHERE fund_id = ?4 AND event_index = ?5
        "#,
    )
    .bind(request.event_type.trim())
    .bind(request.updated_at.trim())
    .bind(&payload)
    .bind(&request.fund_id)
    .bind(request.event_index)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request("fund statement event not found"));
    }

    rebuild_fund_statement_derived_tables(&mut tx, &request.fund_id).await?;
    tx.commit().await?;
    Ok(Json(
        get_fund_statement_event(&state.db, &request.fund_id, request.event_index).await?,
    ))
}

pub async fn delete_fund_statement_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundStatementEventQuery>,
) -> Result<StatusCode, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let event_index = query
        .event_index
        .ok_or_else(|| AppError::bad_request("event_index is required"))?;
    get_fund_config(&state.db, &user.user_id, &query.fund_id).await?;

    let mut tx = state.db.begin().await?;
    let result = sqlx::query(
        r#"
        DELETE FROM fund_statement_events
        WHERE fund_id = ?1 AND event_index = ?2
        "#,
    )
    .bind(&query.fund_id)
    .bind(event_index)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request("fund statement event not found"));
    }

    rebuild_fund_statement_derived_tables(&mut tx, &query.fund_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_fund_statement_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundStatementQuery>,
) -> Result<Json<FundStatementSummary>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &query.fund_id).await?;

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
    let ledger = build_cash_flow_ledger(&statement_orders, &statement_equity);
    let investor_ledger = summarize_statement_investor_ledger(&ledger);
    let overdrawn_cash_flows = ledger
        .iter()
        .filter(|item| item.investor_units_after < -1e-9)
        .count() as i64;
    let mut final_investor_units = HashMap::<&str, f64>::new();
    for item in &ledger {
        final_investor_units.insert(item.investor_name.as_str(), item.investor_units_after);
    }
    let overdrawn_investors = final_investor_units
        .values()
        .filter(|units| **units < -1e-9)
        .count() as i64;
    let capped_cash_flows = ledger
        .iter()
        .filter(|item| item.capped_units > 1e-9)
        .count() as i64;
    let capped_units = ledger.iter().map(|item| item.capped_units).sum();
    let capped_cash_amount = ledger.iter().map(|item| item.capped_cash_amount).sum();
    let mut recent_orders = ledger;
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
    let event_state = load_statement_event_state(&state.db, &query.fund_id).await?;

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
        investor_ledger,
        event_state,
        recent_orders,
        latest_equity,
        reconciliation,
        tax_modes,
        tax_threshold_adjustments,
    }))
}

pub async fn get_fund_settlement_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementQuery>,
) -> Result<Json<FundSettlementPreview>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
    Ok(Json(
        load_fund_settlement_preview(&state.db, query.fund_id).await?,
    ))
}

pub async fn list_fund_settlement_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementQuery>,
) -> Result<Json<Vec<FundSettlementRun>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &query.fund_id).await?;

    Ok(Json(
        sqlx::query_as::<_, FundSettlementRun>(
            r#"
            SELECT id, fund_id, settlement_model, equity_event_index, equity, equity_updated_at,
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
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementRunQuery>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_settlement_run_detail_by_id(&state.db, &user.user_id, &query.run_id)
        .await
        .map(Json)
}

pub async fn export_fund_settlement_run_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementRunQuery>,
) -> Result<Response, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let detail =
        get_fund_settlement_run_detail_by_id(&state.db, &user.user_id, &query.run_id).await?;
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
    owner_id: &str,
    run_id: &str,
) -> Result<FundSettlementRunDetail, AppError> {
    let run = sqlx::query_as::<_, FundSettlementRun>(
        r#"
        SELECT id, fund_id, settlement_model, equity_event_index, equity, equity_updated_at,
               basis_source, basis_id, basis_updated_at,
               total_deposit, total_units, total_tax, total_referrer_rebate,
               capped_cash_flows, capped_units, capped_cash_amount,
               investor_count, status, status_updated_at, created_at
        FROM fund_settlement_runs
        WHERE id = ?1
          AND fund_id IN (SELECT id FROM funds WHERE owner_id = ?2)
        "#,
    )
    .bind(run_id)
    .bind(owner_id)
    .fetch_one(db)
    .await?;
    let investors = sqlx::query_as::<_, FundSettlementInvestorRow>(
        r#"
        SELECT investor_name AS name, referrer, deposit, units, ownership,
               gross_equity, profit, tax_threshold, tax_rate, tax,
               referrer_rebate_rate, referrer_rebate, referrer_rebate_received,
               tax_account_credit, capped_cash_amount, net_equity
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
    headers: HeaderMap,
    Json(request): Json<CreateFundSettlementRunRequest>,
) -> Result<(StatusCode, Json<FundSettlementRunDetail>), AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
    let preview = load_fund_settlement_preview(&state.db, request.fund_id).await?;
    let equity = preview
        .latest_equity
        .as_ref()
        .ok_or_else(|| AppError::bad_request("fund settlement requires legacy equity history"))?;
    let basis = preview
        .basis
        .as_ref()
        .ok_or_else(|| AppError::bad_request("fund settlement requires an equity basis"))?;
    reject_duplicate_active_settlement_run(&state.db, &preview.fund_id, basis).await?;
    let run_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO fund_settlement_runs (
            id, fund_id, settlement_model, equity_event_index, equity, equity_updated_at,
            basis_source, basis_id, basis_updated_at,
            total_deposit, total_units, total_tax, total_referrer_rebate,
            capped_cash_flows, capped_units, capped_cash_amount,
            investor_count, status, status_updated_at, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 'draft', ?18, ?19)
        "#,
    )
    .bind(&run_id)
    .bind(&preview.fund_id)
    .bind(FUND_SETTLEMENT_MODEL_EVENT_STATE)
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
                referrer_rebate_rate, referrer_rebate, referrer_rebate_received,
                tax_account_credit, capped_cash_amount, net_equity
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
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
        .bind(investor.referrer_rebate_received)
        .bind(investor.tax_account_credit)
        .bind(investor.capped_cash_amount)
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
                settlement_model: FUND_SETTLEMENT_MODEL_EVENT_STATE.to_string(),
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

pub async fn confirm_fund_settlement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmFundSettlementRequest>,
) -> Result<Json<FundSettlementPreview>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
    let preview = load_fund_settlement_preview(&state.db, request.fund_id).await?;
    let basis = preview
        .basis
        .as_ref()
        .ok_or_else(|| AppError::bad_request("fund settlement requires an equity basis"))?;
    if preview
        .investors
        .iter()
        .any(|investor| investor.units < -1e-9)
    {
        return Err(AppError::bad_request(
            "fund settlement cannot confirm with negative investor units",
        ));
    }

    let updated_at = Utc::now().to_rfc3339();
    let comment = format!(
        "Settlement {} {}",
        settlement_basis_label(&basis.source),
        basis.id
    );
    let mut tx = state.db.begin().await?;
    reject_duplicate_statement_settlement(&mut tx, &preview.fund_id, basis).await?;
    insert_settlement_statement_events(
        &mut tx,
        &preview.fund_id,
        basis.equity,
        &basis.source,
        &basis.id,
        &comment,
        None,
        &updated_at,
    )
    .await?;
    tx.commit().await?;

    Ok(Json(
        load_fund_settlement_preview(&state.db, preview.fund_id).await?,
    ))
}

async fn reject_duplicate_active_settlement_run(
    db: &SqlitePool,
    fund_id: &str,
    basis: &FundSettlementBasis,
) -> Result<(), AppError> {
    let (active_runs,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM fund_settlement_runs
        WHERE fund_id = ?1
          AND settlement_model = ?2
          AND basis_source = ?3
          AND basis_id = ?4
          AND status IN ('draft', 'confirmed')
        "#,
    )
    .bind(fund_id)
    .bind(FUND_SETTLEMENT_MODEL_EVENT_STATE)
    .bind(&basis.source)
    .bind(&basis.id)
    .fetch_one(db)
    .await?;

    if active_runs > 0 {
        return Err(AppError::bad_request(
            "an active settlement run already exists for this model and basis",
        ));
    }

    Ok(())
}

async fn reject_duplicate_statement_settlement(
    tx: &mut Transaction<'_, Sqlite>,
    fund_id: &str,
    basis: &FundSettlementBasis,
) -> Result<(), AppError> {
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM fund_statement_events
        WHERE fund_id = ?1
          AND event_type = 'taxation/v2'
          AND json_extract(payload, '$.settlement_model') = ?2
          AND json_extract(payload, '$.basis_source') = ?3
          AND json_extract(payload, '$.basis_id') = ?4
        "#,
    )
    .bind(fund_id)
    .bind(FUND_SETTLEMENT_MODEL_EVENT_STATE)
    .bind(&basis.source)
    .bind(&basis.id)
    .fetch_one(&mut **tx)
    .await?;

    if count > 0 {
        return Err(AppError::bad_request(
            "settlement has already been confirmed for this basis",
        ));
    }

    Ok(())
}

pub async fn confirm_fund_settlement_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateFundSettlementRunRequest>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    ensure_settlement_run_owner(&state.db, &user.user_id, &request.run_id).await?;
    confirm_fund_settlement_run_status(&state.db, &request.run_id).await?;
    get_fund_settlement_run_detail_by_id(&state.db, &user.user_id, &request.run_id)
        .await
        .map(Json)
}

pub async fn void_fund_settlement_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateFundSettlementRunRequest>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    ensure_settlement_run_owner(&state.db, &user.user_id, &request.run_id).await?;
    update_fund_settlement_run_status(&state.db, &request.run_id, "voided").await?;
    get_fund_settlement_run_detail_by_id(&state.db, &user.user_id, &request.run_id)
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

async fn ensure_settlement_run_owner(
    db: &SqlitePool,
    owner_id: &str,
    run_id: &str,
) -> Result<(), AppError> {
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM fund_settlement_runs run
        JOIN funds fund ON fund.id = run.fund_id
        WHERE run.id = ?1 AND fund.owner_id = ?2
        "#,
    )
    .bind(run_id)
    .bind(owner_id)
    .fetch_one(db)
    .await?;

    if count == 0 {
        return Err(AppError::bad_request("settlement run not found"));
    }

    Ok(())
}

async fn confirm_fund_settlement_run_status(db: &SqlitePool, run_id: &str) -> Result<(), AppError> {
    let status_updated_at = Utc::now().to_rfc3339();
    let mut tx = db.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE fund_settlement_runs
        SET status = 'confirmed', status_updated_at = ?1
        WHERE id = ?2
          AND status = 'draft'
          AND settlement_model = ?3
          AND NOT EXISTS (
              SELECT 1
              FROM fund_settlement_runs confirmed
              WHERE confirmed.fund_id = fund_settlement_runs.fund_id
                AND confirmed.settlement_model = fund_settlement_runs.settlement_model
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
    .bind(FUND_SETTLEMENT_MODEL_EVENT_STATE)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request(
            "settlement run must use the current model, be draft, non-duplicate, and have no negative investor units",
        ));
    }

    let run = select_fund_settlement_run_by_id(&mut tx, run_id).await?;
    insert_confirmed_settlement_event(&mut tx, &run, &status_updated_at).await?;
    tx.commit().await?;

    Ok(())
}

async fn select_fund_settlement_run_by_id(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: &str,
) -> Result<FundSettlementRun, AppError> {
    Ok(sqlx::query_as::<_, FundSettlementRun>(
        r#"
        SELECT id, fund_id, settlement_model, equity_event_index, equity, equity_updated_at,
               basis_source, basis_id, basis_updated_at,
               total_deposit, total_units, total_tax, total_referrer_rebate,
               capped_cash_flows, capped_units, capped_cash_amount,
               investor_count, status, status_updated_at, created_at
        FROM fund_settlement_runs
        WHERE id = ?1
        "#,
    )
    .bind(run_id)
    .fetch_one(&mut **tx)
    .await?)
}

async fn insert_confirmed_settlement_event(
    tx: &mut Transaction<'_, Sqlite>,
    run: &FundSettlementRun,
    updated_at: &str,
) -> Result<(), AppError> {
    let comment = format!("Settlement run {}", run.id);
    insert_settlement_statement_events(
        tx,
        &run.fund_id,
        run.equity,
        &run.basis_source,
        &run.basis_id,
        &comment,
        Some(&run.id),
        updated_at,
    )
    .await
}

async fn insert_settlement_statement_events(
    tx: &mut Transaction<'_, Sqlite>,
    fund_id: &str,
    equity: f64,
    basis_source: &str,
    basis_id: &str,
    comment: &str,
    settlement_run_id: Option<&str>,
    updated_at: &str,
) -> Result<(), AppError> {
    let (equity_event_index,): (i64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(MAX(event_index), -1) + 1
        FROM fund_statement_events
        WHERE fund_id = ?1
        "#,
    )
    .bind(fund_id)
    .fetch_one(&mut **tx)
    .await?;
    let taxation_event_index = equity_event_index + 1;
    let equity_payload = serde_json::json!({
        "comment": comment,
        "fund_equity": { "equity": equity },
        "settlement_run_id": settlement_run_id,
        "settlement_model": FUND_SETTLEMENT_MODEL_EVENT_STATE,
        "basis_source": basis_source,
        "basis_id": basis_id,
    });
    let taxation_payload = serde_json::json!({
        "type": "taxation/v2",
        "comment": comment,
        "settlement_run_id": settlement_run_id,
        "settlement_model": FUND_SETTLEMENT_MODEL_EVENT_STATE,
        "basis_source": basis_source,
        "basis_id": basis_id,
    });

    sqlx::query(
        r#"
        INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
        VALUES (?1, ?2, 'settlement_equity', ?3, ?4)
        "#,
    )
    .bind(fund_id)
    .bind(equity_event_index)
    .bind(updated_at)
    .bind(equity_payload.to_string())
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO fund_statement_equity (fund_id, event_index, equity, updated_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(fund_id)
    .bind(equity_event_index)
    .bind(equity)
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
        VALUES (?1, ?2, 'taxation/v2', ?3, ?4)
        "#,
    )
    .bind(fund_id)
    .bind(taxation_event_index)
    .bind(updated_at)
    .bind(taxation_payload.to_string())
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO fund_statement_tax_modes (fund_id, event_index, mode, comment, updated_at)
        VALUES (?1, ?2, 'taxation/v2', ?3, ?4)
        "#,
    )
    .bind(fund_id)
    .bind(taxation_event_index)
    .bind(comment)
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn load_fund_settlement_preview(
    db: &SqlitePool,
    fund_id: String,
) -> Result<FundSettlementPreview, AppError> {
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
    let event_state = load_statement_event_state(db, &fund_id).await?;

    Ok(build_settlement_preview(
        fund_id,
        equity_points,
        latest_nav,
        event_state,
    ))
}

pub async fn sample_fund_now(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<SampleFundQuery>,
) -> Result<Json<FundNavSnapshot>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let config = get_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
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

pub async fn list_fund_configs(
    db: &SqlitePool,
    owner_id: &str,
) -> Result<Vec<FundConfig>, AppError> {
    Ok(sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.owner_id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.created_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_nav_snapshots s ON s.fund_id = f.id
        WHERE f.owner_id = ?1
        GROUP BY f.id
        ORDER BY f.created_at DESC
        "#,
    )
    .bind(owner_id)
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
        SELECT f.id, f.owner_id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
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

async fn get_fund_config(
    db: &SqlitePool,
    owner_id: &str,
    fund_id: &str,
) -> Result<FundConfig, AppError> {
    sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.owner_id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.created_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_nav_snapshots s ON s.fund_id = f.id
        WHERE f.id = ?1 AND f.owner_id = ?2
        GROUP BY f.id
        "#,
    )
    .bind(fund_id)
    .bind(owner_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

async fn sample_fund(db: &SqlitePool, config: &FundConfig) -> Result<FundNavSnapshot, AppError> {
    let account =
        virtual_accounts::compose_virtual_account_by_id(db, &config.owner_id, &config.account_id)
            .await?
            .ok_or_else(|| {
                AppError::bad_request("fund account must be an enabled virtual account")
            })?;
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
    equity_points: Vec<FundStatementEquity>,
    latest_nav: Option<FundNavSnapshot>,
    event_state: FundStatementEventState,
) -> FundSettlementPreview {
    let latest_equity = equity_points.last().cloned();
    let basis = settlement_basis(latest_equity.as_ref(), latest_nav.as_ref());
    let final_equity = basis.as_ref().map(|item| item.equity).unwrap_or(0.0);

    let total_units = event_state.total_share;
    let mut rows = event_state
        .investors
        .into_iter()
        .map(|investor| {
            let ownership = if total_units > 0.0 {
                investor.share / total_units
            } else {
                0.0
            };
            let gross_equity = final_equity * ownership;
            let profit = gross_equity - investor.tax_threshold;
            let taxable_profit = profit.max(0.0);
            let tax = taxable_profit * investor.tax_rate;
            let referrer_rebate = tax * investor.referrer_rebate_rate;

            FundInvestorSettlement {
                name: investor.name,
                referrer: investor.referrer,
                deposit: investor.deposit,
                units: investor.share,
                ownership,
                gross_equity,
                profit,
                tax_threshold: investor.tax_threshold,
                tax_rate: investor.tax_rate,
                tax,
                referrer_rebate_rate: investor.referrer_rebate_rate,
                referrer_rebate,
                referrer_rebate_received: 0.0,
                tax_account_credit: 0.0,
                capped_cash_amount: 0.0,
                net_equity: 0.0,
            }
        })
        .collect::<Vec<_>>();
    let referrer_rebates = summarize_referrer_rebates(&rows);
    let rebate_received = referrer_rebates
        .iter()
        .map(|item| (item.referrer.clone(), item.rebate))
        .collect::<HashMap<_, _>>();
    let retained_tax = rows
        .iter()
        .map(|item| item.tax - item.referrer_rebate)
        .sum::<f64>();

    for row in &mut rows {
        row.referrer_rebate_received = rebate_received.get(&row.name).copied().unwrap_or_default();
        if row.name == "@tax" {
            row.tax_account_credit = retained_tax;
        }
        row.net_equity =
            row.gross_equity - row.tax + row.referrer_rebate_received + row.tax_account_credit;
    }
    if retained_tax > 0.0 && !rows.iter().any(|row| row.name == "@tax") {
        rows.push(FundInvestorSettlement {
            name: "@tax".to_string(),
            referrer: None,
            deposit: 0.0,
            units: 0.0,
            ownership: 0.0,
            gross_equity: 0.0,
            profit: 0.0,
            tax_threshold: 0.0,
            tax_rate: 0.0,
            tax: 0.0,
            referrer_rebate_rate: 0.0,
            referrer_rebate: 0.0,
            referrer_rebate_received: 0.0,
            tax_account_credit: retained_tax,
            capped_cash_amount: 0.0,
            net_equity: retained_tax,
        });
    }

    rows.sort_by(|left, right| {
        right
            .gross_equity
            .total_cmp(&left.gross_equity)
            .then_with(|| left.name.cmp(&right.name))
    });

    let totals =
        settlement_totals_with_capped_values(summarize_settlement_totals(&rows), 0, 0.0, 0.0);

    FundSettlementPreview {
        fund_id,
        latest_equity,
        basis,
        total_deposit: event_state.total_deposit,
        total_units,
        total_tax: rows.iter().map(|item| item.tax).sum(),
        total_referrer_rebate: rows.iter().map(|item| item.referrer_rebate).sum(),
        totals,
        investor_taxes: summarize_investor_taxes(&rows),
        referrer_rebates,
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
            let capped_cash_amount = normalized_positive_amount(effective_deposit - order.deposit);
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

fn summarize_statement_investor_ledger(
    ledger: &[FundStatementOrder],
) -> Vec<FundStatementInvestorLedger> {
    let mut investors = HashMap::<String, FundStatementInvestorLedger>::new();
    for flow in ledger {
        let investor =
            investors
                .entry(flow.investor_name.clone())
                .or_insert(FundStatementInvestorLedger {
                    investor_name: flow.investor_name.clone(),
                    deposit: 0.0,
                    effective_deposit: 0.0,
                    inflow_amount: 0.0,
                    outflow_amount: 0.0,
                    capped_cash_amount: 0.0,
                    units: 0.0,
                    flow_count: 0,
                    last_flow_at: flow.updated_at.clone(),
                });

        investor.deposit += flow.deposit;
        investor.effective_deposit += flow.effective_deposit;
        if flow.deposit >= 0.0 {
            investor.inflow_amount += flow.deposit;
        } else {
            investor.outflow_amount += -flow.deposit;
        }
        investor.capped_cash_amount += flow.capped_cash_amount;
        investor.units = flow.investor_units_after;
        investor.flow_count += 1;
        investor.last_flow_at = flow.updated_at.clone();
    }

    let mut rows = investors.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| left.investor_name.cmp(&right.investor_name));
    rows
}

fn capped_unit_delta(requested_unit_delta: f64, current_investor_units: f64) -> f64 {
    if requested_unit_delta < -current_investor_units {
        -current_investor_units
    } else {
        requested_unit_delta
    }
}

fn normalized_positive_amount(value: f64) -> f64 {
    if value > 1e-9 { value } else { 0.0 }
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

async fn get_fund_statement_event(
    db: &SqlitePool,
    fund_id: &str,
    event_index: i64,
) -> Result<FundStatementEvent, AppError> {
    Ok(sqlx::query_as::<_, FundStatementEvent>(
        r#"
        SELECT event_index, event_type, updated_at, payload
        FROM fund_statement_events
        WHERE fund_id = ?1 AND event_index = ?2
        "#,
    )
    .bind(fund_id)
    .bind(event_index)
    .fetch_one(db)
    .await?)
}

fn validate_fund_statement_event_request(
    request: &UpdateFundStatementEventRequest,
) -> Result<(), AppError> {
    if request.event_type.trim().is_empty() {
        return Err(AppError::bad_request("event_type is required"));
    }
    if request.updated_at.trim().is_empty() {
        return Err(AppError::bad_request("updated_at is required"));
    }
    serde_json::from_value::<FundStatementEventPayload>(request.payload.clone())?;
    Ok(())
}

async fn rebuild_fund_statement_derived_tables(
    tx: &mut Transaction<'_, Sqlite>,
    fund_id: &str,
) -> Result<(), AppError> {
    for table in [
        "fund_statement_orders",
        "fund_statement_equity",
        "fund_statement_investors",
        "fund_statement_tax_modes",
    ] {
        let sql = format!("DELETE FROM {table} WHERE fund_id = ?1");
        sqlx::query(&sql).bind(fund_id).execute(&mut **tx).await?;
    }

    let rows = sqlx::query_as::<_, FundStatementEventProjectionRow>(
        r#"
        SELECT event_index, event_type, updated_at, payload
        FROM fund_statement_events
        WHERE fund_id = ?1
        ORDER BY event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut investors = HashMap::<String, FundStatementInvestorProjection>::new();

    for row in rows {
        project_fund_statement_event(tx, fund_id, row, &mut investors).await?;
    }

    for (name, investor) in investors {
        sqlx::query(
            r#"
            INSERT INTO fund_statement_investors (
                fund_id, name, referrer, tax_rate, referrer_rebate_rate,
                tax_threshold, updated_at, source_event_index
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(fund_id)
        .bind(name)
        .bind(investor.referrer)
        .bind(investor.tax_rate)
        .bind(investor.referrer_rebate_rate)
        .bind(investor.tax_threshold)
        .bind(investor.updated_at)
        .bind(investor.source_event_index)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn project_fund_statement_event(
    tx: &mut Transaction<'_, Sqlite>,
    fund_id: &str,
    row: FundStatementEventProjectionRow,
    investors: &mut HashMap<String, FundStatementInvestorProjection>,
) -> Result<(), AppError> {
    let payload = serde_json::from_str::<FundStatementEventPayload>(&row.payload)?;

    if let Some(fund_equity) = payload.fund_equity {
        sqlx::query(
            r#"
            INSERT INTO fund_statement_equity (fund_id, event_index, equity, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(fund_id)
        .bind(row.event_index)
        .bind(fund_equity.equity)
        .bind(&row.updated_at)
        .execute(&mut **tx)
        .await?;
    }

    if let Some(order) = payload.order {
        sqlx::query(
            r#"
            INSERT INTO fund_statement_orders (fund_id, event_index, investor_name, deposit, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(fund_id)
        .bind(row.event_index)
        .bind(order.name)
        .bind(order.deposit)
        .bind(&row.updated_at)
        .execute(&mut **tx)
        .await?;
    }

    if let Some(investor_update) = payload.investor {
        let investor = investors.entry(investor_update.name).or_default();
        investor.referrer = investor_update
            .referrer
            .or_else(|| investor.referrer.take());
        investor.tax_rate = investor_update.tax_rate.or(investor.tax_rate);
        investor.referrer_rebate_rate = investor_update
            .referrer_rebate_rate
            .or(investor.referrer_rebate_rate);
        investor.tax_threshold = investor_update.add_tax_threshold.or(investor.tax_threshold);
        investor.updated_at = row.updated_at.clone();
        investor.source_event_index = row.event_index;
    }

    if statement_event_is_tax_mode(&row.event_type, payload.event_type.as_deref()) {
        sqlx::query(
            r#"
            INSERT INTO fund_statement_tax_modes (fund_id, event_index, mode, comment, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(fund_id)
        .bind(row.event_index)
        .bind(payload.event_type.as_deref().unwrap_or(&row.event_type))
        .bind(payload.comment)
        .bind(&row.updated_at)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

fn statement_event_is_tax_mode(column_type: &str, payload_type: Option<&str>) -> bool {
    matches!(
        payload_type.or(Some(column_type)),
        Some("taxation") | Some("taxation/v2")
    )
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

async fn load_statement_event_state(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<FundStatementEventState, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, updated_at, payload
        FROM fund_statement_events
        WHERE fund_id = ?1
        ORDER BY event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?;

    let mut state = YuanFundState::default();
    recompute_yuan_derived(&mut state);
    for row in rows {
        let event = serde_json::from_str::<FundStatementEventPayload>(&row.payload)?;
        apply_yuan_fund_event(&mut state, event);
        recompute_yuan_derived(&mut state);
    }

    Ok(yuan_event_state_summary(state))
}

fn apply_yuan_fund_event(state: &mut YuanFundState, event: FundStatementEventPayload) {
    if let Some(fund_equity) = event.fund_equity {
        state.total_assets = fund_equity.equity;
    }

    if let Some(order) = event.order {
        let unit_price = yuan_unit_price(state);
        let share = order.deposit / unit_price;
        let investor = ensure_yuan_investor(state, &order.name);
        if share > 0.0 {
            investor.avg_cost_price = (investor.avg_cost_price * investor.share + order.deposit)
                / (investor.share + share);
        }
        investor.deposit += order.deposit;
        investor.tax_threshold += order.deposit;
        investor.share += share;
        state.total_assets += order.deposit;
    }

    if let Some(investor_update) = event.investor {
        let investor = ensure_yuan_investor(state, &investor_update.name);
        if let Some(tax_rate) = investor_update.tax_rate {
            investor.tax_rate = tax_rate;
        }
        if let Some(add_tax_threshold) = investor_update.add_tax_threshold {
            investor.tax_threshold += add_tax_threshold;
        }
        if let Some(referrer) = investor_update.referrer {
            investor.referrer = Some(referrer);
        }
        if let Some(referrer_rebate_rate) = investor_update.referrer_rebate_rate {
            investor.referrer_rebate_rate = referrer_rebate_rate;
        }
    }

    if event.event_type.as_deref() == Some("taxation") {
        apply_yuan_taxation(state);
    }

    if event.event_type.as_deref() == Some("taxation/v2") {
        apply_yuan_taxation_v2(state);
    }
}

fn apply_yuan_taxation(state: &mut YuanFundState) {
    let derived = state.derived.clone();
    for investor in state.investors.values_mut() {
        let item = derived.get(&investor.name).cloned().unwrap_or_default();
        investor.share = item.after_tax_share;
        investor.tax_threshold = item.after_tax_assets;
        state.total_assets -= item.tax;
        state.total_taxed += item.tax;
    }
}

fn apply_yuan_taxation_v2(state: &mut YuanFundState) {
    let derived = state.derived.clone();
    let unit_price = yuan_unit_price(state);
    let referrers = state.investors.keys().cloned().collect::<Vec<_>>();
    let mut rebates = Vec::<(String, f64, f64)>::new();
    let mut total_tax_share = 0.0;

    for investor in state.investors.values_mut() {
        let item = derived.get(&investor.name).cloned().unwrap_or_default();
        let tax_share = investor.share - item.after_tax_share;
        investor.share -= tax_share;
        investor.tax_threshold = item.after_tax_assets;
        investor.taxed += item.tax;
        state.total_taxed += item.tax;

        let mut tax_account_share = tax_share;
        if investor.referrer_rebate_rate > 0.0 {
            if let Some(referrer) = investor
                .referrer
                .as_ref()
                .filter(|name| referrers.iter().any(|item| item == *name))
            {
                let rebate_share = tax_share * investor.referrer_rebate_rate;
                tax_account_share -= rebate_share;
                rebates.push((referrer.clone(), rebate_share, rebate_share * unit_price));
            }
        }
        total_tax_share += tax_account_share;
    }

    for (referrer, rebate_share, rebate_value) in rebates {
        let investor = ensure_yuan_investor(state, &referrer);
        investor.share += rebate_share;
        investor.tax_threshold += rebate_value;
        investor.claimed_referrer_rebate += rebate_value;
    }

    let tax_account = ensure_yuan_investor(state, "@tax");
    tax_account.share += total_tax_share;
    tax_account.tax_threshold += total_tax_share * unit_price;
}

fn recompute_yuan_derived(state: &mut YuanFundState) {
    let total_share = yuan_total_share(state);
    let unit_price = yuan_unit_price(state);
    state.derived = state
        .investors
        .values()
        .map(|investor| {
            let pre_tax_assets = investor.share * unit_price;
            let taxable = pre_tax_assets - investor.tax_threshold;
            let tax = taxable.max(0.0) * investor.tax_rate;
            let after_tax_assets = pre_tax_assets - tax;
            let after_tax_share = after_tax_assets / unit_price;
            (
                investor.name.clone(),
                YuanInvestorDerived {
                    share_ratio: if total_share == 0.0 {
                        0.0
                    } else {
                        investor.share / total_share
                    },
                    tax,
                    taxable,
                    pre_tax_assets,
                    after_tax_assets,
                    after_tax_share,
                },
            )
        })
        .collect();
}

fn yuan_event_state_summary(state: YuanFundState) -> FundStatementEventState {
    let total_deposit = state.investors.values().map(|item| item.deposit).sum();
    let total_share = yuan_total_share(&state);
    let unit_price = yuan_unit_price(&state);
    let total_tax = state.derived.values().map(|item| item.tax).sum();
    let total_profit = state.total_assets - total_deposit + state.total_taxed;
    let mut investors = state
        .investors
        .values()
        .map(|investor| {
            let derived = state
                .derived
                .get(&investor.name)
                .cloned()
                .unwrap_or_default();
            FundStatementEventInvestor {
                name: investor.name.clone(),
                referrer: investor.referrer.clone(),
                deposit: investor.deposit,
                share: investor.share,
                share_ratio: derived.share_ratio,
                tax_threshold: investor.tax_threshold,
                tax_rate: investor.tax_rate,
                tax: derived.tax,
                taxable: derived.taxable,
                pre_tax_assets: derived.pre_tax_assets,
                after_tax_assets: derived.after_tax_assets,
                after_tax_share: derived.after_tax_share,
                referrer_rebate_rate: investor.referrer_rebate_rate,
                claimed_referrer_rebate: investor.claimed_referrer_rebate,
                taxed: investor.taxed,
            }
        })
        .collect::<Vec<_>>();
    investors.sort_by(|left, right| {
        right
            .after_tax_assets
            .total_cmp(&left.after_tax_assets)
            .then_with(|| left.name.cmp(&right.name))
    });

    FundStatementEventState {
        total_assets: state.total_assets,
        total_deposit,
        total_share,
        unit_price,
        total_tax,
        total_taxed: state.total_taxed,
        total_profit,
        investors,
    }
}

fn ensure_yuan_investor<'a>(state: &'a mut YuanFundState, name: &str) -> &'a mut YuanInvestor {
    state
        .investors
        .entry(name.to_string())
        .or_insert_with(|| YuanInvestor {
            name: name.to_string(),
            ..YuanInvestor::default()
        })
}

fn yuan_total_share(state: &YuanFundState) -> f64 {
    state.investors.values().map(|item| item.share).sum()
}

fn yuan_unit_price(state: &YuanFundState) -> f64 {
    let total_share = yuan_total_share(state);
    if total_share == 0.0 {
        1.0
    } else {
        state.total_assets / total_share
    }
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

fn settlement_basis_label(source: &str) -> &str {
    if source == "live_nav" {
        return "Live NAV";
    }
    if source == "legacy_statement" {
        return "Statement history";
    }
    source
}

fn settlement_run_csv(detail: &FundSettlementRunDetail) -> String {
    let mut rows = vec![
        vec![
            "run_id".to_string(),
            detail.run.id.clone(),
            "fund_id".to_string(),
            detail.run.fund_id.clone(),
            "settlement_model".to_string(),
            detail.run.settlement_model.clone(),
            "status".to_string(),
            detail.run.status.clone(),
            "status_updated_at".to_string(),
            detail.run.status_updated_at.clone().unwrap_or_default(),
            "basis_source".to_string(),
            detail.run.basis_source.clone(),
            "basis_id".to_string(),
            detail.run.basis_id.clone(),
            "basis_updated_at".to_string(),
            detail.run.basis_updated_at.clone(),
        ],
        vec![
            "equity".to_string(),
            detail.run.equity.to_string(),
            "equity_updated_at".to_string(),
            detail.run.equity_updated_at.clone(),
            "created_at".to_string(),
            detail.run.created_at.clone(),
            "total_deposit".to_string(),
            detail.run.total_deposit.to_string(),
        ],
        vec![
            "total_units".to_string(),
            detail.run.total_units.to_string(),
            "total_tax".to_string(),
            detail.run.total_tax.to_string(),
            "total_referrer_rebate".to_string(),
            detail.run.total_referrer_rebate.to_string(),
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
        vec!["settlement_report".to_string(), "amount".to_string()],
        vec![
            "post_settlement_equity".to_string(),
            detail.totals.net_equity.to_string(),
        ],
        vec![
            "investor_tax_payable".to_string(),
            detail.totals.tax.to_string(),
        ],
        vec![
            "referrer_rebates_payable".to_string(),
            detail.totals.referrer_rebate.to_string(),
        ],
        vec![
            "retained_tax".to_string(),
            detail.totals.retained_tax.to_string(),
        ],
        vec![
            "gross_equity_control".to_string(),
            detail.totals.gross_equity.to_string(),
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
            "referrer_rebate_received".to_string(),
            "tax_account_credit".to_string(),
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
            investor.referrer_rebate_received.to_string(),
            investor.tax_account_credit.to_string(),
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
    let investor_names = investors
        .iter()
        .map(|investor| investor.name.as_str())
        .collect::<Vec<_>>();
    let mut rebates = HashMap::<String, f64>::new();

    for investor in investors {
        if let Some(referrer) = &investor.referrer {
            if investor.referrer_rebate > 0.0 && investor_names.iter().any(|name| *name == referrer)
            {
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
            None,
            settlement_event_state(vec![
                event_investor("Alice", None, 100.0, 100.0, 0.0, 0.0),
                event_investor_with_rebate("Bob", Some("Alice"), 120.0, 120.0, 110.0, 0.2, 0.25),
            ]),
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
        let tax = preview
            .investors
            .iter()
            .find(|item| item.name == "@tax")
            .unwrap();

        assert_close(preview.total_units, 220.0);
        assert_eq!(
            preview.basis.as_ref().map(|item| item.source.as_str()),
            Some("legacy_statement")
        );
        assert_close(alice.gross_equity, 240.0 * 100.0 / 220.0);
        assert_close(bob.gross_equity, 240.0 * 120.0 / 220.0);
        assert_close(bob.tax, bob.profit * 0.2);
        assert_close(bob.referrer_rebate, bob.tax * 0.25);
        assert_close(alice.referrer_rebate_received, bob.referrer_rebate);
        assert_close(tax.tax_account_credit, bob.tax - bob.referrer_rebate);
        assert_close(bob.net_equity, bob.gross_equity - bob.tax);
        assert_close(preview.totals.net_equity, preview.totals.gross_equity);
    }

    #[test]
    fn previews_settlement_tax_against_tax_threshold_not_cash_deposit() {
        let preview = build_settlement_preview(
            "fund".to_string(),
            vec![FundStatementEquity {
                event_index: 1,
                equity: 150.0,
                updated_at: "2025-01-01T12:00:00+00:00".to_string(),
            }],
            None,
            settlement_event_state(vec![
                event_investor("Active", None, 100.0, 100.0, 90.0, 0.2),
                event_investor("Exited", None, -500.0, 0.0, 0.0, 0.2),
            ]),
        );

        let active = preview
            .investors
            .iter()
            .find(|item| item.name == "Active")
            .unwrap();
        let exited = preview
            .investors
            .iter()
            .find(|item| item.name == "Exited")
            .unwrap();

        assert_close(active.profit, 60.0);
        assert_close(active.tax, 12.0);
        assert_close(exited.gross_equity, 0.0);
        assert_close(exited.profit, 0.0);
        assert_close(exited.tax, 0.0);
        assert_close(preview.total_tax, 12.0);
    }

    #[test]
    fn previews_settlement_post_equity_as_taxation_v2_event_state() {
        let mut state = YuanFundState::default();
        for event in [
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: None,
                order: Some(FundStatementOrderPayload {
                    name: "Alice".to_string(),
                    deposit: 100.0,
                }),
                investor: None,
            },
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: None,
                order: Some(FundStatementOrderPayload {
                    name: "Bob".to_string(),
                    deposit: 100.0,
                }),
                investor: None,
            },
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: Some(FundStatementEquityPayload { equity: 300.0 }),
                order: None,
                investor: None,
            },
            FundStatementEventPayload {
                event_type: Some("update".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: Some(FundStatementInvestorPayload {
                    name: "Alice".to_string(),
                    tax_rate: Some(0.2),
                    add_tax_threshold: None,
                    referrer: Some("Bob".to_string()),
                    referrer_rebate_rate: Some(0.5),
                }),
            },
        ] {
            apply_yuan_fund_event(&mut state, event);
            recompute_yuan_derived(&mut state);
        }

        let preview = build_settlement_preview(
            "fund".to_string(),
            vec![FundStatementEquity {
                event_index: 1,
                equity: 300.0,
                updated_at: "2025-01-01T00:00:00+00:00".to_string(),
            }],
            None,
            yuan_event_state_summary(state.clone()),
        );

        apply_yuan_fund_event(
            &mut state,
            FundStatementEventPayload {
                event_type: Some("taxation/v2".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: None,
            },
        );
        recompute_yuan_derived(&mut state);
        let after_tax = yuan_event_state_summary(state);

        for preview_investor in preview.investors {
            let after_tax_investor = after_tax
                .investors
                .iter()
                .find(|item| item.name == preview_investor.name)
                .unwrap();
            assert_close(
                preview_investor.net_equity,
                after_tax_investor.after_tax_assets,
            );
        }
    }

    #[test]
    fn settlement_taxation_v2_resets_taxable_profit_at_basis_equity() {
        let mut state = YuanFundState::default();
        for event in [
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: None,
                order: Some(FundStatementOrderPayload {
                    name: "Alice".to_string(),
                    deposit: 100.0,
                }),
                investor: None,
            },
            FundStatementEventPayload {
                event_type: Some("update".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: Some(FundStatementInvestorPayload {
                    name: "Alice".to_string(),
                    tax_rate: Some(0.2),
                    add_tax_threshold: None,
                    referrer: None,
                    referrer_rebate_rate: None,
                }),
            },
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: Some(FundStatementEquityPayload { equity: 150.0 }),
                order: None,
                investor: None,
            },
            FundStatementEventPayload {
                event_type: Some("taxation/v2".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: None,
            },
        ] {
            apply_yuan_fund_event(&mut state, event);
            recompute_yuan_derived(&mut state);
        }

        let summary = yuan_event_state_summary(state);
        let alice = summary
            .investors
            .iter()
            .find(|item| item.name == "Alice")
            .unwrap();
        let tax = summary
            .investors
            .iter()
            .find(|item| item.name == "@tax")
            .unwrap();

        assert_close(summary.total_assets, 150.0);
        assert_close(summary.total_tax, 0.0);
        assert_close(alice.tax, 0.0);
        assert_close(alice.tax_threshold, 140.0);
        assert_close(tax.tax_threshold, 10.0);
    }

    #[test]
    fn previews_settlement_against_latest_live_nav_basis() {
        let preview = build_settlement_preview(
            "fund".to_string(),
            vec![FundStatementEquity {
                event_index: 1,
                equity: 100.0,
                updated_at: "2025-01-01T12:00:00+00:00".to_string(),
            }],
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
            settlement_event_state(vec![event_investor("Alice", None, 100.0, 100.0, 0.0, 0.0)]),
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
    fn summarizes_statement_investor_ledger_from_cash_flows() {
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
                    deposit: -300.0,
                    updated_at: "2025-01-02T00:00:00+00:00".to_string(),
                },
                SettlementOrder {
                    event_index: 3,
                    investor_name: "Bob".to_string(),
                    deposit: 50.0,
                    updated_at: "2025-01-03T00:00:00+00:00".to_string(),
                },
            ],
            &[FundStatementEquity {
                event_index: 1,
                equity: 200.0,
                updated_at: "2025-01-01T12:00:00+00:00".to_string(),
            }],
        );

        let rows = summarize_statement_investor_ledger(&ledger);
        let alice = rows
            .iter()
            .find(|item| item.investor_name == "Alice")
            .unwrap();
        let bob = rows
            .iter()
            .find(|item| item.investor_name == "Bob")
            .unwrap();

        assert_eq!(rows.len(), 2);
        assert_close(alice.deposit, -200.0);
        assert_close(alice.effective_deposit, -100.0);
        assert_close(alice.inflow_amount, 100.0);
        assert_close(alice.outflow_amount, 300.0);
        assert_close(alice.capped_cash_amount, 100.0);
        assert_close(alice.units, 0.0);
        assert_eq!(alice.flow_count, 2);
        assert_eq!(alice.last_flow_at, "2025-01-02T00:00:00+00:00");
        assert_close(bob.deposit, 50.0);
        assert_close(bob.units, 50.0);
    }

    #[test]
    fn folds_yuan_taxation_v2_into_referrer_and_tax_account_shares() {
        let mut state = YuanFundState::default();
        for event in [
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: None,
                order: Some(FundStatementOrderPayload {
                    name: "Alice".to_string(),
                    deposit: 100.0,
                }),
                investor: None,
            },
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: None,
                order: Some(FundStatementOrderPayload {
                    name: "Bob".to_string(),
                    deposit: 100.0,
                }),
                investor: None,
            },
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: Some(FundStatementEquityPayload { equity: 300.0 }),
                order: None,
                investor: None,
            },
            FundStatementEventPayload {
                event_type: Some("update".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: Some(FundStatementInvestorPayload {
                    name: "Alice".to_string(),
                    tax_rate: Some(0.2),
                    add_tax_threshold: None,
                    referrer: Some("Bob".to_string()),
                    referrer_rebate_rate: Some(0.5),
                }),
            },
            FundStatementEventPayload {
                event_type: Some("taxation/v2".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: None,
            },
        ] {
            apply_yuan_fund_event(&mut state, event);
            recompute_yuan_derived(&mut state);
        }

        let summary = yuan_event_state_summary(state);
        let alice = summary
            .investors
            .iter()
            .find(|item| item.name == "Alice")
            .unwrap();
        let bob = summary
            .investors
            .iter()
            .find(|item| item.name == "Bob")
            .unwrap();
        let tax = summary
            .investors
            .iter()
            .find(|item| item.name == "@tax")
            .unwrap();

        assert_close(summary.total_assets, 300.0);
        assert_close(summary.total_taxed, 10.0);
        assert_close(summary.total_share, 200.0);
        assert_close(summary.unit_price, 1.5);
        assert_close(alice.share, 100.0 - 10.0 / 1.5);
        assert_close(alice.tax_threshold, 140.0);
        assert_close(alice.taxed, 10.0);
        assert_close(bob.claimed_referrer_rebate, 5.0);
        assert_close(bob.share, 100.0 + 5.0 / 1.5);
        assert_close(tax.share, 5.0 / 1.5);
        assert_close(tax.tax_threshold, 5.0);
    }

    #[test]
    fn folds_settlement_event_with_basis_equity_before_taxation() {
        let mut state = YuanFundState::default();
        for event in [
            FundStatementEventPayload {
                event_type: None,
                comment: None,
                fund_equity: None,
                order: Some(FundStatementOrderPayload {
                    name: "Alice".to_string(),
                    deposit: 100.0,
                }),
                investor: None,
            },
            FundStatementEventPayload {
                event_type: Some("update".to_string()),
                comment: None,
                fund_equity: None,
                order: None,
                investor: Some(FundStatementInvestorPayload {
                    name: "Alice".to_string(),
                    tax_rate: Some(0.2),
                    add_tax_threshold: None,
                    referrer: None,
                    referrer_rebate_rate: None,
                }),
            },
            FundStatementEventPayload {
                event_type: None,
                comment: Some("Settlement run test".to_string()),
                fund_equity: Some(FundStatementEquityPayload { equity: 150.0 }),
                order: None,
                investor: None,
            },
            FundStatementEventPayload {
                event_type: Some("taxation/v2".to_string()),
                comment: Some("Settlement run test".to_string()),
                fund_equity: None,
                order: None,
                investor: None,
            },
        ] {
            apply_yuan_fund_event(&mut state, event);
            recompute_yuan_derived(&mut state);
        }

        let summary = yuan_event_state_summary(state);
        let alice = summary
            .investors
            .iter()
            .find(|item| item.name == "Alice")
            .unwrap();
        let tax = summary
            .investors
            .iter()
            .find(|item| item.name == "@tax")
            .unwrap();

        assert_close(summary.total_assets, 150.0);
        assert_close(summary.total_taxed, 10.0);
        assert_close(alice.taxed, 10.0);
        assert_close(alice.tax_threshold, 140.0);
        assert_close(tax.tax_threshold, 10.0);
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
            investor_rebate("Carol", None, 0.0),
            investor_rebate("Dan", Some("Eve"), 0.0),
            investor_rebate("Finn", Some("Grace"), 7.0),
            investor_rebate("Heidi", None, 11.0),
        ]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].referrer, "Carol");
        assert_close(rows[0].rebate, 8.0);
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

    #[tokio::test]
    async fn writes_confirmed_settlement_events_and_derived_rows() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_events (
                fund_id TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (fund_id, event_index)
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_equity (
                fund_id TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                equity REAL NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (fund_id, event_index)
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_tax_modes (
                fund_id TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                mode TEXT NOT NULL,
                comment TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (fund_id, event_index)
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
            VALUES ('fund', 0, 'root', '2025-01-01T00:00:00+00:00', '{}')
            "#,
        )
        .execute(&db)
        .await
        .unwrap();

        let run = FundSettlementRun {
            id: "run-1".to_string(),
            fund_id: "fund".to_string(),
            settlement_model: FUND_SETTLEMENT_MODEL_EVENT_STATE.to_string(),
            equity_event_index: 7,
            equity: 150.0,
            equity_updated_at: "2025-01-02T00:00:00+00:00".to_string(),
            basis_source: "live_nav".to_string(),
            basis_id: "nav-1".to_string(),
            basis_updated_at: "2025-01-02T00:00:00+00:00".to_string(),
            total_deposit: 100.0,
            total_units: 100.0,
            total_tax: 10.0,
            total_referrer_rebate: 0.0,
            capped_cash_flows: 0,
            capped_units: 0.0,
            capped_cash_amount: 0.0,
            investor_count: 1,
            status: "confirmed".to_string(),
            status_updated_at: Some("2025-01-03T00:00:00+00:00".to_string()),
            created_at: "2025-01-02T00:00:00+00:00".to_string(),
        };
        let mut tx = db.begin().await.unwrap();
        insert_confirmed_settlement_event(&mut tx, &run, "2025-01-03T00:00:00+00:00")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let events = sqlx::query_as::<_, (i64, String, String)>(
            r#"
            SELECT event_index, event_type, payload
            FROM fund_statement_events
            WHERE fund_id = 'fund'
            ORDER BY event_index
            "#,
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].0, 1);
        assert_eq!(events[1].1, "settlement_equity");
        assert_eq!(events[2].0, 2);
        assert_eq!(events[2].1, "taxation/v2");

        let equity_payload: serde_json::Value = serde_json::from_str(&events[1].2).unwrap();
        assert_eq!(equity_payload["settlement_run_id"], "run-1");
        assert_eq!(equity_payload["basis_source"], "live_nav");
        assert_close(
            equity_payload["fund_equity"]["equity"].as_f64().unwrap(),
            150.0,
        );

        let (equity_event_index, equity): (i64, f64) = sqlx::query_as(
            r#"
            SELECT event_index, equity
            FROM fund_statement_equity
            WHERE fund_id = 'fund'
            "#,
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(equity_event_index, 1);
        assert_close(equity, 150.0);

        let (tax_event_index, mode, comment): (i64, String, String) = sqlx::query_as(
            r#"
            SELECT event_index, mode, comment
            FROM fund_statement_tax_modes
            WHERE fund_id = 'fund'
            "#,
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(tax_event_index, 2);
        assert_eq!(mode, "taxation/v2");
        assert_eq!(comment, "Settlement run run-1");
    }

    #[tokio::test]
    async fn confirms_draft_settlement_run_and_writes_statement_events() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        create_statement_write_test_tables(&db).await;
        sqlx::query(
            r#"
            CREATE TABLE fund_settlement_runs (
                id TEXT PRIMARY KEY NOT NULL,
                fund_id TEXT NOT NULL,
                settlement_model TEXT NOT NULL,
                equity_event_index INTEGER NOT NULL,
                equity REAL NOT NULL,
                equity_updated_at TEXT NOT NULL,
                basis_source TEXT NOT NULL,
                basis_id TEXT NOT NULL,
                basis_updated_at TEXT NOT NULL,
                total_deposit REAL NOT NULL,
                total_units REAL NOT NULL,
                total_tax REAL NOT NULL,
                total_referrer_rebate REAL NOT NULL,
                capped_cash_flows INTEGER NOT NULL,
                capped_units REAL NOT NULL,
                capped_cash_amount REAL NOT NULL,
                investor_count INTEGER NOT NULL,
                status TEXT NOT NULL,
                status_updated_at TEXT,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_settlement_investor_rows (
                run_id TEXT NOT NULL,
                units REAL NOT NULL
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
            VALUES ('fund', 0, 'root', '2025-01-01T00:00:00+00:00', '{}')
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO fund_settlement_runs (
                id, fund_id, settlement_model, equity_event_index, equity, equity_updated_at,
                basis_source, basis_id, basis_updated_at, total_deposit, total_units,
                total_tax, total_referrer_rebate, capped_cash_flows, capped_units,
                capped_cash_amount, investor_count, status, status_updated_at, created_at
            )
            VALUES (
                'run-1', 'fund', 'event_state_v1', 0, 150.0, '2025-01-02T00:00:00+00:00',
                'live_nav', 'nav-1', '2025-01-02T00:00:00+00:00', 100.0, 100.0,
                10.0, 0.0, 0, 0.0, 0.0, 1, 'draft', NULL, '2025-01-02T00:00:00+00:00'
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fund_settlement_investor_rows (run_id, units) VALUES ('run-1', 100.0)",
        )
        .execute(&db)
        .await
        .unwrap();

        confirm_fund_settlement_run_status(&db, "run-1")
            .await
            .unwrap();

        let (status,): (String,) =
            sqlx::query_as("SELECT status FROM fund_settlement_runs WHERE id = 'run-1'")
                .fetch_one(&db)
                .await
                .unwrap();
        let events = sqlx::query_as::<_, (i64, String)>(
            r#"
            SELECT event_index, event_type
            FROM fund_statement_events
            WHERE fund_id = 'fund'
            ORDER BY event_index
            "#,
        )
        .fetch_all(&db)
        .await
        .unwrap();

        assert_eq!(status, "confirmed");
        assert_eq!(
            events,
            vec![
                (0, "root".to_string()),
                (1, "settlement_equity".to_string()),
                (2, "taxation/v2".to_string()),
            ]
        );
        assert!(
            confirm_fund_settlement_run_status(&db, "run-1")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn folds_statement_events_by_event_index_not_timestamp() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_events (
                fund_id TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (fund_id, event_index)
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        for (event_index, updated_at, payload) in [
            (
                0,
                "2025-01-02T00:00:00+00:00",
                r#"{"type":"order","order":{"name":"Alice","deposit":100}}"#,
            ),
            (
                1,
                "2025-01-01T00:00:00+00:00",
                r#"{"type":"equity","fund_equity":{"equity":150}}"#,
            ),
        ] {
            sqlx::query(
                r#"
                INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
                VALUES ('fund', ?1, 'test', ?2, ?3)
                "#,
            )
            .bind(event_index)
            .bind(updated_at)
            .bind(payload)
            .execute(&db)
            .await
            .unwrap();
        }

        let summary = load_statement_event_state(&db, "fund").await.unwrap();
        let alice = summary
            .investors
            .iter()
            .find(|item| item.name == "Alice")
            .unwrap();

        assert_close(summary.total_assets, 150.0);
        assert_close(alice.share, 100.0);
    }

    #[tokio::test]
    async fn rebuilds_statement_derived_rows_after_event_delete() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        create_statement_write_test_tables(&db).await;
        for (event_index, event_type, payload) in [
            (0, "equity", r#"{"fund_equity":{"equity":150}}"#),
            (1, "order", r#"{"order":{"name":"Alice","deposit":100}}"#),
            (
                2,
                "investor",
                r#"{"investor":{"name":"Alice","tax_rate":0.2,"referrer":"Bob"}}"#,
            ),
            (
                3,
                "taxation/v2",
                r#"{"type":"taxation/v2","comment":"tax checkpoint"}"#,
            ),
        ] {
            sqlx::query(
                r#"
                INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
                VALUES ('fund', ?1, ?2, '2025-01-01T00:00:00+00:00', ?3)
                "#,
            )
            .bind(event_index)
            .bind(event_type)
            .bind(payload)
            .execute(&db)
            .await
            .unwrap();
        }

        let mut tx = db.begin().await.unwrap();
        rebuild_fund_statement_derived_tables(&mut tx, "fund")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let (orders_before,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM fund_statement_orders WHERE fund_id = 'fund'")
                .fetch_one(&db)
                .await
                .unwrap();
        let (investor_name, tax_mode): (String, String) = sqlx::query_as(
            r#"
            SELECT investor.name, mode.mode
            FROM fund_statement_investors investor
            CROSS JOIN fund_statement_tax_modes mode
            WHERE investor.fund_id = 'fund' AND mode.fund_id = 'fund'
            "#,
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(orders_before, 1);
        assert_eq!(investor_name, "Alice");
        assert_eq!(tax_mode, "taxation/v2");

        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM fund_statement_events WHERE fund_id = 'fund' AND event_index = 1")
            .execute(&mut *tx)
            .await
            .unwrap();
        rebuild_fund_statement_derived_tables(&mut tx, "fund")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let (orders_after,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM fund_statement_orders WHERE fund_id = 'fund'")
                .fetch_one(&db)
                .await
                .unwrap();
        let (equity_after,): (f64,) =
            sqlx::query_as("SELECT equity FROM fund_statement_equity WHERE fund_id = 'fund'")
                .fetch_one(&db)
                .await
                .unwrap();

        assert_eq!(orders_after, 0);
        assert_close(equity_after, 150.0);
    }

    #[tokio::test]
    async fn rejects_duplicate_statement_settlement_basis() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        create_statement_write_test_tables(&db).await;
        sqlx::query(
            r#"
            INSERT INTO fund_statement_events (fund_id, event_index, event_type, updated_at, payload)
            VALUES (
                'fund', 0, 'taxation/v2', '2025-01-01T00:00:00+00:00',
                '{"type":"taxation/v2","settlement_model":"event_state_v1","basis_source":"live_nav","basis_id":"nav-1"}'
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();

        let basis = FundSettlementBasis {
            source: "live_nav".to_string(),
            id: "nav-1".to_string(),
            equity: 150.0,
            updated_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        let mut tx = db.begin().await.unwrap();

        assert!(
            reject_duplicate_statement_settlement(&mut tx, "fund", &basis)
                .await
                .is_err()
        );
    }

    async fn create_statement_write_test_tables(db: &SqlitePool) {
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_events (
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
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_equity (
                fund_id TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                equity REAL NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (fund_id, event_index)
            )
            "#,
        )
        .execute(db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_orders (
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
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_investors (
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
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_statement_tax_modes (
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
        .await
        .unwrap();
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
            referrer_rebate_received: 0.0,
            tax_account_credit: 0.0,
            capped_cash_amount: 0.0,
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

    fn settlement_event_state(
        investors: Vec<FundStatementEventInvestor>,
    ) -> FundStatementEventState {
        let total_share = investors.iter().map(|item| item.share).sum();
        let total_deposit = investors.iter().map(|item| item.deposit).sum();
        FundStatementEventState {
            total_assets: 0.0,
            total_deposit,
            total_share,
            unit_price: 1.0,
            total_tax: 0.0,
            total_taxed: 0.0,
            total_profit: 0.0,
            investors,
        }
    }

    fn event_investor(
        name: &str,
        referrer: Option<&str>,
        deposit: f64,
        share: f64,
        tax_threshold: f64,
        tax_rate: f64,
    ) -> FundStatementEventInvestor {
        FundStatementEventInvestor {
            name: name.to_string(),
            referrer: referrer.map(str::to_string),
            deposit,
            share,
            share_ratio: 0.0,
            tax_threshold,
            tax_rate,
            tax: 0.0,
            taxable: 0.0,
            pre_tax_assets: 0.0,
            after_tax_assets: 0.0,
            after_tax_share: 0.0,
            referrer_rebate_rate: 0.0,
            claimed_referrer_rebate: 0.0,
            taxed: 0.0,
        }
    }

    fn event_investor_with_rebate(
        name: &str,
        referrer: Option<&str>,
        deposit: f64,
        share: f64,
        tax_threshold: f64,
        tax_rate: f64,
        referrer_rebate_rate: f64,
    ) -> FundStatementEventInvestor {
        FundStatementEventInvestor {
            referrer_rebate_rate,
            ..event_investor(name, referrer, deposit, share, tax_threshold, tax_rate)
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
