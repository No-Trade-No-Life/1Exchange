use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

const SPOT_EXCHANGE_INFO_URL: &str = "https://api.binance.com/api/v3/exchangeInfo";
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

    async fn get_account(&self, _credential: &Value) -> anyhow::Result<AccountInfo> {
        Err(common::not_implemented(ID, "account"))
    }

    async fn list_positions(&self, _credential: &Value) -> anyhow::Result<Vec<Position>> {
        Err(common::not_implemented(ID, "position"))
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
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
