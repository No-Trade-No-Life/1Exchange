use async_trait::async_trait;
use serde_json::Value;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "GATE";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Gate.io", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        Err(common::not_implemented(ID, "product"))
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
