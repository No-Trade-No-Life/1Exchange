use async_trait::async_trait;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "BITGET";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key", "passphrase"];
const INSTRUMENTS_URL: &str = "https://api.bitget.com/api/v3/market/instruments";

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
        volume_step: common::opt_f64_value(row, "quantityMultiplier")
            .or_else(|| quantity_precision.map(common::pow_step)),
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
        spread: None,
    }
}
