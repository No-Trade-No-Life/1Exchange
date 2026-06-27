use std::str::FromStr;

use anyhow::Context;
use async_trait::async_trait;
use ethers_core::{
    abi::{Token, encode},
    types::{Address, U256},
    utils::{format_units, keccak256},
};
use serde_json::{Value, json};

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, Product},
};

pub const ID: &str = "EARNBYAI";
pub const REQUIRED_FIELDS: &[&str] = &["address"];

const BSC_RPC_URL: &str = "https://bsc-dataseed.binance.org/";
const SEA_USD_TOKEN: &str = "0xd44c72d47F029e1545D1689A440AAA38cF3db71A";
const SEA_USD_VAULT: &str = "0x0cedAaA38A5b7789aE5C8aCa1ee7886F83Fd23F3";
const PRODUCT_ID: &str = "EARNBYAI/BSC/seaUSD-USDC";
const MARKET_ID: &str = "EARNBYAI/BSC";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Earnby.AI", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        Ok(vec![Product {
            datasource_id: ID.to_string(),
            product_id: PRODUCT_ID.to_string(),
            name: Some("seaUSD".to_string()),
            quote_currency: Some("USDC".to_string()),
            base_currency: Some("seaUSD".to_string()),
            price_step: Some(0.000001),
            volume_step: Some(0.000001),
            value_scale: Some(1.0),
            value_scale_unit: None,
            margin_rate: Some(1.0),
            value_based_cost: None,
            volume_based_cost: None,
            max_position: None,
            max_volume: None,
            allow_long: Some(true),
            allow_short: Some(false),
            market_id: Some(MARKET_ID.to_string()),
            no_interest_rate: Some(true),
            spread: None,
        }])
    }

    async fn get_account(&self, credential: &Value) -> anyhow::Result<AccountInfo> {
        let account_id = self.get_account_id(credential).await?;
        let positions = self.list_positions(credential).await?;

        Ok(AccountInfo {
            account_id,
            positions,
            orders: Vec::new(),
            timestamp_in_us: common::now_timestamp_in_us(),
        })
    }

    async fn get_account_id(&self, credential: &Value) -> anyhow::Result<String> {
        let address = credential_address(credential)?;
        Ok(common::account_id(ID, format!("{address:#x}")))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let address = credential_address(credential)?;
        let token = Address::from_str(SEA_USD_TOKEN).context("invalid seaUSD token address")?;
        let vault = Address::from_str(SEA_USD_VAULT).context("invalid seaUSD vault address")?;
        let client = common::http_client()?;

        let balance_raw = eth_call_u256(
            &client,
            token,
            "balanceOf(address)",
            encode(&[Token::Address(address)]),
        )
        .await?;
        let decimals = eth_call_u256(&client, token, "decimals()", Vec::new())
            .await?
            .as_u32();
        let nav_per_share_raw =
            eth_call_u256(&client, vault, "getNetAssetValuePerShare()", Vec::new()).await?;

        let balance = format_u256(balance_raw, decimals)?;
        let nav_per_share = format_u256(nav_per_share_raw, 18)?;
        let value_raw = sea_usd_value_raw(balance_raw, nav_per_share_raw, decimals);
        let value = format_u256(value_raw, 18)?;

        Ok(vec![Position {
            position_id: "seaUSD".to_string(),
            product_id: PRODUCT_ID.to_string(),
            base_currency: Some("seaUSD".to_string()),
            quote_currency: Some("USDC".to_string()),
            direction: None,
            volume: balance,
            free_volume: balance,
            position_price: 0.0,
            closable_price: nav_per_share,
            notional_value: value,
            notional_currency: Some("USDC".to_string()),
            valuation: value,
            floating_profit: 0.0,
            comment: Some("BSC seaUSD share token".to_string()),
            ..Position::default()
        }])
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }
}

async fn eth_call_u256(
    client: &reqwest::Client,
    to: Address,
    signature: &str,
    args: Vec<u8>,
) -> anyhow::Result<U256> {
    let data = eth_call_data(signature, args);
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            {
                "to": format!("{to:#x}"),
                "data": data,
            },
            "latest"
        ]
    });
    let response = client
        .post(BSC_RPC_URL)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(error) = response.get("error") {
        anyhow::bail!("BSC eth_call failed for {signature}: {error}");
    }
    let result = response
        .get("result")
        .and_then(Value::as_str)
        .context("BSC eth_call response missing result")?;

    parse_u256_hex(result)
}

fn eth_call_data(signature: &str, args: Vec<u8>) -> String {
    let mut data = keccak256(signature.as_bytes())[..4].to_vec();
    data.extend(args);
    format!("0x{}", hex::encode(data))
}

fn parse_u256_hex(value: &str) -> anyhow::Result<U256> {
    let hex = value.strip_prefix("0x").unwrap_or(value);
    U256::from_str_radix(hex, 16).context("invalid uint256 hex value")
}

fn format_u256(value: U256, decimals: u32) -> anyhow::Result<f64> {
    format_units(value, decimals as usize)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .context("uint256 value is outside finite f64 range")
}

fn sea_usd_value_raw(balance_raw: U256, nav_per_share_raw: U256, share_decimals: u32) -> U256 {
    balance_raw * nav_per_share_raw / U256::exp10(share_decimals as usize)
}

fn credential_address(credential: &Value) -> anyhow::Result<Address> {
    let address = common::str_value(credential, "address");
    if address.is_empty() {
        anyhow::bail!("missing credential field: address");
    }
    Address::from_str(&address).context("invalid credential field: address")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_sea_usd_value_from_share_nav() {
        let balance = U256::exp10(18) * U256::from(25_u64);
        let nav = U256::exp10(18) * U256::from(12_u64) / U256::from(10_u64);

        assert_eq!(
            sea_usd_value_raw(balance, nav, 18),
            U256::exp10(18) * U256::from(30_u64)
        );
    }

    #[test]
    fn builds_eth_call_data_with_selector_and_encoded_args() {
        let address = Address::from_str("0x775EaD0bc1eaf95d8B4Ee7Ad0f0b16E3c36F71C9").unwrap();
        let data = eth_call_data("balanceOf(address)", encode(&[Token::Address(address)]));

        assert!(data.starts_with("0x70a08231"));
        assert_eq!(data.len(), 74);
    }
}
