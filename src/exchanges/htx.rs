use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::collections::BTreeMap;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product, TradeFill},
};

pub const ID: &str = "HTX";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];
const SPOT_BASE_URL: &str = "https://api.huobi.pro";
const SPOT_HOST: &str = "api.huobi.pro";
const SPOT_ACCOUNTS_PATH: &str = "/v1/account/accounts";
const SPOT_BALANCE_PATH_PREFIX: &str = "/v1/account/accounts/";
const USER_UID_PATH: &str = "/v2/user/uid";
const SWAP_CONTRACT_INFO_URL: &str = "https://api.hbdm.com/linear-swap-api/v1/swap_contract_info";
const SWAP_HOST: &str = "api.hbdm.com";
const SWAP_ACCOUNT_TYPE_PATH: &str = "/linear-swap-api/v3/swap_unified_account_type";
const SWAP_POSITION_INFO_PATH: &str = "/linear-swap-api/v1/swap_position_info";
const UNION_ACCOUNT_BALANCE_PATH: &str = "/v5/account/balance";
const UNION_POSITION_OPENS_PATH: &str = "/v5/trade/position/opens";
const UNION_ORDER_DETAILS_PATH: &str = "/v5/trade/order/details";
const SWAP_BASE_URL: &str = "https://api.hbdm.com";
const SPOT_SYMBOLS_URL: &str = "https://api.huobi.pro/v2/settings/common/symbols";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "HTX", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let swap = client
            .get(SWAP_CONTRACT_INFO_URL)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let spot = client
            .get(SPOT_SYMBOLS_URL)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        let swap_rows = swap
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let spot_rows = spot
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(swap_rows
            .into_iter()
            .filter(|row| common::f64_value(row, "contract_status") == 1.0)
            .map(map_swap_product)
            .chain(
                spot_rows
                    .into_iter()
                    .filter(|row| common::str_value(row, "state") == "online")
                    .map(map_spot_product),
            )
            .collect())
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
        let response = htx_get(credential, SPOT_BASE_URL, SPOT_HOST, USER_UID_PATH).await?;
        let uid = common::text_value(&response, "data");
        anyhow::ensure!(!uid.is_empty(), "HTX account uid is missing");

        Ok(common::account_id(ID, uid))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let accounts = htx_get(credential, SPOT_BASE_URL, SPOT_HOST, SPOT_ACCOUNTS_PATH).await?;
        let spot_account_id = spot_account_id(&accounts)?;
        let balance_path = format!("{SPOT_BALANCE_PATH_PREFIX}{spot_account_id}/balance");
        let balance = htx_get(credential, SPOT_BASE_URL, SPOT_HOST, &balance_path).await?;
        let account_type = swap_account_type(credential).await?;
        let swap_balance = swap_balance(credential, account_type).await?;
        let swap = swap_positions(credential, account_type).await?;
        let contract_sizes = swap_contract_sizes().await?;
        let balance_rows = balance
            .get("data")
            .and_then(|row| row.get("list"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let swap_rows = swap
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(balance_rows
            .into_iter()
            .filter_map(map_spot_position)
            .chain(swap_balance)
            .chain(
                swap_rows
                    .into_iter()
                    .filter_map(|row| map_swap_position(row, &contract_sizes)),
            )
            .collect())
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }

    async fn list_trades(&self, credential: &Value) -> anyhow::Result<Vec<TradeFill>> {
        let now = chrono::Utc::now().timestamp_millis();
        let start = now.saturating_sub(48 * 60 * 60 * 1000);
        let response = htx_get_query(
            credential,
            SWAP_BASE_URL,
            SWAP_HOST,
            UNION_ORDER_DETAILS_PATH,
            &BTreeMap::from([
                ("start_time".to_string(), start.to_string()),
                ("end_time".to_string(), now.to_string()),
                ("direct".to_string(), "next".to_string()),
                ("limit".to_string(), "100".to_string()),
            ]),
        )
        .await?;
        let rows = response
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(rows.into_iter().filter_map(map_trade_fill).collect())
    }
}

async fn htx_get(
    credential: &Value,
    base_url: &str,
    host: &str,
    path: &str,
) -> anyhow::Result<Value> {
    htx_get_query(credential, base_url, host, path, &BTreeMap::new()).await
}

async fn htx_get_query(
    credential: &Value,
    base_url: &str,
    host: &str,
    path: &str,
    params: &BTreeMap<String, String>,
) -> anyhow::Result<Value> {
    let query = htx_signed_query(credential, "GET", host, path, params)?;
    let response = common::http_client()?
        .get(format!("{base_url}{path}?{query}"))
        .send()
        .await?;
    decode_htx_response(response).await
}

async fn htx_post(
    credential: &Value,
    base_url: &str,
    host: &str,
    path: &str,
    body: &str,
) -> anyhow::Result<Value> {
    let query = htx_signed_query(credential, "POST", host, path, &BTreeMap::new())?;
    let response = common::http_client()?
        .post(format!("{base_url}{path}?{query}"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await?;
    decode_htx_response(response).await
}

async fn decode_htx_response(response: reqwest::Response) -> anyhow::Result<Value> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("HTX request failed: {status} {body}");
    }
    let value: Value = serde_json::from_str(&body)?;
    if common::str_value(&value, "status") != "ok" && common::f64_value(&value, "code") != 200.0 {
        anyhow::bail!("HTX request failed: {value}");
    }

    Ok(value)
}

async fn swap_account_type(credential: &Value) -> anyhow::Result<f64> {
    let response = htx_get(credential, SWAP_BASE_URL, SWAP_HOST, SWAP_ACCOUNT_TYPE_PATH).await?;
    Ok(response
        .get("data")
        .map(|data| common::f64_value(data, "account_type"))
        .unwrap_or_default())
}

async fn swap_balance(credential: &Value, account_type: f64) -> anyhow::Result<Vec<Position>> {
    if account_type == 2.0 {
        let response = htx_get(
            credential,
            SWAP_BASE_URL,
            SWAP_HOST,
            UNION_ACCOUNT_BALANCE_PATH,
        )
        .await?;
        let rows = response
            .get("data")
            .and_then(|data| data.get("details"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        return Ok(rows.into_iter().filter_map(map_union_swap_asset).collect());
    }

    Ok(Vec::new())
}

async fn swap_positions(credential: &Value, account_type: f64) -> anyhow::Result<Value> {
    if account_type == 2.0 {
        return htx_get(
            credential,
            SWAP_BASE_URL,
            SWAP_HOST,
            UNION_POSITION_OPENS_PATH,
        )
        .await;
    }

    let path = SWAP_POSITION_INFO_PATH;

    htx_post(
        credential,
        SWAP_BASE_URL,
        SWAP_HOST,
        path,
        r#"{"contract_type":"swap"}"#,
    )
    .await
}

async fn swap_contract_sizes() -> anyhow::Result<BTreeMap<String, f64>> {
    let response = common::http_client()?
        .get(SWAP_CONTRACT_INFO_URL)
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

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let code = common::str_value(&row, "contract_code");
            let contract_size = common::f64_value(&row, "contract_size");
            (contract_size > 0.0 && !code.is_empty()).then_some((code, contract_size))
        })
        .collect())
}

fn htx_signed_query(
    credential: &Value,
    method: &str,
    host: &str,
    path: &str,
    extra_params: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let mut params = BTreeMap::from([
        (
            "AccessKeyId".to_string(),
            common::str_value(credential, "access_key"),
        ),
        ("SignatureMethod".to_string(), "HmacSHA256".to_string()),
        ("SignatureVersion".to_string(), "2".to_string()),
        (
            "Timestamp".to_string(),
            Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        ),
    ]);
    params.extend(extra_params.clone());
    let payload_query = encode_query(&params);
    let payload = format!("{method}\n{host}\n{path}\n{payload_query}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(common::str_value(credential, "secret_key").as_bytes())?;
    mac.update(payload.as_bytes());
    params.insert(
        "Signature".to_string(),
        BASE64_STANDARD.encode(mac.finalize().into_bytes()),
    );

    Ok(encode_query(&params))
}

fn encode_query(params: &BTreeMap<String, String>) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_component(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn spot_account_id(response: &Value) -> anyhow::Result<String> {
    response
        .get("data")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|row| {
                common::str_value(row, "type") == "spot"
                    && common::str_value(row, "state") == "working"
            })
        })
        .map(|row| {
            row.get("id")
                .and_then(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| value.as_i64().map(|id| id.to_string()))
                })
                .unwrap_or_default()
        })
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("HTX spot account not found"))
}

fn map_spot_position(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "currency").to_uppercase();
    let balance = common::f64_value(&row, "balance");
    if currency.is_empty() || balance <= 0.0 {
        return None;
    }
    let closable_price = if currency == "USDT" { 1.0 } else { 0.0 };

    Some(Position {
        position_id: format!("SPOT/{currency}"),
        product_id: if currency == "USDT" {
            format!("{ID}/SPOT/usdt")
        } else {
            format!("{ID}/SPOT/{}usdt", currency.to_lowercase())
        },
        base_currency: Some(currency.clone()),
        quote_currency: Some("USDT".to_string()),
        direction: None,
        volume: balance,
        free_volume: if common::str_value(&row, "type") == "trade" {
            balance
        } else {
            0.0
        },
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(balance, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
        ..Position::default()
    })
}

fn map_union_swap_asset(row: Value) -> Option<Position> {
    let currency = common::str_value(&row, "currency");
    let volume = common::f64_value(&row, "available");
    if currency.is_empty() || volume == 0.0 {
        return None;
    }
    let closable_price = if matches!(currency.as_str(), "USDT" | "USDC" | "USDD") {
        1.0
    } else {
        0.0
    };

    Some(Position {
        position_id: format!("SWAP-ASSET/{currency}"),
        product_id: format!("{ID}/SWAP-ASSET/{currency}"),
        base_currency: Some(currency.clone()),
        quote_currency: Some(currency.clone()),
        direction: None,
        volume,
        free_volume: common::f64_value(&row, "withdraw_available"),
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some(currency),
        floating_profit: common::f64_value(&row, "profit_unreal"),
        comment: None,
        ..Position::default()
    })
}

fn map_swap_position(row: Value, contract_sizes: &BTreeMap<String, f64>) -> Option<Position> {
    let contract_code = common::str_value(&row, "contract_code");
    let volume = common::f64_value(&row, "volume");
    if contract_code.is_empty() || volume == 0.0 {
        return None;
    }
    let contract_size = *contract_sizes.get(&contract_code)?;
    let closable_price =
        common::f64_value(&row, "last_price").max(common::f64_value(&row, "mark_price"));
    let (base_currency, quote_currency) = htx_contract_currencies(&contract_code);
    let direction = if common::str_value(&row, "direction") == "sell" {
        PositionDirection::Short
    } else {
        PositionDirection::Long
    };
    let signed_size = signed_swap_size(volume, contract_size, &direction);
    let signed_free_size = signed_swap_size(
        common::f64_value(&row, "available"),
        contract_size,
        &direction,
    );

    Some(Position {
        position_id: contract_code.clone(),
        product_id: format!("{ID}/SWAP/{contract_code}"),
        base_currency,
        quote_currency,
        direction: Some(direction),
        size: Some(signed_size.to_string()),
        free_size: Some(signed_free_size.to_string()),
        volume,
        free_volume: common::f64_value(&row, "available"),
        position_price: common::f64_value(&row, "cost_open")
            .max(common::f64_value(&row, "open_avg_price")),
        closable_price,
        notional_value: common::notional_value(signed_size, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: common::f64_value(&row, "profit_unreal"),
        comment: None,
        ..Position::default()
    })
}

fn signed_swap_size(volume: f64, contract_size: f64, direction: &PositionDirection) -> f64 {
    match direction {
        PositionDirection::Short => -volume.abs() * contract_size,
        PositionDirection::Long => volume.abs() * contract_size,
    }
}

fn htx_contract_currencies(contract_code: &str) -> (Option<String>, Option<String>) {
    let mut parts = contract_code.split('-');
    (
        parts.next().map(str::to_string),
        parts.next().map(str::to_string),
    )
}

fn map_trade_fill(row: Value) -> Option<TradeFill> {
    let trade_id = common::text_value(&row, "trade_id");
    let contract = common::str_value(&row, "contract_code");
    let price = common::f64_value(&row, "trade_price");
    let volume = common::f64_value(&row, "trade_volume");
    if trade_id.is_empty() || contract.is_empty() || volume == 0.0 {
        return None;
    }

    Some(TradeFill {
        exchange: ID.to_string(),
        trade_id,
        order_id: Some(common::text_value(&row, "order_id")).filter(|value| !value.is_empty()),
        product_id: format!("{ID}/SWAP/{contract}"),
        direction: htx_fill_direction(
            &common::str_value(&row, "side"),
            &common::str_value(&row, "position_side"),
        ),
        price,
        volume: volume.abs(),
        value: common::f64_value(&row, "trade_turnover"),
        value_currency: Some("USDT".to_string()),
        fee: common::f64_value(&row, "trade_fee"),
        fee_currency: Some(common::str_value(&row, "fee_currency"))
            .filter(|value| !value.is_empty()),
        created_at: Some(common::text_value(&row, "created_time"))
            .filter(|value| !value.is_empty()),
    })
}

fn htx_fill_direction(side: &str, position_side: &str) -> Option<PositionDirection> {
    match (side, position_side) {
        ("buy", _) => Some(PositionDirection::Long),
        ("sell", _) => Some(PositionDirection::Short),
        _ => None,
    }
}

fn map_swap_product(row: Value) -> Product {
    let code = common::str_value(&row, "contract_code");
    let value_scale = common::opt_f64_value(&row, "contract_size");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/SWAP/{code}"),
        name: Some(code),
        quote_currency: Some("USDT".to_string()),
        base_currency: Some(common::str_value(&row, "symbol")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(&row, "price_tick"),
        volume_step: common::normalized_volume_step(Some(1.0), value_scale),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: None,
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(true),
        market_id: Some(ID.to_string()),
        no_interest_rate: Some(false),
        spread: None,
    }
}

fn map_spot_product(row: Value) -> Product {
    let symbol = common::str_value(&row, "sc");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/SPOT/{symbol}"),
        name: Some(symbol),
        quote_currency: Some(common::str_value(&row, "qc")).filter(|value| !value.is_empty()),
        base_currency: Some(common::str_value(&row, "bc")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(&row, "tpp").map(common::pow_step),
        volume_step: common::normalized_volume_step(
            common::opt_f64_value(&row, "tap").map(common::pow_step),
            None,
        ),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: None,
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(false),
        market_id: Some(format!("{ID}/SPOT")),
        no_interest_rate: Some(true),
        spread: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn maps_swap_contract_volume_to_base_size_for_valuation() {
        let contract_sizes = BTreeMap::from([("TAO-USDT".to_string(), 0.001)]);
        let position = map_swap_position(
            json!({
                "contract_code": "TAO-USDT",
                "volume": 9352,
                "available": 9352,
                "direction": "sell",
                "last_price": "203.54",
                "mark_price": "203.5",
                "cost_open": "208.91099016253207",
                "profit_unreal": "50.2295"
            }),
            &contract_sizes,
        )
        .expect("swap position with contract size should be mapped");

        assert_eq!(position.volume, 9352.0);
        assert_eq!(position.free_volume, 9352.0);
        assert_eq!(position.size.as_deref(), Some("-9.352"));
        assert_eq!(position.free_size.as_deref(), Some("-9.352"));
        assert!(matches!(position.direction, Some(PositionDirection::Short)));
        assert_eq!(position.closable_price, 203.54);
        assert_close(position.notional_value, 1903.50608);
    }

    #[test]
    fn skips_swap_position_without_contract_size() {
        let position = map_swap_position(
            json!({
                "contract_code": "TAO-USDT",
                "volume": 9352,
                "direction": "sell"
            }),
            &BTreeMap::new(),
        );

        assert!(position.is_none());
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0000001,
            "expected {actual} to be close to {expected}"
        );
    }
}
