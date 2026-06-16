use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product, TradeFill},
};

pub const ID: &str = "OKX";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key", "passphrase"];
const BASE_URL: &str = "https://www.okx.com";
const INSTRUMENTS_URL: &str = "https://www.okx.com/api/v5/public/instruments";
const BALANCE_PATH: &str = "/api/v5/account/balance";
const POSITIONS_PATH: &str = "/api/v5/account/positions";
const FUNDING_BALANCES_PATH: &str = "/api/v5/asset/balances";
const SAVINGS_BALANCE_PATH: &str = "/api/v5/finance/savings/balance";
const FLEXIBLE_LOAN_PATH: &str = "/api/v5/finance/flexible-loan/loan-info";
const FILLS_HISTORY_PATH: &str = "/api/v5/trade/fills-history";
const ACCOUNT_CONFIG_PATH: &str = "/api/v5/account/config";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "OKX", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let mut products = Vec::new();
        for inst_type in ["SPOT", "MARGIN", "SWAP"] {
            let response = client
                .get(INSTRUMENTS_URL)
                .query(&[("instType", inst_type)])
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let rows = response
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            products.extend(rows.into_iter().map(|row| map_product(inst_type, &row)));
        }

        Ok(products)
    }

    async fn get_account(&self, credential: &Value) -> anyhow::Result<AccountInfo> {
        let account_id = self.get_account_id(credential).await?;
        let positions = self.list_positions(credential).await?;

        Ok(AccountInfo {
            account_id,
            positions,
            orders: Vec::new(),
            timestamp_in_us: common::now_timestamp_in_us(),
        })
    }

    async fn get_account_id(&self, credential: &Value) -> anyhow::Result<String> {
        let config = okx_get(credential, ACCOUNT_CONFIG_PATH).await?;
        let uid = config
            .get("data")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .map(|row| common::text_value(row, "uid"))
            .unwrap_or_default();
        anyhow::ensure!(!uid.is_empty(), "OKX account uid is missing");

        Ok(common::account_id(ID, uid))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let balance = okx_get(credential, BALANCE_PATH).await?;
        let positions = okx_get(credential, POSITIONS_PATH).await?;
        let funding_balances = okx_get(credential, FUNDING_BALANCES_PATH).await?;
        let savings_balances = okx_get(credential, SAVINGS_BALANCE_PATH).await?;
        let flexible_loan = okx_get(credential, FLEXIBLE_LOAN_PATH).await?;
        let balance_rows = balance
            .get("data")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .and_then(|row| row.get("details"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let position_rows = positions
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let funding_rows = data_rows(&funding_balances);
        let savings_rows = data_rows(&savings_balances);
        let loan_summary = flexible_loan
            .get("data")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .cloned()
            .unwrap_or_default();

        Ok(balance_rows
            .into_iter()
            .filter_map(map_balance_position)
            .chain(
                position_rows
                    .into_iter()
                    .filter_map(map_derivative_position),
            )
            .chain(funding_rows.into_iter().filter_map(map_funding_position))
            .chain(savings_rows.into_iter().filter_map(map_savings_position))
            .chain(loan_rows(&loan_summary, "loanData").filter_map(map_loan_position))
            .chain(loan_rows(&loan_summary, "collateralData").filter_map(map_collateral_position))
            .collect())
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }

    async fn list_trades(&self, credential: &Value) -> anyhow::Result<Vec<TradeFill>> {
        let mut trades = Vec::new();
        for inst_type in ["SPOT", "SWAP"] {
            let response = okx_get_query(
                credential,
                FILLS_HISTORY_PATH,
                &format!("instType={inst_type}&limit=100"),
            )
            .await?;
            trades.extend(data_rows(&response).into_iter().filter_map(map_trade_fill));
        }

        Ok(trades)
    }
}

async fn okx_get(credential: &Value, path: &str) -> anyhow::Result<Value> {
    okx_get_query(credential, path, "").await
}

async fn okx_get_query(credential: &Value, path: &str, query: &str) -> anyhow::Result<Value> {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    let signature = okx_signature(
        &common::str_value(credential, "secret_key"),
        &timestamp,
        "GET",
        &request_path,
        "",
    )?;
    let response = common::http_client()?
        .get(format!("{BASE_URL}{request_path}"))
        .header("OK-ACCESS-KEY", common::str_value(credential, "access_key"))
        .header("OK-ACCESS-SIGN", signature)
        .header("OK-ACCESS-TIMESTAMP", timestamp)
        .header(
            "OK-ACCESS-PASSPHRASE",
            common::str_value(credential, "passphrase"),
        )
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let code = common::str_value(&response, "code");
    if code != "0" {
        anyhow::bail!("OKX request failed: {response}");
    }

    Ok(response)
}

fn okx_signature(
    secret_key: &str,
    timestamp: &str,
    method: &str,
    path: &str,
    body: &str,
) -> anyhow::Result<String> {
    let message = format!("{timestamp}{method}{path}{body}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())?;
    mac.update(message.as_bytes());
    Ok(BASE64_STANDARD.encode(mac.finalize().into_bytes()))
}

fn data_rows(response: &Value) -> Vec<Value> {
    response
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn loan_rows(summary: &Value, field: &str) -> impl Iterator<Item = Value> {
    summary
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
}

fn map_balance_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "ccy");
    let volume = common::f64_value(&row, "cashBal");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }
    let closable_price = common::stablecoin_unit_price(&currency);

    Some(Position {
        position_id: format!("BALANCE/{currency}"),
        product_id: if currency == "USDT" {
            format!("{ID}/SPOT/USDT")
        } else {
            format!("{ID}/SPOT/{currency}-USDT")
        },
        base_currency: Some(currency.clone()),
        quote_currency: Some("USDT".to_string()),
        direction: None,
        volume: volume.abs(),
        free_volume: common::f64_value(&row, "availBal").abs(),
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
    })
}

fn map_funding_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "ccy");
    let volume = common::f64_value(&row, "bal");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }

    Some(asset_position(
        format!("FUNDING-ASSET/{currency}"),
        currency,
        volume,
        common::f64_value(&row, "availBal"),
    ))
}

fn map_savings_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "ccy");
    let volume = common::f64_value(&row, "amt");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }

    Some(asset_position(
        format!("EARNING-ASSET/{currency}"),
        currency,
        volume,
        volume,
    ))
}

fn map_loan_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "ccy");
    let volume = common::f64_value(&row, "amt");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }
    let closable_price = 1.0;

    Some(Position {
        position_id: format!("LOAN/{currency}"),
        product_id: format!("{ID}/LOAN/{currency}"),
        base_currency: Some(currency.clone()),
        quote_currency: Some(currency.clone()),
        direction: Some(PositionDirection::Short),
        volume: volume.abs(),
        free_volume: volume.abs(),
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some(currency),
        floating_profit: 0.0,
        comment: None,
    })
}

fn map_collateral_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "ccy");
    let volume = common::f64_value(&row, "amt");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }

    Some(asset_position(
        format!("COLLATERAL/{currency}"),
        currency,
        volume,
        volume,
    ))
}

fn asset_position(
    position_id: String,
    currency: String,
    volume: f64,
    free_volume: f64,
) -> Position {
    Position {
        product_id: format!("{ID}/{position_id}"),
        position_id,
        base_currency: Some(currency.clone()),
        quote_currency: Some(currency.clone()),
        direction: None,
        volume: volume.abs(),
        free_volume: free_volume.abs(),
        position_price: 0.0,
        closable_price: 1.0,
        notional_value: volume.abs(),
        notional_currency: Some(currency),
        floating_profit: 0.0,
        comment: None,
    }
}

fn map_derivative_position(row: Value) -> Option<Position> {
    let inst_id = common::str_value(&row, "instId");
    let inst_type = common::str_value(&row, "instType");
    let position_id = common::str_value(&row, "posId");
    let volume = common::f64_value(&row, "pos");
    if inst_id.is_empty() || volume == 0.0 {
        return None;
    }
    let side = common::str_value(&row, "posSide");
    let direction = if side == "short" || volume < 0.0 {
        crate::models::PositionDirection::Short
    } else {
        crate::models::PositionDirection::Long
    };
    let closable_price = common::f64_value(&row, "markPx");
    let (base_currency, quote_currency) = okx_pair_currencies(&inst_id);

    Some(Position {
        position_id,
        product_id: format!("{ID}/{inst_type}/{inst_id}"),
        base_currency,
        quote_currency,
        direction: Some(direction),
        volume: volume.abs(),
        free_volume: common::f64_value(&row, "availPos").abs(),
        position_price: common::f64_value(&row, "avgPx"),
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: okx_quote_currency(&inst_id),
        floating_profit: common::f64_value(&row, "upl"),
        comment: None,
    })
}

fn okx_pair_currencies(inst_id: &str) -> (Option<String>, Option<String>) {
    let mut parts = inst_id.split('-');
    (
        parts.next().map(str::to_string),
        parts.next().map(str::to_string),
    )
}

fn map_trade_fill(row: Value) -> Option<TradeFill> {
    let trade_id = common::text_value(&row, "tradeId");
    let inst_id = common::str_value(&row, "instId");
    let price = common::f64_value(&row, "fillPx");
    let volume = common::f64_value(&row, "fillSz");
    if trade_id.is_empty() || inst_id.is_empty() || volume == 0.0 {
        return None;
    }

    Some(TradeFill {
        exchange: ID.to_string(),
        trade_id,
        order_id: Some(common::text_value(&row, "ordId")).filter(|value| !value.is_empty()),
        product_id: format!("{ID}/{}/{}", common::str_value(&row, "instType"), inst_id),
        direction: okx_fill_direction(&common::str_value(&row, "side")),
        price,
        volume: volume.abs(),
        value: common::notional_value(volume, price),
        value_currency: okx_quote_currency(&inst_id),
        fee: common::f64_value(&row, "fee"),
        fee_currency: Some(common::str_value(&row, "feeCcy")).filter(|value| !value.is_empty()),
        created_at: Some(common::text_value(&row, "ts")).filter(|value| !value.is_empty()),
    })
}

fn okx_fill_direction(side: &str) -> Option<PositionDirection> {
    match side {
        "buy" => Some(PositionDirection::Long),
        "sell" => Some(PositionDirection::Short),
        _ => None,
    }
}

fn okx_quote_currency(inst_id: &str) -> Option<String> {
    inst_id.split('-').nth(1).map(str::to_string)
}

fn map_product(inst_type: &str, row: &Value) -> Product {
    let inst_id = common::str_value(row, "instId");
    let lever = common::opt_f64_value(row, "lever").unwrap_or(1.0);
    let value_scale = common::opt_f64_value(row, "ctVal");

    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/{inst_type}/{inst_id}"),
        name: Some(inst_id),
        quote_currency: Some(common::str_value(row, "quoteCcy")).filter(|value| !value.is_empty()),
        base_currency: Some(common::str_value(row, "baseCcy")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(row, "tickSz"),
        volume_step: common::normalized_volume_step(
            common::opt_f64_value(row, "lotSz"),
            value_scale,
        ),
        value_scale: Some(1.0),
        value_scale_unit: Some(common::str_value(row, "ctValCcy"))
            .filter(|value| !value.is_empty()),
        margin_rate: if inst_type == "SWAP" && lever > 0.0 {
            Some(1.0 / lever)
        } else {
            None
        },
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(inst_type != "SPOT"),
        spread: None,
    }
}
