mod aster;
mod binance;
mod bitget;
mod common;
mod gate;
mod htx;
mod hyperliquid;
mod okx;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::models::{AccountInfo, Order, Position, Product, TradeFill};

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
    async fn list_products(&self) -> anyhow::Result<Vec<Product>>;
    async fn get_account(&self, credential: &Value) -> anyhow::Result<AccountInfo>;
    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>>;
    async fn list_orders(&self, credential: &Value) -> anyhow::Result<Vec<Order>>;
    async fn list_trades(&self, _credential: &Value) -> anyhow::Result<Vec<TradeFill>> {
        Err(anyhow::anyhow!("trade history adapter is not implemented"))
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
        binance::ID => Some(binance::REQUIRED_FIELDS),
        okx::ID => Some(okx::REQUIRED_FIELDS),
        htx::ID => Some(htx::REQUIRED_FIELDS),
        gate::ID => Some(gate::REQUIRED_FIELDS),
        bitget::ID => Some(bitget::REQUIRED_FIELDS),
        hyperliquid::ID => Some(hyperliquid::REQUIRED_FIELDS),
        aster::ID => Some(aster::REQUIRED_FIELDS),
        _ => None,
    }
}

pub fn adapter(exchange: &str) -> Option<Box<dyn ExchangeAdapter>> {
    registered_adapters()
        .into_iter()
        .find(|adapter| adapter.info().id == exchange)
}

fn registered_adapters() -> Vec<Box<dyn ExchangeAdapter>> {
    vec![
        Box::new(binance::Adapter),
        Box::new(okx::Adapter),
        Box::new(htx::Adapter),
        Box::new(gate::Adapter),
        Box::new(bitget::Adapter),
        Box::new(hyperliquid::Adapter),
        Box::new(aster::Adapter),
    ]
}
