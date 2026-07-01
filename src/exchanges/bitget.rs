use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product, TradeFill},
};

pub const ID: &str = "BITGET";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key", "passphrase"];
const BASE_URL: &str = "https://api.bitget.com";
const ACCOUNT_ASSETS_PATH: &str = "/api/v3/account/assets";
const ACCOUNT_SETTINGS_PATH: &str = "/api/v3/account/settings";
const INSTRUMENTS_URL: &str = "https://api.bitget.com/api/v3/market/instruments";
const CURRENT_POSITION_PATH: &str = "/api/v3/position/current-position";
const TRADE_FILLS_PATH: &str = "/api/v3/trade/fills";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Bitget", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let mut products = Vec::new();
        for category in ["USDT-FUTURES", "COIN-FUTURES", "SPOT"] {
            let response = client
                .get(INSTRUMENTS_URL)
                .query(&[("category", category)])
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
            products.extend(rows.into_iter().map(|row| map_product(category, &row)));
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
        let response = bitget_get(credential, ACCOUNT_SETTINGS_PATH, &[]).await?;
        let uid = bitget_account_uid(&response)?;

        Ok(common::account_id(ID, uid))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let assets = bitget_get(credential, ACCOUNT_ASSETS_PATH, &[]).await?;
        let usdt_futures = bitget_get(
            credential,
            CURRENT_POSITION_PATH,
            &[("category", "USDT-FUTURES")],
        )
        .await?;
        let coin_futures = bitget_get(
            credential,
            CURRENT_POSITION_PATH,
            &[("category", "COIN-FUTURES")],
        )
        .await?;
        let asset_rows = assets
            .get("data")
            .and_then(|data| data.get("assets"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let usdt_rows = position_rows(&usdt_futures);
        let coin_rows = position_rows(&coin_futures);

        Ok(asset_rows
            .into_iter()
            .filter_map(map_asset_position)
            .chain(
                usdt_rows
                    .into_iter()
                    .map(|row| map_derivative_position("USDT-FUTURES", row))
                    .chain(
                        coin_rows
                            .into_iter()
                            .map(|row| map_derivative_position("COIN-FUTURES", row)),
                    )
                    .flatten(),
            )
            .collect())
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }

    async fn list_trades(&self, credential: &Value) -> anyhow::Result<Vec<TradeFill>> {
        let mut trades = Vec::new();
        for category in ["USDT-FUTURES", "COIN-FUTURES", "SPOT"] {
            let response = bitget_get(
                credential,
                TRADE_FILLS_PATH,
                &[("category", category), ("limit", "100")],
            )
            .await?;
            let rows = response
                .get("data")
                .and_then(|data| data.get("list"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            trades.extend(
                rows.into_iter()
                    .filter_map(|row| map_trade_fill(category, row)),
            );
        }

        Ok(trades)
    }
}

fn bitget_account_uid(response: &Value) -> anyhow::Result<String> {
    let uid = response
        .get("data")
        .map(|data| common::text_value(data, "uid"))
        .unwrap_or_default();
    anyhow::ensure!(!uid.is_empty(), "Bitget account uid is missing");

    Ok(uid)
}

async fn bitget_get(
    credential: &Value,
    path: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<Value> {
    let query_string = encode_query(query);
    let url = if query_string.is_empty() {
        format!("{BASE_URL}{path}")
    } else {
        format!("{BASE_URL}{path}?{query_string}")
    };
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let signature = bitget_signature(
        &common::str_value(credential, "secret_key"),
        timestamp,
        "GET",
        path,
        &query_string,
        "",
    )?;
    let response = common::http_client()?
        .get(url)
        .header("ACCESS-KEY", common::str_value(credential, "access_key"))
        .header("ACCESS-SIGN", signature)
        .header("ACCESS-TIMESTAMP", timestamp.to_string())
        .header(
            "ACCESS-PASSPHRASE",
            common::str_value(credential, "passphrase"),
        )
        .header("content-type", "application/json")
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("Bitget request failed: {status} {body}");
    }
    let value: Value = serde_json::from_str(&body)?;
    if common::str_value(&value, "msg") != "success" {
        anyhow::bail!("Bitget request failed: {value}");
    }

    Ok(value)
}

fn bitget_signature(
    secret_key: &str,
    timestamp: u128,
    method: &str,
    path: &str,
    query: &str,
    body: &str,
) -> anyhow::Result<String> {
    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    let payload = format!("{timestamp}{method}{request_path}{body}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())?;
    mac.update(payload.as_bytes());

    Ok(BASE64_STANDARD.encode(mac.finalize().into_bytes()))
}

fn encode_query(query: &[(&str, &str)]) -> String {
    query
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn position_rows(response: &Value) -> Vec<Value> {
    response
        .get("data")
        .and_then(|data| data.get("list"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn map_asset_position(row: Value) -> Option<Position> {
    let coin = common::str_value(&row, "coin");
    let volume = common::f64_value(&row, "balance");
    if coin.is_empty() || volume <= 0.0 {
        return None;
    }
    let closable_price = if matches!(coin.as_str(), "USDT" | "USDC" | "USDD") {
        1.0
    } else {
        0.0
    };

    Some(Position {
        position_id: format!("UTA-ASSET/{coin}"),
        product_id: if coin == "USDT" {
            format!("{ID}/SPOT/USDT")
        } else {
            format!("{ID}/SPOT/{coin}USDT")
        },
        base_currency: Some(coin.clone()),
        quote_currency: Some(coin.clone()),
        direction: None,
        volume,
        free_volume: common::f64_value(&row, "available"),
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some(coin),
        floating_profit: 0.0,
        comment: None,
        ..Position::default()
    })
}

fn map_derivative_position(category: &str, row: Value) -> Option<Position> {
    let symbol = common::str_value(&row, "symbol");
    let total = common::f64_value(&row, "total");
    if symbol.is_empty() || total == 0.0 {
        return None;
    }
    let closable_price = common::f64_value(&row, "markPrice");
    let (base_currency, quote_currency) = bitget_symbol_currencies(&symbol);

    Some(Position {
        position_id: format!("{}-{}", symbol, common::str_value(&row, "posSide")),
        product_id: format!("{ID}/{category}/{symbol}"),
        base_currency,
        quote_currency,
        direction: Some(if common::str_value(&row, "posSide") == "long" {
            PositionDirection::Long
        } else {
            PositionDirection::Short
        }),
        volume: total.abs(),
        free_volume: common::f64_value(&row, "available").abs(),
        position_price: common::f64_value(&row, "avgPrice"),
        closable_price,
        notional_value: common::notional_value(total, closable_price),
        notional_currency: Some(bitget_notional_currency(category).to_string()),
        floating_profit: common::f64_value(&row, "unrealisedPnl"),
        comment: None,
        ..Position::default()
    })
}

fn bitget_symbol_currencies(symbol: &str) -> (Option<String>, Option<String>) {
    for quote in ["USDT", "USDC", "USD"] {
        if let Some(base) = symbol.strip_suffix(quote) {
            return (Some(base.to_string()), Some(quote.to_string()));
        }
    }
    (Some(symbol.to_string()), None)
}

fn bitget_notional_currency(category: &str) -> &str {
    if category == "COIN-FUTURES" {
        "USD"
    } else {
        "USDT"
    }
}

fn map_trade_fill(category: &str, row: Value) -> Option<TradeFill> {
    let trade_id = common::str_value(&row, "execId");
    let symbol = common::str_value(&row, "symbol");
    let price = common::f64_value(&row, "execPrice");
    let volume = common::f64_value(&row, "execQty");
    if trade_id.is_empty() || symbol.is_empty() || volume == 0.0 {
        return None;
    }
    let fee = row
        .get("feeDetail")
        .and_then(Value::as_array)
        .and_then(|fees| fees.first())
        .cloned()
        .unwrap_or_default();

    Some(TradeFill {
        exchange: ID.to_string(),
        trade_id,
        order_id: Some(common::str_value(&row, "orderId")).filter(|value| !value.is_empty()),
        product_id: format!("{ID}/{category}/{symbol}"),
        direction: bitget_fill_direction(&common::str_value(&row, "side")),
        price,
        volume: volume.abs(),
        value: common::f64_value(&row, "execValue"),
        value_currency: Some(bitget_notional_currency(category).to_string()),
        fee: common::f64_value(&fee, "fee"),
        fee_currency: Some(common::str_value(&fee, "feeCoin")).filter(|value| !value.is_empty()),
        created_at: Some(common::str_value(&row, "createdTime")).filter(|value| !value.is_empty()),
    })
}

fn bitget_fill_direction(side: &str) -> Option<PositionDirection> {
    match side {
        "buy" => Some(PositionDirection::Long),
        "sell" => Some(PositionDirection::Short),
        _ => None,
    }
}

fn map_product(category: &str, row: &Value) -> Product {
    let symbol = common::str_value(row, "symbol");
    let max_leverage = common::f64_value(row, "maxLeverage");
    let quantity_precision = common::opt_f64_value(row, "quantityPrecision");
    let price_precision = common::opt_f64_value(row, "pricePrecision");

    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/{category}/{symbol}"),
        name: Some(symbol),
        quote_currency: Some(common::str_value(row, "quoteCoin")).filter(|value| !value.is_empty()),
        base_currency: Some(common::str_value(row, "baseCoin")).filter(|value| !value.is_empty()),
        price_step: price_precision.map(common::pow_step),
        volume_step: common::normalized_volume_step(
            common::opt_f64_value(row, "quantityMultiplier")
                .or_else(|| quantity_precision.map(common::pow_step)),
            None,
        ),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: if category == "SPOT" || max_leverage <= 0.0 {
            None
        } else {
            Some(1.0 / max_leverage)
        },
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(category != "SPOT"),
        market_id: Some(format!("{ID}/{category}")),
        no_interest_rate: Some(category == "SPOT"),
        spread: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn bitget_account_uid_reads_settings_uid() {
        let response = json!({
            "data": {
                "uid": 6893877321_u64
            }
        });

        assert_eq!(bitget_account_uid(&response).unwrap(), "6893877321");
    }

    #[test]
    fn bitget_account_uid_requires_uid() {
        let response = json!({
            "data": {}
        });

        assert!(bitget_account_uid(&response).is_err());
    }
}
