use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::collections::BTreeMap;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product},
};

pub const ID: &str = "HTX";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];
const SPOT_BASE_URL: &str = "https://api.huobi.pro";
const SPOT_HOST: &str = "api.huobi.pro";
const SPOT_ACCOUNTS_PATH: &str = "/v1/account/accounts";
const SPOT_BALANCE_PATH_PREFIX: &str = "/v1/account/accounts/";
const SWAP_CONTRACT_INFO_URL: &str = "https://api.hbdm.com/linear-swap-api/v1/swap_contract_info";
const SWAP_HOST: &str = "api.hbdm.com";
const SWAP_ACCOUNT_TYPE_PATH: &str = "/linear-swap-api/v3/swap_unified_account_type";
const SWAP_POSITION_INFO_PATH: &str = "/linear-swap-api/v1/swap_position_info";
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
        let positions = self.list_positions(credential).await?;

        Ok(AccountInfo {
            account_id: format!("{ID}/{}", common::str_value(credential, "access_key")),
            positions,
            orders: Vec::new(),
            timestamp_in_us: common::now_timestamp_in_us(),
        })
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let accounts = htx_get(credential, SPOT_BASE_URL, SPOT_HOST, SPOT_ACCOUNTS_PATH).await?;
        let spot_account_id = spot_account_id(&accounts)?;
        let balance_path = format!("{SPOT_BALANCE_PATH_PREFIX}{spot_account_id}/balance");
        let balance = htx_get(credential, SPOT_BASE_URL, SPOT_HOST, &balance_path).await?;
        let account_type =
            htx_get(credential, SWAP_BASE_URL, SWAP_HOST, SWAP_ACCOUNT_TYPE_PATH).await?;
        let swap = htx_post(
            credential,
            SWAP_BASE_URL,
            SWAP_HOST,
            swap_position_path(&account_type)?,
            r#"{"contract_type":"swap"}"#,
        )
        .await?;
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
            .chain(swap_rows.into_iter().filter_map(map_swap_position))
            .collect())
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }
}

async fn htx_get(
    credential: &Value,
    base_url: &str,
    host: &str,
    path: &str,
) -> anyhow::Result<Value> {
    let query = htx_signed_query(credential, "GET", host, path)?;
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
    let query = htx_signed_query(credential, "POST", host, path)?;
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

fn swap_position_path(account_type: &Value) -> anyhow::Result<&'static str> {
    let account_type = account_type
        .get("data")
        .map(|data| common::f64_value(data, "account_type"))
        .unwrap_or_default();
    if account_type == 2.0 {
        anyhow::bail!("HTX unified contract account is not implemented")
    }

    Ok(SWAP_POSITION_INFO_PATH)
}

fn htx_signed_query(
    credential: &Value,
    method: &str,
    host: &str,
    path: &str,
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

    Some(Position {
        position_id: format!("SPOT/{currency}"),
        product_id: if currency == "USDT" {
            format!("{ID}/SPOT/usdt")
        } else {
            format!("{ID}/SPOT/{}usdt", currency.to_lowercase())
        },
        direction: None,
        volume: balance,
        free_volume: if common::str_value(&row, "type") == "trade" {
            balance
        } else {
            0.0
        },
        position_price: 0.0,
        closable_price: if currency == "USDT" { 1.0 } else { 0.0 },
        floating_profit: 0.0,
        comment: None,
    })
}

fn map_swap_position(row: Value) -> Option<Position> {
    let contract_code = common::str_value(&row, "contract_code");
    let volume = common::f64_value(&row, "volume");
    if contract_code.is_empty() || volume == 0.0 {
        return None;
    }

    Some(Position {
        position_id: contract_code.clone(),
        product_id: format!("{ID}/SWAP/{contract_code}"),
        direction: Some(if common::str_value(&row, "direction") == "sell" {
            PositionDirection::Short
        } else {
            PositionDirection::Long
        }),
        volume,
        free_volume: common::f64_value(&row, "available"),
        position_price: common::f64_value(&row, "cost_open"),
        closable_price: common::f64_value(&row, "last_price"),
        floating_profit: common::f64_value(&row, "profit_unreal"),
        comment: None,
    })
}

fn map_swap_product(row: Value) -> Product {
    let code = common::str_value(&row, "contract_code");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/SWAP/{code}"),
        name: Some(code),
        quote_currency: Some("USDT".to_string()),
        base_currency: Some(common::str_value(&row, "symbol")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(&row, "price_tick"),
        volume_step: Some(1.0),
        value_scale: common::opt_f64_value(&row, "contract_size"),
        value_scale_unit: None,
        margin_rate: None,
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
    let symbol = common::str_value(&row, "sc");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/SPOT/{symbol}"),
        name: Some(symbol),
        quote_currency: Some(common::str_value(&row, "qc")).filter(|value| !value.is_empty()),
        base_currency: Some(common::str_value(&row, "bc")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(&row, "tpp").map(common::pow_step),
        volume_step: common::opt_f64_value(&row, "tap").map(common::pow_step),
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
