use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

const SPOT_EXCHANGE_INFO_URL: &str = "https://api.binance.com/api/v3/exchangeInfo";
const SPOT_ACCOUNT_URL: &str = "https://api.binance.com/api/v3/account";
const USD_M_FUTURES_EXCHANGE_INFO_URL: &str = "https://fapi.binance.com/fapi/v1/exchangeInfo";

pub const ID: &str = "BINANCE";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];

pub struct Adapter;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeInfoResponse {
    symbols: Vec<BinanceSymbol>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceSymbol {
    symbol: String,
    status: String,
    base_asset: String,
    quote_asset: String,
    filters: Vec<BinanceFilter>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceFilter {
    filter_type: String,
    tick_size: Option<String>,
    step_size: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotAccountResponse {
    balances: Vec<SpotBalance>,
}

#[derive(Deserialize)]
struct SpotBalance {
    asset: String,
    free: String,
    locked: String,
}

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Binance", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let spot = fetch_products(&client, SPOT_EXCHANGE_INFO_URL, "SPOT", false).await?;
        let futures = fetch_products(
            &client,
            USD_M_FUTURES_EXCHANGE_INFO_URL,
            "USDT-FUTURES",
            true,
        )
        .await?;

        Ok(spot.into_iter().chain(futures).collect())
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
        let account = fetch_spot_account(credential).await?;

        Ok(account
            .balances
            .into_iter()
            .filter_map(map_spot_balance)
            .collect())
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }
}

async fn fetch_spot_account(credential: &Value) -> anyhow::Result<SpotAccountResponse> {
    let access_key = common::str_value(credential, "access_key");
    let secret_key = common::str_value(credential, "secret_key");
    let query = signed_query(&secret_key)?;
    let client = common::http_client()?;

    Ok(client
        .get(format!("{SPOT_ACCOUNT_URL}?{query}"))
        .header("X-MBX-APIKEY", access_key)
        .send()
        .await?
        .error_for_status()?
        .json::<SpotAccountResponse>()
        .await?)
}

fn signed_query(secret_key: &str) -> anyhow::Result<String> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let query = format!("recvWindow=5000&timestamp={timestamp}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())?;
    mac.update(query.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    Ok(format!("{query}&signature={signature}"))
}

fn map_spot_balance(balance: SpotBalance) -> Option<Position> {
    let free = balance.free.parse::<f64>().ok()?;
    let locked = balance.locked.parse::<f64>().ok()?;
    let volume = free + locked;
    if volume <= 0.0 {
        return None;
    }

    Some(Position {
        position_id: format!("SPOT/{}", balance.asset),
        product_id: spot_asset_product_id(&balance.asset),
        direction: None,
        volume,
        free_volume: free,
        position_price: 0.0,
        closable_price: if balance.asset == "USDT" { 1.0 } else { 0.0 },
        floating_profit: 0.0,
        comment: None,
    })
}

fn spot_asset_product_id(asset: &str) -> String {
    if asset == "USDT" {
        format!("{ID}/SPOT/USDT")
    } else {
        format!("{ID}/SPOT/{asset}USDT")
    }
}

async fn fetch_products(
    client: &reqwest::Client,
    url: &str,
    market: &str,
    allow_short: bool,
) -> anyhow::Result<Vec<Product>> {
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<ExchangeInfoResponse>()
        .await?;

    Ok(response
        .symbols
        .into_iter()
        .filter(|symbol| symbol.status == "TRADING")
        .map(|symbol| map_product(symbol, market, allow_short))
        .collect())
}

fn map_product(symbol: BinanceSymbol, market: &str, allow_short: bool) -> Product {
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/{market}/{}", symbol.symbol),
        name: Some(symbol.symbol),
        quote_currency: Some(symbol.quote_asset),
        base_currency: Some(symbol.base_asset),
        price_step: filter_number(&symbol.filters, "PRICE_FILTER", "tickSize"),
        volume_step: filter_number(&symbol.filters, "LOT_SIZE", "stepSize"),
        value_scale: None,
        value_scale_unit: None,
        margin_rate: None,
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(allow_short),
        spread: None,
    }
}

fn filter_number(filters: &[BinanceFilter], filter_type: &str, field: &str) -> Option<f64> {
    filters
        .iter()
        .find(|filter| filter.filter_type == filter_type)
        .and_then(|filter| match field {
            "tickSize" => filter.tick_size.as_deref(),
            "stepSize" => filter.step_size.as_deref(),
            _ => None,
        })
        .and_then(|value| value.parse().ok())
}
