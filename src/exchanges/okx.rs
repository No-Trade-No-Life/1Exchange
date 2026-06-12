use async_trait::async_trait;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "OKX";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key", "passphrase"];
const INSTRUMENTS_URL: &str = "https://www.okx.com/api/v5/public/instruments";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "OKX", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let mut products = Vec::new();
        for inst_type in ["SPOT", "MARGIN", "SWAP"] {
            let response = client
                .get(INSTRUMENTS_URL)
                .query(&[("instType", inst_type)])
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
            products.extend(rows.into_iter().map(|row| map_product(inst_type, &row)));
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

fn map_product(inst_type: &str, row: &Value) -> Product {
    let inst_id = common::str_value(row, "instId");
    let lever = common::opt_f64_value(row, "lever").unwrap_or(1.0);

    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/{inst_type}/{inst_id}"),
        name: Some(inst_id),
        quote_currency: Some(common::str_value(row, "quoteCcy")).filter(|value| !value.is_empty()),
        base_currency: Some(common::str_value(row, "baseCcy")).filter(|value| !value.is_empty()),
        price_step: common::opt_f64_value(row, "tickSz"),
        volume_step: common::opt_f64_value(row, "lotSz"),
        value_scale: common::opt_f64_value(row, "ctVal"),
        value_scale_unit: Some(common::str_value(row, "ctValCcy"))
            .filter(|value| !value.is_empty()),
        margin_rate: if inst_type == "SWAP" && lever > 0.0 {
            Some(1.0 / lever)
        } else {
            None
        },
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(inst_type != "SPOT"),
        spread: None,
    }
}
