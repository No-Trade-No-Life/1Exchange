use std::collections::{BTreeMap, HashMap};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::{AppError, AppState, auth, models::AccountInfo, rates, virtual_accounts};

const DEFAULT_POLL_INTERVAL_SECONDS: i64 = 600;
const FUND_SCAN_INTERVAL_SECONDS: u64 = 60;
const FUND_EVENT_EQUITY_SET: &str = "fund_equity_set";
const FUND_EVENT_CASH_FLOW_RECORDED: &str = "cash_flow_recorded";
const FUND_EVENT_INVESTOR_PROFILE_UPDATED: &str = "investor_profile_updated";
const FUND_EVENT_TAX_THRESHOLD_ADJUSTED: &str = "tax_threshold_adjusted";
const FUND_EVENT_TAXATION_V1_APPLIED: &str = "taxation_v1_applied";
const FUND_EVENT_TAXATION_V2_APPLIED: &str = "taxation_v2_applied";

#[derive(Debug, Deserialize)]
pub struct CreateFundRequest {
    pub id: Option<String>,
    pub name: String,
    pub account_id: String,
    pub enabled: bool,
    pub target_currency: Option<String>,
    pub poll_interval_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FundAccessGrantRequest {
    pub fund_id: String,
    pub grantee_user_id: String,
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
pub struct FundAccessGrant {
    pub fund_id: String,
    pub grantee_user_id: String,
    pub created_at: String,
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
pub struct FundUnitPriceCandleQuery {
    fund_id: String,
}

#[derive(Deserialize)]
pub struct UpdateFundStatementEventRequest {
    fund_id: String,
    event_index: i64,
    event_type: String,
    occurred_at: String,
    investor_id: Option<String>,
    comment: Option<String>,
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
    occurred_at: String,
    investor_id: Option<String>,
    comment: Option<String>,
    payload: String,
}

#[derive(Debug, Serialize)]
pub struct FundStatementEventPage {
    events: Vec<FundStatementEvent>,
    total: i64,
    limit: i64,
    offset: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FundUnitPriceCandle {
    day: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    events: i64,
    last_event_index: i64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct FundReducerSnapshotState {
    unit_price_candles: Option<Vec<FundUnitPriceCandle>>,
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

#[derive(Clone, Debug, FromRow)]
struct FundStatementEventPayloadRow {
    event_index: i64,
    occurred_at: String,
    investor_id: Option<String>,
    comment: Option<String>,
    event_type: String,
    payload: String,
}

#[derive(Debug, Deserialize)]
struct FundStatementEquityPayload {
    equity: f64,
}

#[derive(Debug, Deserialize)]
struct FundCashFlowPayload {
    amount: f64,
}

#[derive(Debug, Deserialize)]
struct FundInvestorProfilePayload {
    tax_rate: Option<f64>,
    referrer_id: Option<String>,
    referrer_rebate_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FundTaxThresholdAdjustmentPayload {
    amount: f64,
}

#[cfg(test)]
struct FundStatementEventPayload {
    event_type: Option<String>,
    comment: Option<String>,
    fund_equity: Option<FundStatementEquityPayload>,
    order: Option<FundStatementOrderPayload>,
    investor: Option<FundStatementInvestorPayload>,
}

#[cfg(test)]
struct FundStatementOrderPayload {
    name: String,
    deposit: f64,
}

#[cfg(test)]
struct FundStatementInvestorPayload {
    name: String,
    tax_rate: Option<f64>,
    add_tax_threshold: Option<f64>,
    referrer: Option<String>,
    referrer_rebate_rate: Option<f64>,
}

#[cfg(test)]
impl From<FundStatementEventPayload> for FundStatementEventPayloadRow {
    fn from(payload: FundStatementEventPayload) -> Self {
        if let Some(fund_equity) = payload.fund_equity {
            return Self {
                event_index: 0,
                occurred_at: String::new(),
                investor_id: None,
                comment: payload.comment,
                event_type: FUND_EVENT_EQUITY_SET.to_string(),
                payload: serde_json::json!({ "equity": fund_equity.equity }).to_string(),
            };
        }
        if let Some(order) = payload.order {
            return Self {
                event_index: 0,
                occurred_at: String::new(),
                investor_id: Some(order.name),
                comment: payload.comment,
                event_type: FUND_EVENT_CASH_FLOW_RECORDED.to_string(),
                payload: serde_json::json!({ "amount": order.deposit }).to_string(),
            };
        }
        if let Some(investor) = payload.investor {
            if let Some(amount) = investor.add_tax_threshold {
                return Self {
                    event_index: 0,
                    occurred_at: String::new(),
                    investor_id: Some(investor.name),
                    comment: payload.comment,
                    event_type: FUND_EVENT_TAX_THRESHOLD_ADJUSTED.to_string(),
                    payload: serde_json::json!({ "amount": amount }).to_string(),
                };
            }
            return Self {
                event_index: 0,
                occurred_at: String::new(),
                investor_id: Some(investor.name),
                comment: payload.comment,
                event_type: FUND_EVENT_INVESTOR_PROFILE_UPDATED.to_string(),
                payload: serde_json::json!({
                    "tax_rate": investor.tax_rate,
                    "referrer_id": investor.referrer,
                    "referrer_rebate_rate": investor.referrer_rebate_rate,
                })
                .to_string(),
            };
        }

        Self {
            event_index: 0,
            occurred_at: String::new(),
            investor_id: None,
            comment: payload.comment,
            event_type: match payload.event_type.as_deref() {
                Some("taxation") => FUND_EVENT_TAXATION_V1_APPLIED,
                Some("taxation/v2") => FUND_EVENT_TAXATION_V2_APPLIED,
                _ => FUND_EVENT_TAXATION_V1_APPLIED,
            }
            .to_string(),
            payload: "{}".to_string(),
        }
    }
}

#[derive(Default)]
struct FundStatementInvestorProjection {
    referrer: Option<String>,
    tax_rate: Option<f64>,
    referrer_rebate_rate: Option<f64>,
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
        Json(get_owned_fund_config(&state.db, &user.user_id, &id).await?),
    ))
}

pub async fn list_fund_access_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundStatementQuery>,
) -> Result<Json<Vec<FundAccessGrant>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_owned_fund_config(&state.db, &user.user_id, &query.fund_id).await?;

    Ok(Json(
        sqlx::query_as::<_, FundAccessGrant>(
            r#"
            SELECT fund_id, grantee_user_id, created_at
            FROM fund_access_grants
            WHERE fund_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(query.fund_id)
        .fetch_all(&state.db)
        .await?,
    ))
}

pub async fn create_fund_access_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FundAccessGrantRequest>,
) -> Result<(StatusCode, Json<FundAccessGrant>), AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_owned_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
    validate_fund_access_grant_request(&request)?;
    let grantee_user_id = request.grantee_user_id.trim();
    if grantee_user_id == user.user_id {
        return Err(AppError::bad_request(
            "fund access user id must be another user",
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO fund_access_grants (fund_id, grantee_user_id)
        VALUES (?1, ?2)
        ON CONFLICT(fund_id, grantee_user_id) DO UPDATE SET
            created_at = fund_access_grants.created_at
        "#,
    )
    .bind(request.fund_id.trim())
    .bind(grantee_user_id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(get_fund_access_grant(&state.db, request.fund_id.trim(), grantee_user_id).await?),
    ))
}

pub async fn delete_fund_access_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundAccessGrantRequest>,
) -> Result<StatusCode, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_owned_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
    validate_fund_access_grant_request(&query)?;

    sqlx::query(
        r#"
        DELETE FROM fund_access_grants
        WHERE fund_id = ?1 AND grantee_user_id = ?2
        "#,
    )
    .bind(query.fund_id.trim())
    .bind(query.grantee_user_id.trim())
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_fund_nav(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundNavQuery>,
) -> Result<Json<Vec<FundNavSnapshot>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let rows = if let Some(fund_id) = query.fund_id {
        get_readable_fund_config(&state.db, &user.user_id, &fund_id).await?;
        sqlx::query_as::<_, FundNavSnapshot>(
            r#"
            SELECT CAST(e.event_index AS TEXT) AS id,
                   e.fund_id,
                   f.account_id,
                   json_extract(e.payload, '$.equity') AS equity,
                   f.target_currency,
                   0 AS positions_count,
                   0 AS unpriced_positions,
                   e.occurred_at AS created_at
            FROM fund_events e
            JOIN funds f ON f.id = e.fund_id
            WHERE e.fund_id = ?1 AND e.event_type = 'fund_equity_set'
            ORDER BY e.event_index DESC
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
            SELECT CAST(e.event_index AS TEXT) AS id,
                   e.fund_id,
                   f.account_id,
                   json_extract(e.payload, '$.equity') AS equity,
                   f.target_currency,
                   0 AS positions_count,
                   0 AS unpriced_positions,
                   e.occurred_at AS created_at
            FROM fund_events e
            JOIN funds f ON f.id = e.fund_id
            LEFT JOIN fund_access_grants g ON g.fund_id = f.id AND g.grantee_user_id = ?1
            WHERE (f.owner_id = ?1 OR g.grantee_user_id = ?1)
              AND e.event_type = 'fund_equity_set'
            ORDER BY e.event_index DESC
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
    get_readable_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM fund_events WHERE fund_id = ?1")
        .bind(&query.fund_id)
        .fetch_one(&state.db)
        .await?;
    let events = sqlx::query_as::<_, FundStatementEvent>(
        r#"
            SELECT event_index, event_type, occurred_at, investor_id, comment, payload
            FROM fund_events
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
    get_owned_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
    validate_fund_statement_event_request(&request)?;

    let payload = serde_json::to_string(&request.payload)?;
    let mut tx = state.db.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE fund_events
        SET event_type = ?1, occurred_at = ?2, investor_id = ?3, comment = ?4,
            payload = ?5, updated_at = CURRENT_TIMESTAMP
        WHERE fund_id = ?6 AND event_index = ?7
        "#,
    )
    .bind(request.event_type.trim())
    .bind(request.occurred_at.trim())
    .bind(trimmed_optional_string(request.investor_id.as_deref()))
    .bind(trimmed_optional_string(request.comment.as_deref()))
    .bind(&payload)
    .bind(&request.fund_id)
    .bind(request.event_index)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request("fund statement event not found"));
    }

    invalidate_fund_reducer_snapshot(&mut tx, &request.fund_id).await?;
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
    get_owned_fund_config(&state.db, &user.user_id, &query.fund_id).await?;

    let mut tx = state.db.begin().await?;
    let result = sqlx::query(
        r#"
        DELETE FROM fund_events
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

    invalidate_fund_reducer_snapshot(&mut tx, &query.fund_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_fund_statement_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundStatementQuery>,
) -> Result<Json<FundStatementSummary>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_readable_fund_config(&state.db, &user.user_id, &query.fund_id).await?;

    let (events,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM fund_events WHERE fund_id = ?1")
        .bind(&query.fund_id)
        .fetch_one(&state.db)
        .await?;
    let statement_orders = load_cash_flow_events(&state.db, &query.fund_id).await?;
    let statement_equity = load_equity_events(&state.db, &query.fund_id).await?;
    let latest_equity = statement_equity.last().cloned();
    let ledger = build_cash_flow_ledger(&statement_orders, &statement_equity);
    let investors = load_investor_profile_events(&state.db, &query.fund_id).await?;
    let investor_ledger = summarize_statement_investor_ledger(&ledger);
    let orders = statement_orders.len() as i64;
    let order_deposit = statement_orders
        .iter()
        .map(|item| item.deposit)
        .sum::<f64>();
    let inflow_count = statement_orders
        .iter()
        .filter(|item| item.deposit > 0.0)
        .count() as i64;
    let inflow_amount = statement_orders
        .iter()
        .filter(|item| item.deposit > 0.0)
        .map(|item| item.deposit)
        .sum::<f64>();
    let outflow_count = statement_orders
        .iter()
        .filter(|item| item.deposit < 0.0)
        .count() as i64;
    let outflow_amount = statement_orders
        .iter()
        .filter(|item| item.deposit < 0.0)
        .map(|item| -item.deposit)
        .sum::<f64>();
    let equity_points = statement_equity.len() as i64;
    let investor_count = investors.len() as i64;
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
    let reconciliation = None;
    let tax_modes = load_tax_events(&state.db, &query.fund_id).await?;
    let tax_mode_count = tax_modes.len() as i64;
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
            order_deposit,
            inflow_count,
            inflow_amount,
            outflow_count,
            outflow_amount,
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

pub async fn get_fund_unit_price_candles(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundUnitPriceCandleQuery>,
) -> Result<Json<Vec<FundUnitPriceCandle>>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_readable_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
    Ok(Json(
        load_fund_unit_price_candles(&state.db, &query.fund_id).await?,
    ))
}

pub async fn get_fund_settlement_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementQuery>,
) -> Result<Json<FundSettlementPreview>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_readable_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
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
    get_readable_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
    Ok(Json(Vec::new()))
}

pub async fn get_fund_settlement_run_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementRunQuery>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    let _user = auth::require_initialized_user(&state, &headers).await?;
    let _run_id = query.run_id;
    Err(AppError::bad_request(
        "settlement run history is not supported",
    ))
}

pub async fn export_fund_settlement_run_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<FundSettlementRunQuery>,
) -> Result<Response, AppError> {
    let _user = auth::require_initialized_user(&state, &headers).await?;
    let _run_id = query.run_id;
    Err(AppError::bad_request(
        "settlement run history is not supported",
    ))
}

pub async fn create_fund_settlement_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateFundSettlementRunRequest>,
) -> Result<(StatusCode, Json<FundSettlementRunDetail>), AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_owned_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
    Err(AppError::bad_request(
        "settlement drafts are not supported; use settlement preview or confirm settlement",
    ))
}

pub async fn confirm_fund_settlement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmFundSettlementRequest>,
) -> Result<Json<FundSettlementPreview>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    get_owned_fund_config(&state.db, &user.user_id, &request.fund_id).await?;
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
    append_fund_event(
        &state.db,
        &preview.fund_id,
        FUND_EVENT_EQUITY_SET,
        &updated_at,
        None,
        Some(&comment),
        serde_json::json!({ "equity": basis.equity }),
    )
    .await?;
    append_fund_event(
        &state.db,
        &preview.fund_id,
        FUND_EVENT_TAXATION_V2_APPLIED,
        &updated_at,
        None,
        Some(&comment),
        serde_json::json!({}),
    )
    .await?;

    Ok(Json(
        load_fund_settlement_preview(&state.db, preview.fund_id).await?,
    ))
}

pub async fn confirm_fund_settlement_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateFundSettlementRunRequest>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    let _user = auth::require_initialized_user(&state, &headers).await?;
    let _run_id = request.run_id;
    Err(AppError::bad_request("settlement drafts are not supported"))
}

pub async fn void_fund_settlement_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateFundSettlementRunRequest>,
) -> Result<Json<FundSettlementRunDetail>, AppError> {
    let _user = auth::require_initialized_user(&state, &headers).await?;
    let _run_id = request.run_id;
    Err(AppError::bad_request("settlement drafts are not supported"))
}

async fn load_fund_settlement_preview(
    db: &SqlitePool,
    fund_id: String,
) -> Result<FundSettlementPreview, AppError> {
    let equity_points = load_equity_events(db, &fund_id).await?;
    let event_state = load_statement_event_state(db, &fund_id).await?;

    Ok(build_settlement_preview(
        fund_id,
        equity_points,
        None,
        event_state,
    ))
}

pub async fn sample_fund_now(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<SampleFundQuery>,
) -> Result<Json<FundNavSnapshot>, AppError> {
    let user = auth::require_initialized_user(&state, &headers).await?;
    let config = get_owned_fund_config(&state.db, &user.user_id, &query.fund_id).await?;
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
    user_id: &str,
) -> Result<Vec<FundConfig>, AppError> {
    Ok(sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.owner_id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.occurred_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_events s ON s.fund_id = f.id AND s.event_type = 'fund_equity_set'
        LEFT JOIN fund_access_grants g ON g.fund_id = f.id AND g.grantee_user_id = ?1
        WHERE f.owner_id = ?1 OR g.grantee_user_id = ?1
        GROUP BY f.id
        ORDER BY f.created_at DESC
        "#,
    )
    .bind(user_id)
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
               f.created_at, f.updated_at, MAX(s.occurred_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_events s ON s.fund_id = f.id AND s.event_type = 'fund_equity_set'
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

async fn get_owned_fund_config(
    db: &SqlitePool,
    owner_id: &str,
    fund_id: &str,
) -> Result<FundConfig, AppError> {
    sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.owner_id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.occurred_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_events s ON s.fund_id = f.id AND s.event_type = 'fund_equity_set'
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

async fn get_readable_fund_config(
    db: &SqlitePool,
    user_id: &str,
    fund_id: &str,
) -> Result<FundConfig, AppError> {
    sqlx::query_as::<_, FundConfig>(
        r#"
        SELECT f.id, f.owner_id, f.name, f.account_id, f.enabled, f.target_currency, f.poll_interval_seconds,
               f.created_at, f.updated_at, MAX(s.occurred_at) AS last_sampled_at
        FROM funds f
        LEFT JOIN fund_events s ON s.fund_id = f.id AND s.event_type = 'fund_equity_set'
        LEFT JOIN fund_access_grants g ON g.fund_id = f.id AND g.grantee_user_id = ?2
        WHERE f.id = ?1 AND (f.owner_id = ?2 OR g.grantee_user_id = ?2)
        GROUP BY f.id
        "#,
    )
    .bind(fund_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

async fn get_fund_access_grant(
    db: &SqlitePool,
    fund_id: &str,
    grantee_user_id: &str,
) -> Result<FundAccessGrant, AppError> {
    sqlx::query_as::<_, FundAccessGrant>(
        r#"
        SELECT fund_id, grantee_user_id, created_at
        FROM fund_access_grants
        WHERE fund_id = ?1 AND grantee_user_id = ?2
        "#,
    )
    .bind(fund_id)
    .bind(grantee_user_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

async fn append_fund_event(
    db: &SqlitePool,
    fund_id: &str,
    event_type: &str,
    occurred_at: &str,
    investor_id: Option<&str>,
    comment: Option<&str>,
    payload: serde_json::Value,
) -> Result<i64, AppError> {
    validate_fund_event_payload(event_type, investor_id, &payload)?;
    let payload = serde_json::to_string(&payload)?;
    let mut tx = db.begin().await?;
    let (event_index,): (i64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(MAX(event_index), -1) + 1
        FROM fund_events
        WHERE fund_id = ?1
        "#,
    )
    .bind(fund_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO fund_events (
            fund_id, event_index, event_type, occurred_at, investor_id, comment, payload
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(fund_id)
    .bind(event_index)
    .bind(event_type)
    .bind(occurred_at)
    .bind(trimmed_optional_string(investor_id))
    .bind(trimmed_optional_string(comment))
    .bind(payload)
    .execute(&mut *tx)
    .await?;
    invalidate_fund_reducer_snapshot(&mut tx, fund_id).await?;
    tx.commit().await?;

    Ok(event_index)
}

async fn sample_fund(db: &SqlitePool, config: &FundConfig) -> Result<FundNavSnapshot, AppError> {
    let account =
        virtual_accounts::compose_virtual_account_by_id(db, &config.owner_id, &config.account_id)
            .await?
            .ok_or_else(|| {
                AppError::bad_request("fund account must be an enabled virtual account")
            })?;
    let valuation = value_account(&account, &config.target_currency);
    let created_at = Utc::now().to_rfc3339();
    let event_index = append_fund_event(
        db,
        &config.id,
        FUND_EVENT_EQUITY_SET,
        &created_at,
        None,
        Some("Automatic NAV sample"),
        serde_json::json!({ "equity": valuation.equity }),
    )
    .await?;

    Ok(FundNavSnapshot {
        id: event_index.to_string(),
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

#[cfg(test)]
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

async fn load_cash_flow_events(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<SettlementOrder>, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
        WHERE fund_id = ?1 AND event_type = 'cash_flow_recorded'
        ORDER BY occurred_at ASC, event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            let payload = serde_json::from_str::<FundCashFlowPayload>(&row.payload)?;
            Ok(SettlementOrder {
                event_index: row.event_index,
                investor_name: row.investor_id.unwrap_or_default(),
                deposit: payload.amount,
                updated_at: row.occurred_at,
            })
        })
        .collect()
}

async fn load_equity_events(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<FundStatementEquity>, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
        WHERE fund_id = ?1 AND event_type = 'fund_equity_set'
        ORDER BY occurred_at ASC, event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            let payload = serde_json::from_str::<FundStatementEquityPayload>(&row.payload)?;
            Ok(FundStatementEquity {
                event_index: row.event_index,
                equity: payload.equity,
                updated_at: row.occurred_at,
            })
        })
        .collect()
}

async fn load_tax_events(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<FundStatementTaxMode>, AppError> {
    Ok(sqlx::query_as::<_, FundStatementTaxMode>(
        r#"
        SELECT event_index,
               CASE
                   WHEN event_type = 'taxation_v1_applied' THEN 'taxation'
                   ELSE 'taxation/v2'
               END AS mode,
               comment,
               occurred_at AS updated_at
        FROM fund_events
        WHERE fund_id = ?1
          AND event_type IN ('taxation_v1_applied', 'taxation_v2_applied')
        ORDER BY occurred_at ASC, event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?)
}

async fn load_investor_profile_events(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<FundStatementInvestor>, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
        WHERE fund_id = ?1 AND event_type = 'investor_profile_updated'
        ORDER BY event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?;
    let mut investors = HashMap::<String, FundStatementInvestorProjection>::new();
    for row in rows {
        let payload = serde_json::from_str::<FundInvestorProfilePayload>(&row.payload)?;
        let investor_id = row.investor_id.unwrap_or_default();
        let investor = investors.entry(investor_id).or_default();
        investor.referrer = payload.referrer_id.or_else(|| investor.referrer.take());
        investor.tax_rate = payload.tax_rate.or(investor.tax_rate);
        investor.referrer_rebate_rate = payload
            .referrer_rebate_rate
            .or(investor.referrer_rebate_rate);
        investor.updated_at = row.occurred_at;
        investor.source_event_index = row.event_index;
    }
    let mut rows = investors
        .into_iter()
        .map(|(name, investor)| FundStatementInvestor {
            name,
            referrer: investor.referrer,
            tax_rate: investor.tax_rate,
            referrer_rebate_rate: investor.referrer_rebate_rate,
            tax_threshold: None,
            updated_at: investor.updated_at,
            source_event_index: investor.source_event_index,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(rows)
}

async fn load_tax_threshold_adjustments(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<FundTaxThresholdAdjustment>, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
        WHERE fund_id = ?1 AND event_type = 'tax_threshold_adjusted'
        ORDER BY occurred_at ASC, event_index ASC
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
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
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
    if request.occurred_at.trim().is_empty() {
        return Err(AppError::bad_request("occurred_at is required"));
    }
    validate_fund_event_payload(
        request.event_type.trim(),
        request.investor_id.as_deref(),
        &request.payload,
    )?;
    Ok(())
}

fn validate_fund_event_payload(
    event_type: &str,
    investor_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<(), AppError> {
    match event_type {
        FUND_EVENT_EQUITY_SET => {
            require_no_investor(event_type, investor_id)?;
            serde_json::from_value::<FundStatementEquityPayload>(payload.clone())?;
        }
        FUND_EVENT_CASH_FLOW_RECORDED => {
            require_investor(event_type, investor_id)?;
            serde_json::from_value::<FundCashFlowPayload>(payload.clone())?;
        }
        FUND_EVENT_INVESTOR_PROFILE_UPDATED => {
            require_investor(event_type, investor_id)?;
            serde_json::from_value::<FundInvestorProfilePayload>(payload.clone())?;
        }
        FUND_EVENT_TAX_THRESHOLD_ADJUSTED => {
            require_investor(event_type, investor_id)?;
            serde_json::from_value::<FundTaxThresholdAdjustmentPayload>(payload.clone())?;
        }
        FUND_EVENT_TAXATION_V1_APPLIED | FUND_EVENT_TAXATION_V2_APPLIED => {
            require_no_investor(event_type, investor_id)?;
            if !payload.as_object().is_some_and(|object| object.is_empty()) {
                return Err(AppError::bad_request(format!(
                    "{event_type} payload must be an empty object"
                )));
            }
        }
        _ => {
            return Err(AppError::bad_request(format!(
                "unsupported fund event: {event_type}"
            )));
        }
    }

    Ok(())
}

fn require_investor(event_type: &str, investor_id: Option<&str>) -> Result<(), AppError> {
    if trimmed_optional_string(investor_id).is_none() {
        return Err(AppError::bad_request(format!(
            "{event_type} requires investor_id"
        )));
    }
    Ok(())
}

fn require_no_investor(event_type: &str, investor_id: Option<&str>) -> Result<(), AppError> {
    if trimmed_optional_string(investor_id).is_some() {
        return Err(AppError::bad_request(format!(
            "{event_type} must not include investor_id"
        )));
    }
    Ok(())
}

fn trimmed_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn invalidate_fund_reducer_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    fund_id: &str,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM fund_reducer_snapshots WHERE fund_id = ?1")
        .bind(fund_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

fn tax_threshold_adjustment_from_row(
    row: FundStatementEventPayloadRow,
) -> Option<Result<FundTaxThresholdAdjustment, serde_json::Error>> {
    let payload = match serde_json::from_str::<FundTaxThresholdAdjustmentPayload>(&row.payload) {
        Ok(payload) => payload,
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(FundTaxThresholdAdjustment {
        event_index: row.event_index,
        investor_name: row.investor_id.unwrap_or_default(),
        amount: payload.amount,
        comment: row.comment,
        updated_at: row.occurred_at,
    }))
}

async fn load_statement_event_state(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<FundStatementEventState, AppError> {
    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
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
        try_apply_yuan_fund_event(&mut state, row)?;
        recompute_yuan_derived(&mut state);
    }

    Ok(yuan_event_state_summary(state))
}

async fn load_fund_unit_price_candles(
    db: &SqlitePool,
    fund_id: &str,
) -> Result<Vec<FundUnitPriceCandle>, AppError> {
    let last_event_index = latest_fund_event_index(db, fund_id).await?;
    if let Some(candles) = load_cached_unit_price_candles(db, fund_id, last_event_index).await? {
        return Ok(candles);
    }

    let rows = sqlx::query_as::<_, FundStatementEventPayloadRow>(
        r#"
        SELECT event_index, event_type, occurred_at, investor_id, comment, payload
        FROM fund_events
        WHERE fund_id = ?1
        ORDER BY event_index ASC
        "#,
    )
    .bind(fund_id)
    .fetch_all(db)
    .await?;

    let candles = fund_unit_price_candles_from_rows(rows)?;
    save_unit_price_candle_snapshot(db, fund_id, last_event_index, &candles).await?;
    Ok(candles)
}

async fn latest_fund_event_index(db: &SqlitePool, fund_id: &str) -> Result<i64, AppError> {
    let (last_event_index,): (i64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(MAX(event_index), -1)
        FROM fund_events
        WHERE fund_id = ?1
        "#,
    )
    .bind(fund_id)
    .fetch_one(db)
    .await?;
    Ok(last_event_index)
}

async fn load_cached_unit_price_candles(
    db: &SqlitePool,
    fund_id: &str,
    last_event_index: i64,
) -> Result<Option<Vec<FundUnitPriceCandle>>, AppError> {
    let snapshot = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT state_json
        FROM fund_reducer_snapshots
        WHERE fund_id = ?1 AND last_event_index = ?2
        "#,
    )
    .bind(fund_id)
    .bind(last_event_index)
    .fetch_optional(db)
    .await?;
    snapshot
        .map(|(state_json,)| {
            let state = serde_json::from_str::<FundReducerSnapshotState>(&state_json)?;
            Ok(state.unit_price_candles)
        })
        .transpose()
        .map(Option::flatten)
}

async fn save_unit_price_candle_snapshot(
    db: &SqlitePool,
    fund_id: &str,
    last_event_index: i64,
    candles: &[FundUnitPriceCandle],
) -> Result<(), AppError> {
    let state_json = serde_json::to_string(&FundReducerSnapshotState {
        unit_price_candles: Some(candles.to_vec()),
    })?;
    sqlx::query(
        r#"
        INSERT INTO fund_reducer_snapshots (fund_id, last_event_index, state_json)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(fund_id) DO UPDATE SET
            last_event_index = excluded.last_event_index,
            state_json = excluded.state_json,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(fund_id)
    .bind(last_event_index)
    .bind(state_json)
    .execute(db)
    .await?;

    Ok(())
}

fn fund_unit_price_candles_from_rows(
    rows: Vec<FundStatementEventPayloadRow>,
) -> Result<Vec<FundUnitPriceCandle>, AppError> {
    let mut state = YuanFundState::default();
    let mut candles = BTreeMap::<String, FundUnitPriceCandle>::new();
    recompute_yuan_derived(&mut state);

    for row in rows {
        let day = fund_event_day(&row.occurred_at)?;
        try_apply_yuan_fund_event(&mut state, row.clone())?;
        recompute_yuan_derived(&mut state);
        let unit_price = yuan_unit_price(&state);
        candles
            .entry(day.clone())
            .and_modify(|candle| {
                candle.high = candle.high.max(unit_price);
                candle.low = candle.low.min(unit_price);
                candle.close = unit_price;
                candle.events += 1;
                candle.last_event_index = row.event_index;
            })
            .or_insert_with(|| FundUnitPriceCandle {
                day,
                open: unit_price,
                high: unit_price,
                low: unit_price,
                close: unit_price,
                events: 1,
                last_event_index: row.event_index,
            });
    }

    Ok(candles.into_values().collect())
}

fn fund_event_day(occurred_at: &str) -> Result<String, AppError> {
    let parsed = DateTime::parse_from_rfc3339(occurred_at)
        .map_err(|_| AppError::bad_request("fund event occurred_at must be RFC3339"))?;
    Ok(parsed.with_timezone(&Utc).date_naive().to_string())
}

#[cfg(test)]
fn apply_yuan_fund_event(
    event_state: &mut YuanFundState,
    event: impl Into<FundStatementEventPayloadRow>,
) {
    try_apply_yuan_fund_event(event_state, event).expect("fund event should reduce");
}

fn try_apply_yuan_fund_event(
    state: &mut YuanFundState,
    event: impl Into<FundStatementEventPayloadRow>,
) -> Result<(), AppError> {
    let event = event.into();
    if event.event_type == FUND_EVENT_EQUITY_SET {
        let fund_equity = serde_json::from_str::<FundStatementEquityPayload>(&event.payload)?;
        state.total_assets = fund_equity.equity;
    }

    if event.event_type == FUND_EVENT_CASH_FLOW_RECORDED {
        let order = serde_json::from_str::<FundCashFlowPayload>(&event.payload)?;
        let investor_id = event
            .investor_id
            .as_deref()
            .ok_or_else(|| AppError::bad_request("cash_flow_recorded requires investor_id"))?;
        let unit_price = yuan_unit_price(state);
        let share = order.amount / unit_price;
        let investor = ensure_yuan_investor(state, investor_id);
        if share > 0.0 {
            investor.avg_cost_price = (investor.avg_cost_price * investor.share + order.amount)
                / (investor.share + share);
        }
        investor.deposit += order.amount;
        investor.tax_threshold += order.amount;
        investor.share += share;
        state.total_assets += order.amount;
    }

    if event.event_type == FUND_EVENT_INVESTOR_PROFILE_UPDATED {
        let investor_update = serde_json::from_str::<FundInvestorProfilePayload>(&event.payload)?;
        let investor_id = event.investor_id.as_deref().ok_or_else(|| {
            AppError::bad_request("investor_profile_updated requires investor_id")
        })?;
        let investor = ensure_yuan_investor(state, investor_id);
        if let Some(tax_rate) = investor_update.tax_rate {
            investor.tax_rate = tax_rate;
        }
        if let Some(referrer) = investor_update.referrer_id {
            investor.referrer = Some(referrer);
        }
        if let Some(referrer_rebate_rate) = investor_update.referrer_rebate_rate {
            investor.referrer_rebate_rate = referrer_rebate_rate;
        }
    }

    if event.event_type == FUND_EVENT_TAX_THRESHOLD_ADJUSTED {
        let adjustment = serde_json::from_str::<FundTaxThresholdAdjustmentPayload>(&event.payload)?;
        let investor_id = event
            .investor_id
            .as_deref()
            .ok_or_else(|| AppError::bad_request("tax_threshold_adjusted requires investor_id"))?;
        let investor = ensure_yuan_investor(state, investor_id);
        investor.tax_threshold += adjustment.amount;
    }

    if event.event_type == FUND_EVENT_TAXATION_V1_APPLIED {
        apply_yuan_taxation(state);
    }

    if event.event_type == FUND_EVENT_TAXATION_V2_APPLIED {
        apply_yuan_taxation_v2(state);
    }

    Ok(())
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
        if investor.referrer_rebate_rate > 0.0
            && let Some(referrer) = investor
                .referrer
                .as_ref()
                .filter(|name| referrers.iter().any(|item| item == *name))
        {
            let rebate_share = tax_share * investor.referrer_rebate_rate;
            tax_account_share -= rebate_share;
            rebates.push((referrer.clone(), rebate_share, rebate_share * unit_price));
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
        if let Some(referrer) = &investor.referrer
            && investor.referrer_rebate > 0.0
            && investor_names.iter().any(|name| *name == referrer)
        {
            *rebates.entry(referrer.clone()).or_default() += investor.referrer_rebate;
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

#[cfg(test)]
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
    if let Some(value) = request.poll_interval_seconds
        && value < 60
    {
        return Err(AppError::bad_request(
            "fund poll interval must be at least 60 seconds",
        ));
    }

    Ok(())
}

fn validate_fund_access_grant_request(request: &FundAccessGrantRequest) -> Result<(), AppError> {
    if request.fund_id.trim().is_empty() {
        return Err(AppError::bad_request("missing fund id"));
    }
    if Uuid::parse_str(request.grantee_user_id.trim()).is_err() {
        return Err(AppError::bad_request("fund access user id must be a UUID"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::models::Position;

    use super::*;

    fn event_row(
        event_index: i64,
        event_type: &str,
        occurred_at: &str,
        investor_id: Option<&str>,
        payload: serde_json::Value,
    ) -> FundStatementEventPayloadRow {
        FundStatementEventPayloadRow {
            event_index,
            occurred_at: occurred_at.to_string(),
            investor_id: investor_id.map(str::to_string),
            comment: None,
            event_type: event_type.to_string(),
            payload: payload.to_string(),
        }
    }

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
    fn aggregates_daily_unit_price_candles_from_yuan_events() {
        let candles = fund_unit_price_candles_from_rows(vec![
            event_row(
                0,
                FUND_EVENT_CASH_FLOW_RECORDED,
                "2026-01-01T01:00:00+00:00",
                Some("Alice"),
                serde_json::json!({ "amount": 100.0 }),
            ),
            event_row(
                1,
                FUND_EVENT_EQUITY_SET,
                "2026-01-01T02:00:00+00:00",
                None,
                serde_json::json!({ "equity": 150.0 }),
            ),
            event_row(
                2,
                FUND_EVENT_CASH_FLOW_RECORDED,
                "2026-01-01T03:00:00+00:00",
                Some("Bob"),
                serde_json::json!({ "amount": 150.0 }),
            ),
            event_row(
                3,
                FUND_EVENT_EQUITY_SET,
                "2026-01-02T01:00:00+00:00",
                None,
                serde_json::json!({ "equity": 600.0 }),
            ),
        ])
        .unwrap();

        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].day, "2026-01-01");
        assert_close(candles[0].open, 1.0);
        assert_close(candles[0].high, 1.5);
        assert_close(candles[0].low, 1.0);
        assert_close(candles[0].close, 1.5);
        assert_eq!(candles[0].events, 3);
        assert_eq!(candles[0].last_event_index, 2);
        assert_eq!(candles[1].day, "2026-01-02");
        assert_close(candles[1].open, 3.0);
        assert_close(candles[1].high, 3.0);
        assert_close(candles[1].low, 3.0);
        assert_close(candles[1].close, 3.0);
    }

    #[tokio::test]
    async fn loads_daily_unit_price_candles_from_reducer_snapshot() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_events (
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
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_reducer_snapshots (
                fund_id TEXT PRIMARY KEY NOT NULL,
                last_event_index INTEGER NOT NULL,
                state_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO fund_events (fund_id, event_index, event_type, occurred_at, investor_id, payload)
            VALUES ('fund', 0, 'fund_equity_set', '2026-01-01T00:00:00+00:00', NULL, '{"equity":100}')
            "#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO fund_reducer_snapshots (fund_id, last_event_index, state_json)
            VALUES ('fund', 0, ?1)
            "#,
        )
        .bind(
            serde_json::json!({
                "unit_price_candles": [{
                    "day": "2026-01-09",
                    "open": 9.0,
                    "high": 10.0,
                    "low": 8.0,
                    "close": 9.5,
                    "events": 7,
                    "last_event_index": 0
                }]
            })
            .to_string(),
        )
        .execute(&db)
        .await
        .unwrap();

        let candles = load_fund_unit_price_candles(&db, "fund").await.unwrap();

        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].day, "2026-01-09");
        assert_close(candles[0].open, 9.0);
        assert_close(candles[0].high, 10.0);
        assert_close(candles[0].low, 8.0);
        assert_close(candles[0].close, 9.5);
        assert_eq!(candles[0].events, 7);
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
            occurred_at: "2025-09-30T15:59:59.999000+00:00".to_string(),
            investor_id: Some("张秦".to_string()),
            comment: Some("快捷申报免税 张秦 108.06756441281664".to_string()),
            event_type: FUND_EVENT_TAX_THRESHOLD_ADJUSTED.to_string(),
            payload: r#"{"amount":108.06756441281664}"#.to_string(),
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
    async fn folds_statement_events_by_event_index_not_timestamp() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE fund_events (
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
        .execute(&db)
        .await
        .unwrap();
        for (event_index, event_type, investor_id, occurred_at, payload) in [
            (
                0,
                FUND_EVENT_CASH_FLOW_RECORDED,
                Some("Alice"),
                "2025-01-02T00:00:00+00:00",
                r#"{"amount":100}"#,
            ),
            (
                1,
                FUND_EVENT_EQUITY_SET,
                None,
                "2025-01-01T00:00:00+00:00",
                r#"{"equity":150}"#,
            ),
        ] {
            sqlx::query(
                r#"
                INSERT INTO fund_events (fund_id, event_index, event_type, occurred_at, investor_id, payload)
                VALUES ('fund', ?1, ?2, ?3, ?4, ?5)
                "#,
            )
            .bind(event_index)
            .bind(event_type)
            .bind(occurred_at)
            .bind(investor_id)
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
