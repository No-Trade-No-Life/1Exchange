use async_trait::async_trait;
use ethers_core::{
    abi::{Token, encode},
    types::{Address, H256, U256},
    utils::keccak256,
};
use ethers_signers::{LocalWallet, Signer};
use serde_json::Value;
use std::str::FromStr;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product},
};

pub const ID: &str = "ASTER";
pub const REQUIRED_FIELDS: &[&str] = &["address", "signer", "private_key"];
const PERP_BASE_URL: &str = "https://fapi.asterdex.com";
const PERP_EXCHANGE_INFO_URL: &str = "https://fapi.asterdex.com/fapi/v3/exchangeInfo";
const SPOT_EXCHANGE_INFO_URL: &str = "https://sapi.asterdex.com/api/v3/exchangeInfo";
const PERP_ACCOUNT_PATH: &str = "/fapi/v3/accountWithJoinMargin";
const SPOT_PRICES_URL: &str = "https://sapi.asterdex.com/api/v3/ticker/price";

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
        Ok(common::account_id(
            ID,
            common::str_value(credential, "address").to_lowercase(),
        ))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let perp_account = signed_get(credential, PERP_BASE_URL, PERP_ACCOUNT_PATH, true).await?;
        let prices = common::http_client()?
            .get(SPOT_PRICES_URL)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        let mut positions = Vec::new();
        positions.extend(
            perp_account
                .get("assets")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|row| map_perp_asset(row, &prices)),
        );
        positions.extend(
            perp_account
                .get("positions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(map_perp_position),
        );

        Ok(positions)
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }
}

async fn signed_get(
    credential: &Value,
    base_url: &str,
    path: &str,
    include_user: bool,
) -> anyhow::Result<Value> {
    let nonce = common::now_timestamp_in_us();
    let user = common::str_value(credential, "address");
    let signer = common::str_value(credential, "signer");
    let private_key = common::str_value(credential, "private_key");
    ensure_signer_matches_private_key(&signer, &private_key)?;
    let params = if include_user {
        format!(
            "nonce={}&user={}&signer={}",
            nonce,
            urlencoding::encode(&user),
            urlencoding::encode(&signer)
        )
    } else {
        format!("nonce={}&signer={}", nonce, urlencoding::encode(&signer))
    };
    let signature = sign_v3_message(&private_key, &params)?;
    let url = format!("{base_url}{path}?{params}&signature={signature}");
    let response = common::http_client()?
        .get(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "1Exchange/0.1")
        .send()
        .await?;
    let status = response.status();
    let response_text = response.text().await?;

    if !status.is_success() {
        anyhow::bail!("ASTER V3 {path} request failed with {status}: {response_text}");
    }

    let response = serde_json::from_str::<Value>(&response_text)?;

    if response.get("code").and_then(Value::as_i64).is_some() {
        anyhow::bail!("ASTER V3 {path} request failed: {response}");
    }

    Ok(response)
}

fn ensure_signer_matches_private_key(signer: &str, private_key: &str) -> anyhow::Result<()> {
    let wallet = LocalWallet::from_str(private_key)?;
    if format!("{:?}", wallet.address()).eq_ignore_ascii_case(signer) {
        return Ok(());
    }

    anyhow::bail!("ASTER V3 signer does not match private_key");
}

fn sign_v3_message(private_key: &str, message: &str) -> anyhow::Result<String> {
    let digest = eip712_message_digest(message)?;
    let wallet = LocalWallet::from_str(private_key)?;
    let signature = wallet.sign_hash(H256::from(digest))?.to_string();

    Ok(if signature.starts_with("0x") {
        signature
    } else {
        format!("0x{signature}")
    })
}

fn eip712_message_digest(message: &str) -> anyhow::Result<[u8; 32]> {
    let domain_type_hash = keccak256(
        "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
            .as_bytes(),
    );
    let message_type_hash = keccak256("Message(string msg)".as_bytes());
    let verifying_contract = Address::zero();
    let domain_separator = keccak256(encode(&[
        Token::FixedBytes(domain_type_hash.to_vec()),
        Token::FixedBytes(keccak256("AsterSignTransaction".as_bytes()).to_vec()),
        Token::FixedBytes(keccak256("1".as_bytes()).to_vec()),
        Token::Uint(U256::from(1666_u64)),
        Token::Address(verifying_contract),
    ]));
    let message_hash = keccak256(encode(&[
        Token::FixedBytes(message_type_hash.to_vec()),
        Token::FixedBytes(keccak256(message.as_bytes()).to_vec()),
    ]));
    let mut bytes = Vec::with_capacity(66);
    bytes.extend_from_slice(b"\x19\x01");
    bytes.extend_from_slice(&domain_separator);
    bytes.extend_from_slice(&message_hash);

    Ok(keccak256(bytes))
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
        volume_step: common::normalized_volume_step(
            filter_number(row, "LOT_SIZE", "stepSize")
                .or_else(|| common::opt_f64_value(row, "quantityPrecision").map(common::pow_step)),
            None,
        ),
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

fn map_perp_asset(row: Value, prices: &Value) -> Option<Position> {
    let asset = common::str_value(&row, "asset");
    let volume = common::f64_value(&row, "walletBalance");
    if asset.is_empty() || volume == 0.0 {
        return None;
    }
    let closable_price = asset_price(&asset, prices);

    Some(Position {
        position_id: format!("{asset}/ASSET"),
        product_id: format!("{ID}/PERP-ASSET/{asset}"),
        direction: None,
        volume,
        free_volume: common::f64_value(&row, "availableBalance"),
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: common::f64_value(&row, "unrealizedProfit"),
        comment: None,
    })
}

fn map_perp_position(row: Value) -> Option<Position> {
    let symbol = common::str_value(&row, "symbol");
    let amount = common::f64_value(&row, "positionAmt");
    if symbol.is_empty() || amount == 0.0 {
        return None;
    }
    let side = common::str_value(&row, "positionSide");
    let notional = common::f64_value(&row, "notional");
    let closable_price = common::f64_value(&row, "markPrice");
    let resolved_closable_price = if closable_price > 0.0 {
        closable_price
    } else if amount != 0.0 {
        (notional / amount).abs()
    } else {
        0.0
    };

    Some(Position {
        position_id: symbol.clone(),
        product_id: format!("{ID}/PERP/{symbol}"),
        direction: Some(if side == "SHORT" || amount < 0.0 {
            PositionDirection::Short
        } else {
            PositionDirection::Long
        }),
        volume: amount.abs(),
        free_volume: amount.abs(),
        position_price: common::f64_value(&row, "entryPrice"),
        closable_price: resolved_closable_price,
        notional_value: notional.abs(),
        notional_currency: Some("USDT".to_string()),
        floating_profit: common::f64_value(&row, "unRealizedProfit"),
        comment: None,
    })
}

fn asset_price(asset: &str, prices: &Value) -> f64 {
    if asset == "USDT" {
        return 1.0;
    }
    let symbol = format!("{asset}USDT");
    prices
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| common::str_value(row, "symbol") == symbol)
        })
        .map(|row| common::f64_value(row, "price"))
        .unwrap_or_default()
}
