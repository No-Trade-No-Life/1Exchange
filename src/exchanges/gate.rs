use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product},
};

pub const ID: &str = "GATE";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];
const BASE_URL: &str = "https://api.gateio.ws";
const FUTURES_CONTRACTS_URL: &str = "https://api.gateio.ws/api/v4/futures/usdt/contracts";
const FUTURES_POSITIONS_PATH: &str = "/api/v4/futures/usdt/positions";
const SPOT_ACCOUNTS_PATH: &str = "/api/v4/spot/accounts";
const SPOT_PAIRS_URL: &str = "https://api.gateio.ws/api/v4/spot/currency_pairs";
const UNIFIED_ACCOUNTS_PATH: &str = "/api/v4/unified/accounts";
const EARN_BALANCE_PATH: &str = "/api/v4/earn/uni/lends";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Gate.io", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let futures = client
            .get(FUTURES_CONTRACTS_URL)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Value>>()
            .await?;
        let spot = client
            .get(SPOT_PAIRS_URL)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Value>>()
            .await?;

        Ok(futures
            .into_iter()
            .map(map_future_product)
            .chain(spot.into_iter().map(map_spot_product))
            .collect())
    }

    async fn get_account(&self, credential: &Value) -> anyhow::Result<AccountInfo> {
        let positions = self.list_positions(credential).await?;

        Ok(AccountInfo {
            account_id: format!("{ID}/{}", common::str_value(credential, "access_key")),
            positions,
            orders: Vec::new(),
            timestamp_in_us: common::now_timestamp_in_us(),
        })
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let spot = gate_get(credential, SPOT_ACCOUNTS_PATH, "").await?;
        let futures = gate_get(credential, FUTURES_POSITIONS_PATH, "").await?;
        let unified = gate_get(credential, UNIFIED_ACCOUNTS_PATH, "").await?;
        let earn = gate_get(credential, EARN_BALANCE_PATH, "").await?;
        let spot_rows = spot.as_array().cloned().unwrap_or_default();
        let futures_rows = futures.as_array().cloned().unwrap_or_default();
        let unified_rows = unified_balance_rows(&unified);
        let earn_rows = earn.as_array().cloned().unwrap_or_default();

        Ok(spot_rows
            .into_iter()
            .filter_map(map_spot_position)
            .chain(futures_rows.into_iter().filter_map(map_future_position))
            .chain(unified_rows.into_iter().filter_map(map_unified_position))
            .chain(earn_rows.into_iter().filter_map(map_earn_position))
            .collect())
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }
}

async fn gate_get(credential: &Value, path: &str, query: &str) -> anyhow::Result<Value> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let signature = gate_signature(
        &common::str_value(credential, "secret_key"),
        "GET",
        path,
        query,
        "",
        timestamp,
    )?;
    let url = if query.is_empty() {
        format!("{BASE_URL}{path}")
    } else {
        format!("{BASE_URL}{path}?{query}")
    };
    let response = common::http_client()?
        .get(url)
        .header("KEY", common::str_value(credential, "access_key"))
        .header("Timestamp", timestamp.to_string())
        .header("SIGN", signature)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("Gate request failed: {status} {body}");
    }

    Ok(serde_json::from_str(&body)?)
}

fn gate_signature(
    secret_key: &str,
    method: &str,
    path: &str,
    query: &str,
    body: &str,
    timestamp: u64,
) -> anyhow::Result<String> {
    let body_hash = hex::encode(Sha512::digest(body.as_bytes()));
    let payload = format!("{method}\n{path}\n{query}\n{body_hash}\n{timestamp}");
    let mut mac = Hmac::<Sha512>::new_from_slice(secret_key.as_bytes())?;
    mac.update(payload.as_bytes());

    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn unified_balance_rows(response: &Value) -> Vec<Value> {
    response
        .get("balances")
        .and_then(Value::as_object)
        .map(|balances| {
            balances
                .iter()
                .map(|(currency, balance)| {
                    let mut row = balance.clone();
                    row["currency"] = Value::String(currency.clone());
                    row
                })
                .collect()
        })
        .unwrap_or_default()
}

fn map_spot_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "currency");
    let available = common::f64_value(&row, "available");
    let locked = common::f64_value(&row, "locked");
    let volume = available + locked;
    if currency.is_empty() || volume <= 0.0 {
        return None;
    }
    let closable_price = if currency == "USDT" { 1.0 } else { 0.0 };

    Some(Position {
        position_id: format!("SPOT/{currency}"),
        product_id: if currency == "USDT" {
            format!("{ID}/SPOT/USDT")
        } else {
            format!("{ID}/SPOT/{currency}_USDT")
        },
        direction: None,
        volume,
        free_volume: available,
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
    })
}

fn map_unified_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "currency");
    let volume = common::f64_value(&row, "available");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }
    let closable_price = if currency == "USDT" { 1.0 } else { 0.0 };

    Some(Position {
        position_id: format!("UNIFIED/{currency}"),
        product_id: spot_product_id(&currency),
        direction: None,
        volume,
        free_volume: volume,
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
    })
}

fn map_earn_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "currency");
    let volume = common::f64_value(&row, "amount");
    if currency.is_empty() || volume <= 0.0 {
        return None;
    }
    let frozen = common::f64_value(&row, "frozen_amount");
    let closable_price = if currency == "USDT" { 1.0 } else { 0.0 };

    Some(Position {
        position_id: format!("EARNING/{currency}"),
        product_id: format!("{ID}/EARNING/{currency}"),
        direction: None,
        volume,
        free_volume: (volume - frozen).max(0.0),
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
    })
}

fn spot_product_id(currency: &str) -> String {
    if currency == "USDT" {
        format!("{ID}/SPOT/USDT")
    } else {
        format!("{ID}/SPOT/{currency}_USDT")
    }
}

fn map_future_position(row: Value) -> Option<Position> {
    let contract = common::str_value(&row, "contract");
    let size = common::f64_value(&row, "size");
    if contract.is_empty() || size == 0.0 {
        return None;
    }
    let closable_price = common::f64_value(&row, "mark_price");
    let notional_value = common::f64_value(&row, "value").abs();

    Some(Position {
        position_id: contract.clone(),
        product_id: format!("{ID}/FUTURE/{contract}"),
        direction: Some(if size < 0.0 {
            PositionDirection::Short
        } else {
            PositionDirection::Long
        }),
        volume: size.abs(),
        free_volume: size.abs(),
        position_price: common::f64_value(&row, "entry_price"),
        closable_price,
        notional_value: if notional_value > 0.0 {
            notional_value
        } else {
            common::notional_value(size, closable_price)
        },
        notional_currency: Some("USDT".to_string()),
        floating_profit: common::f64_value(&row, "unrealised_pnl"),
        comment: None,
    })
}

fn map_future_product(row: Value) -> Product {
    let name = common::str_value(&row, "name");
    let mut parts = name.split('_');
    let base = parts.next().unwrap_or_default().to_string();
    let quote = parts.next().unwrap_or_default().to_string();
    let leverage = common::f64_value(&row, "leverage_max");

    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/FUTURE/{name}"),
        name: Some(name),
        quote_currency: Some(quote).filter(|value| !value.is_empty()),
        base_currency: Some(base).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(&row, "order_price_round"),
        volume_step: Some(1.0),
        value_scale: common::opt_f64_value(&row, "quanto_multiplier"),
        value_scale_unit: None,
        margin_rate: if leverage > 0.0 {
            Some(1.0 / leverage)
        } else {
            None
        },
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(true),
        spread: None,
    }
}

fn map_spot_product(row: Value) -> Product {
    let id = common::str_value(&row, "id");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/SPOT/{id}"),
        name: Some(id),
        quote_currency: Some(common::str_value(&row, "quote")).filter(|value| !value.is_empty()),
        base_currency: Some(common::str_value(&row, "base")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(&row, "precision").map(common::pow_step),
        volume_step: Some(1.0),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: None,
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(false),
        spread: None,
    }
}
