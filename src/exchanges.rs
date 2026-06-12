use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Map, Value, json};

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
    credential_required_fields(exchange).is_some()
}

pub fn credential_required_fields(exchange: &str) -> Option<&'static [&'static str]> {
    match exchange {
        "BINANCE" => Some(&["access_key", "secret_key"]),
        "OKX" => Some(&["access_key", "secret_key", "passphrase"]),
        "HTX" => Some(&["access_key", "secret_key"]),
        "GATE" => Some(&["access_key", "secret_key"]),
        "BITGET" => Some(&["access_key", "secret_key", "passphrase"]),
        "HYPERLIQUID" => Some(&["private_key", "address"]),
        "ASTER" => Some(&["address", "secret_key", "api_key"]),
        _ => None,
    }
}

fn exchange(
    id: &'static str,
    name: &'static str,
    required_fields: &'static [&'static str],
) -> ExchangeInfo {
    ExchangeInfo {
        id,
        name,
        credential_schema: credential_schema(required_fields),
        capabilities: vec!["products", "account", "positions", "orders"],
    }
}

fn credential_schema(required_fields: &[&str]) -> Value {
    let mut properties = Map::new();
    for field in required_fields {
        properties.insert((*field).to_string(), json!({ "type": "string" }));
    }

    json!({
        "type": "object",
        "required": required_fields,
        "properties": properties
    })
}

fn registered_adapters() -> Vec<Box<dyn ExchangeAdapter>> {
    vec![
        Box::new(StaticExchangeAdapter {
            info: exchange("BINANCE", "Binance", &["access_key", "secret_key"]),
        }),
        Box::new(StaticExchangeAdapter {
            info: exchange("OKX", "OKX", &["access_key", "secret_key", "passphrase"]),
        }),
        Box::new(StaticExchangeAdapter {
            info: exchange("HTX", "HTX", &["access_key", "secret_key"]),
        }),
        Box::new(StaticExchangeAdapter {
            info: exchange("GATE", "Gate.io", &["access_key", "secret_key"]),
        }),
        Box::new(StaticExchangeAdapter {
            info: exchange(
                "BITGET",
                "Bitget",
                &["access_key", "secret_key", "passphrase"],
            ),
        }),
        Box::new(StaticExchangeAdapter {
            info: exchange("HYPERLIQUID", "HyperLiquid", &["private_key", "address"]),
        }),
        Box::new(StaticExchangeAdapter {
            info: exchange("ASTER", "Aster", &["address", "secret_key", "api_key"]),
        }),
    ]
}
