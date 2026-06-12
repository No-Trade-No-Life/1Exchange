use async_trait::async_trait;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "GATE";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];
const FUTURES_CONTRACTS_URL: &str = "https://api.gateio.ws/api/v4/futures/usdt/contracts";
const SPOT_PAIRS_URL: &str = "https://api.gateio.ws/api/v4/spot/currency_pairs";

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
