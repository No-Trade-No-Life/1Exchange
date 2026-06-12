use async_trait::async_trait;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "HTX";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];
const SWAP_CONTRACT_INFO_URL: &str = "https://api.hbdm.com/linear-swap-api/v1/swap_contract_info";
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
