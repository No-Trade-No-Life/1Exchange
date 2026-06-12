use async_trait::async_trait;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "ASTER";
pub const REQUIRED_FIELDS: &[&str] = &["address", "secret_key", "api_key"];
const PERP_EXCHANGE_INFO_URL: &str = "https://fapi.asterdex.com/fapi/v1/exchangeInfo";
const SPOT_EXCHANGE_INFO_URL: &str = "https://sapi.asterdex.com/api/v1/exchangeInfo";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Aster", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let perp = fetch_products(&client, PERP_EXCHANGE_INFO_URL, "PERP", true).await?;
        let spot = fetch_products(&client, SPOT_EXCHANGE_INFO_URL, "SPOT", false).await?;

        Ok(perp.into_iter().chain(spot).collect())
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
        .json::<Value>()
        .await?;
    let rows = response
        .get("symbols")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(rows
        .into_iter()
        .filter(|row| common::str_value(row, "status") == "TRADING")
        .map(|row| map_product(&row, market, allow_short))
        .collect())
}

fn map_product(row: &Value, market: &str, allow_short: bool) -> Product {
    let symbol = common::str_value(row, "symbol");
    let base = common::str_value(row, "baseAsset");
    let quote = common::str_value(row, "quoteAsset");

    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/{market}/{symbol}"),
        name: Some(format!("{base}/{quote} {market}")),
        quote_currency: Some(quote).filter(|value| !value.is_empty()),
        base_currency: Some(base).filter(|value| !value.is_empty()),
        price_step: filter_number(row, "PRICE_FILTER", "tickSize")
            .or_else(|| common::opt_f64_value(row, "pricePrecision").map(common::pow_step)),
        volume_step: filter_number(row, "LOT_SIZE", "stepSize")
            .or_else(|| common::opt_f64_value(row, "quantityPrecision").map(common::pow_step)),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: Some(if allow_short { 0.1 } else { 1.0 }),
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(allow_short),
        spread: None,
    }
}

fn filter_number(row: &Value, filter_type: &str, field: &str) -> Option<f64> {
    row.get("filters")
        .and_then(Value::as_array)?
        .iter()
        .find(|filter| common::str_value(filter, "filterType") == filter_type)
        .and_then(|filter| common::opt_f64_value(filter, field))
}
