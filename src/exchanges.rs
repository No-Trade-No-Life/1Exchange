use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};

use crate::models::{AccountInfo, Order, Position, Product};

#[derive(Clone, Debug, Serialize)]
pub struct ExchangeInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub credential_schema: Value,
    pub capabilities: Vec<&'static str>,
}

#[async_trait]
#[allow(dead_code)]
pub trait ExchangeAdapter: Send + Sync {
    fn info(&self) -> ExchangeInfo;
    async fn list_products(&self, credential: &Value) -> anyhow::Result<Vec<Product>>;
    async fn get_account(&self, credential: &Value) -> anyhow::Result<AccountInfo>;
    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>>;
    async fn list_orders(&self, credential: &Value) -> anyhow::Result<Vec<Order>>;
}

struct StaticExchangeAdapter {
    info: ExchangeInfo,
}

#[async_trait]
impl ExchangeAdapter for StaticExchangeAdapter {
    fn info(&self) -> ExchangeInfo {
        self.info.clone()
    }

    async fn list_products(&self, _credential: &Value) -> anyhow::Result<Vec<Product>> {
        anyhow::bail!("{} product adapter is not implemented", self.info.id)
    }

    async fn get_account(&self, _credential: &Value) -> anyhow::Result<AccountInfo> {
        anyhow::bail!("{} account adapter is not implemented", self.info.id)
    }

    async fn list_positions(&self, _credential: &Value) -> anyhow::Result<Vec<Position>> {
        anyhow::bail!("{} position adapter is not implemented", self.info.id)
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        anyhow::bail!("{} order adapter is not implemented", self.info.id)
    }
}

pub fn list_exchanges() -> Vec<ExchangeInfo> {
    registered_adapters()
        .into_iter()
        .map(|adapter| adapter.info())
        .collect()
}

pub fn is_supported_exchange(exchange: &str) -> bool {
    list_exchanges().iter().any(|item| item.id == exchange)
}

fn api_key_exchange(id: &'static str, name: &'static str) -> ExchangeInfo {
    ExchangeInfo {
        id,
        name,
        credential_schema: json!({
            "type": "object",
            "required": ["access_key", "secret_key"],
            "properties": {
                "access_key": { "type": "string" },
                "secret_key": { "type": "string" }
            }
        }),
        capabilities: vec!["products", "account", "positions", "orders"],
    }
}

fn registered_adapters() -> Vec<Box<dyn ExchangeAdapter>> {
    vec![
        Box::new(StaticExchangeAdapter {
            info: api_key_exchange("BINANCE", "Binance"),
        }),
        Box::new(StaticExchangeAdapter {
            info: api_key_exchange("GATE", "Gate.io"),
        }),
        Box::new(StaticExchangeAdapter {
            info: api_key_exchange("ASTER", "Aster"),
        }),
        Box::new(StaticExchangeAdapter {
            info: api_key_exchange("HYPERLIQUID", "Hyperliquid"),
        }),
    ]
}
