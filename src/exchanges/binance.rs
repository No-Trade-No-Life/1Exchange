use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product},
};

const SPOT_EXCHANGE_INFO_URL: &str = "https://api.binance.com/api/v3/exchangeInfo";
const SPOT_ACCOUNT_URL: &str = "https://api.binance.com/api/v3/account";
const SPOT_TIME_URL: &str = "https://api.binance.com/api/v3/time";
const USD_M_FUTURES_EXCHANGE_INFO_URL: &str = "https://fapi.binance.com/fapi/v1/exchangeInfo";
const USD_M_FUTURES_ACCOUNT_URL: &str = "https://fapi.binance.com/fapi/v2/account";
const USD_M_FUTURES_TIME_URL: &str = "https://fapi.binance.com/fapi/v1/time";

pub const ID: &str = "BINANCE";
pub const REQUIRED_FIELDS: &[&str] = &["access_key", "secret_key"];

pub struct Adapter;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeInfoResponse {
    symbols: Vec<BinanceSymbol>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceSymbol {
    symbol: String,
    status: String,
    base_asset: String,
    quote_asset: String,
    filters: Vec<BinanceFilter>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceFilter {
    filter_type: String,
    tick_size: Option<String>,
    step_size: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotAccountResponse {
    uid: u64,
    balances: Vec<SpotBalance>,
}

#[derive(Deserialize)]
struct SpotBalance {
    asset: String,
    free: String,
    locked: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FuturesAccountResponse {
    assets: Vec<FuturesAsset>,
    positions: Vec<FuturesPosition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FuturesAsset {
    asset: String,
    wallet_balance: String,
    available_balance: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FuturesPosition {
    symbol: String,
    position_amt: String,
    entry_price: String,
    unrealized_profit: String,
    position_side: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceTimeResponse {
    server_time: u128,
}

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "Binance", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let spot = fetch_products(&client, SPOT_EXCHANGE_INFO_URL, "SPOT", false).await?;
        let futures = fetch_products(
            &client,
            USD_M_FUTURES_EXCHANGE_INFO_URL,
            "USDT-FUTURES",
            true,
        )
        .await?;

        Ok(spot.into_iter().chain(futures).collect())
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
        let spot_account = fetch_spot_account(credential).await?;

        Ok(common::account_id(ID, spot_account.uid.to_string()))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let spot_account = fetch_spot_account(credential).await?;

        let mut positions = spot_account
            .balances
            .into_iter()
            .filter_map(map_spot_balance)
            .collect::<Vec<_>>();
        match fetch_futures_account(credential).await {
            Ok(futures_account) => {
                positions.extend(
                    futures_account
                        .assets
                        .into_iter()
                        .filter_map(map_futures_asset),
                );
                positions.extend(
                    futures_account
                        .positions
                        .into_iter()
                        .filter_map(map_futures_position),
                );
            }
            Err(error) if is_http_auth_error(&error) => {}
            Err(error) => return Err(error),
        }

        Ok(positions)
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }
}

async fn fetch_spot_account(credential: &Value) -> anyhow::Result<SpotAccountResponse> {
    fetch_signed(credential, SPOT_ACCOUNT_URL, SPOT_TIME_URL).await
}

async fn fetch_futures_account(credential: &Value) -> anyhow::Result<FuturesAccountResponse> {
    fetch_signed(
        credential,
        USD_M_FUTURES_ACCOUNT_URL,
        USD_M_FUTURES_TIME_URL,
    )
    .await
}

async fn fetch_signed<T>(credential: &Value, url: &str, time_url: &str) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let access_key = common::str_value(credential, "access_key");
    let secret_key = common::str_value(credential, "secret_key");
    let client = common::http_client()?;
    let timestamp = fetch_server_timestamp(&client, time_url).await?;
    let query = signed_query(&secret_key, timestamp)?;

    Ok(client
        .get(format!("{url}?{query}"))
        .header("X-MBX-APIKEY", access_key)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?)
}

async fn fetch_server_timestamp(client: &reqwest::Client, time_url: &str) -> anyhow::Result<u128> {
    Ok(client
        .get(time_url)
        .send()
        .await?
        .error_for_status()?
        .json::<BinanceTimeResponse>()
        .await?
        .server_time)
}

fn signed_query(secret_key: &str, timestamp: u128) -> anyhow::Result<String> {
    let query = format!("recvWindow=5000&timestamp={timestamp}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())?;
    mac.update(query.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    Ok(format!("{query}&signature={signature}"))
}

fn is_http_auth_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .and_then(reqwest::Error::status)
        .is_some_and(|status| {
            status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        })
}

fn map_spot_balance(balance: SpotBalance) -> Option<Position> {
    let free = common::parse_f64(&balance.free)?;
    let locked = common::parse_f64(&balance.locked)?;
    let volume = free + locked;
    if volume <= 0.0 {
        return None;
    }
    let closable_price = if balance.asset == "USDT" { 1.0 } else { 0.0 };

    Some(Position {
        position_id: format!("SPOT/{}", balance.asset),
        product_id: spot_asset_product_id(&balance.asset),
        base_currency: Some(balance.asset.clone()),
        quote_currency: Some("USDT".to_string()),
        direction: None,
        volume,
        free_volume: free,
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
        ..Position::default()
    })
}

fn spot_asset_product_id(asset: &str) -> String {
    if asset == "USDT" {
        format!("{ID}/SPOT/USDT")
    } else {
        format!("{ID}/SPOT/{asset}USDT")
    }
}

fn map_futures_asset(asset: FuturesAsset) -> Option<Position> {
    let volume = common::parse_f64(&asset.wallet_balance)?;
    if volume <= 0.0 {
        return None;
    }
    let closable_price = if asset.asset == "USDT" { 1.0 } else { 0.0 };

    Some(Position {
        position_id: format!("USDT-FUTURES/{}", asset.asset),
        product_id: format!("{ID}/USDT-FUTURES/{}", asset.asset),
        base_currency: Some(asset.asset.clone()),
        quote_currency: Some("USDT".to_string()),
        direction: None,
        volume,
        free_volume: common::parse_f64(&asset.available_balance)?,
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit: 0.0,
        comment: None,
        ..Position::default()
    })
}

fn map_futures_position(position: FuturesPosition) -> Option<Position> {
    let signed_volume = common::parse_f64(&position.position_amt)?;
    if signed_volume == 0.0 {
        return None;
    }
    let entry_price = common::parse_f64(&position.entry_price)?;
    let floating_profit = common::parse_f64(&position.unrealized_profit)?;
    let closable_price = entry_price + floating_profit / signed_volume;

    Some(Position {
        position_id: format!(
            "USDT-FUTURES/{}/{}",
            position.symbol, position.position_side
        ),
        product_id: format!("{ID}/USDT-FUTURES/{}", position.symbol),
        base_currency: Some(position.symbol.trim_end_matches("USDT").to_string()),
        quote_currency: Some("USDT".to_string()),
        direction: Some(futures_direction(signed_volume, &position.position_side)),
        volume: signed_volume.abs(),
        free_volume: signed_volume.abs(),
        position_price: entry_price,
        closable_price,
        notional_value: common::notional_value(signed_volume, closable_price),
        notional_currency: Some("USDT".to_string()),
        floating_profit,
        comment: None,
        ..Position::default()
    })
}

fn futures_direction(signed_volume: f64, position_side: &str) -> PositionDirection {
    if position_side == "SHORT" || signed_volume < 0.0 {
        PositionDirection::Short
    } else {
        PositionDirection::Long
    }
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
        .json::<ExchangeInfoResponse>()
        .await?;

    Ok(response
        .symbols
        .into_iter()
        .filter(|symbol| symbol.status == "TRADING")
        .map(|symbol| map_product(symbol, market, allow_short))
        .collect())
}

fn map_product(symbol: BinanceSymbol, market: &str, allow_short: bool) -> Product {
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/{market}/{}", symbol.symbol),
        name: Some(symbol.symbol),
        quote_currency: Some(symbol.quote_asset),
        base_currency: Some(symbol.base_asset),
        price_step: filter_number(&symbol.filters, "PRICE_FILTER", "tickSize"),
        volume_step: common::normalized_volume_step(
            filter_number(&symbol.filters, "LOT_SIZE", "stepSize"),
            None,
        ),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: None,
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(allow_short),
        market_id: Some(format!("{ID}/{market}")),
        no_interest_rate: Some(market == "SPOT"),
        spread: None,
    }
}

fn filter_number(filters: &[BinanceFilter], filter_type: &str, field: &str) -> Option<f64> {
    filters
        .iter()
        .find(|filter| filter.filter_type == filter_type)
        .and_then(|filter| match field {
            "tickSize" => filter.tick_size.as_deref(),
            "stepSize" => filter.step_size.as_deref(),
            _ => None,
        })
        .and_then(common::parse_f64)
}
