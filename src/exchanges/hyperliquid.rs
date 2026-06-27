use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{
    exchanges::{ExchangeAdapter, ExchangeInfo, common},
    models::{AccountInfo, Order, Position, PositionDirection, Product, TradeFill},
};

pub const ID: &str = "HYPERLIQUID";
pub const REQUIRED_FIELDS: &[&str] = &["address"];
const INFO_URL: &str = "https://api.hyperliquid.xyz/info";

pub struct Adapter;

#[async_trait]
impl ExchangeAdapter for Adapter {
    fn info(&self) -> ExchangeInfo {
        common::exchange_info(ID, "HyperLiquid", REQUIRED_FIELDS)
    }

    async fn list_products(&self) -> anyhow::Result<Vec<Product>> {
        let client = common::http_client()?;
        let spot_meta = post_info(&client, json!({ "type": "spotMeta" })).await?;
        let perp_meta = post_info(&client, json!({ "type": "meta" })).await?;

        let spot_tokens = spot_meta
            .get("tokens")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let perp_universe = perp_meta
            .get("universe")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(spot_tokens
            .into_iter()
            .map(map_spot_product)
            .chain(perp_universe.into_iter().map(map_perp_product))
            .collect())
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
            credential_address(credential)?.to_lowercase(),
        ))
    }

    async fn list_positions(&self, credential: &Value) -> anyhow::Result<Vec<Position>> {
        let address = credential_address(credential)?;
        let client = common::http_client()?;
        let perp = post_info(
            &client,
            json!({ "type": "clearinghouseState", "user": address }),
        )
        .await?;
        let spot = post_info(
            &client,
            json!({ "type": "spotClearinghouseState", "user": address }),
        )
        .await?;
        let mids = post_info(&client, json!({ "type": "allMids" })).await?;

        let perp_rows = perp
            .get("assetPositions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let spot_rows = spot
            .get("balances")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut positions: Vec<Position> = perp_rows
            .into_iter()
            .filter_map(map_perp_position)
            .collect();
        positions.extend(
            spot_rows
                .into_iter()
                .filter_map(|row| map_spot_position(row, &mids)),
        );
        Ok(positions)
    }

    async fn list_orders(&self, _credential: &Value) -> anyhow::Result<Vec<Order>> {
        Err(common::not_implemented(ID, "order"))
    }

    async fn list_trades(&self, credential: &Value) -> anyhow::Result<Vec<TradeFill>> {
        let address = credential_address(credential)?;
        let client = common::http_client()?;
        let response = post_info(&client, json!({ "type": "userFills", "user": address })).await?;
        let rows = response
            .get("fills")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| response.as_array().cloned().unwrap_or_default());

        Ok(rows.into_iter().filter_map(map_fill).collect())
    }
}

async fn post_info(client: &reqwest::Client, body: Value) -> anyhow::Result<Value> {
    Ok(client
        .post(INFO_URL)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?)
}

fn credential_address(credential: &Value) -> anyhow::Result<String> {
    let address = common::str_value(credential, "address");
    if address.is_empty() {
        anyhow::bail!("missing credential field: address");
    }
    Ok(address)
}

fn map_spot_product(row: Value) -> Product {
    let name = common::str_value(&row, "name");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/SPOT/{name}-USDC"),
        name: Some(name.clone()),
        quote_currency: Some("USDC".to_string()),
        base_currency: Some(name),
        price_step: Some(0.01),
        volume_step: common::normalized_volume_step(
            common::opt_f64_value(&row, "szDecimals").map(common::pow_step),
            None,
        ),
        value_scale: Some(1.0),
        value_scale_unit: None,
        margin_rate: Some(1.0),
        value_based_cost: None,
        volume_based_cost: None,
        max_position: None,
        max_volume: None,
        allow_long: Some(true),
        allow_short: Some(false),
        market_id: Some(format!("{ID}/SPOT")),
        no_interest_rate: Some(true),
        spread: None,
    }
}

fn map_perp_product(row: Value) -> Product {
    let name = common::str_value(&row, "name");
    let leverage = common::f64_value(&row, "maxLeverage");
    Product {
        datasource_id: ID.to_string(),
        product_id: format!("{ID}/PERPETUAL/{name}-USD"),
        name: Some(name.clone()),
        quote_currency: Some("USD".to_string()),
        base_currency: Some(name),
        price_step: Some(0.01),
        volume_step: common::normalized_volume_step(
            common::opt_f64_value(&row, "szDecimals").map(common::pow_step),
            None,
        ),
        value_scale: Some(1.0),
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
        market_id: Some(format!("{ID}/PERPETUAL")),
        no_interest_rate: Some(false),
        spread: None,
    }
}

fn map_perp_position(row: Value) -> Option<Position> {
    let position = row.get("position")?;
    let coin = common::str_value(position, "coin");
    let size = common::f64_value(position, "szi");
    if size == 0.0 {
        return None;
    }
    let position_value = common::f64_value(position, "positionValue");
    let closable_price = if size == 0.0 {
        0.0
    } else {
        (position_value / size).abs()
    };

    Some(Position {
        position_id: format!("{coin}-USD"),
        product_id: format!("{ID}/PERPETUAL/{coin}-USD"),
        base_currency: Some(coin),
        quote_currency: Some("USD".to_string()),
        direction: Some(if size > 0.0 {
            PositionDirection::Long
        } else {
            PositionDirection::Short
        }),
        volume: size.abs(),
        free_volume: size.abs(),
        position_price: common::f64_value(position, "entryPx"),
        closable_price,
        notional_value: position_value.abs(),
        notional_currency: Some("USD".to_string()),
        floating_profit: common::f64_value(position, "unrealizedPnl"),
        comment: None,
        ..Position::default()
    })
}

fn map_spot_position(row: Value, mids: &Value) -> Option<Position> {
    let coin = common::str_value(&row, "coin");
    let total = common::f64_value(&row, "total");
    if total <= 0.0 {
        return None;
    }
    let hold = common::f64_value(&row, "hold");
    let closable_price = if coin == "USDC" {
        1.0
    } else {
        mids.get(&coin)
            .and_then(Value::as_str)
            .and_then(common::parse_f64)
            .unwrap_or_default()
    };

    Some(Position {
        position_id: coin.clone(),
        product_id: format!("{ID}/SPOT/{coin}-USDC"),
        base_currency: Some(coin),
        quote_currency: Some("USDC".to_string()),
        direction: None,
        volume: total,
        free_volume: total - hold,
        position_price: 0.0,
        closable_price,
        notional_value: common::notional_value(total, closable_price),
        notional_currency: Some("USDC".to_string()),
        floating_profit: 0.0,
        comment: None,
        ..Position::default()
    })
}

fn map_fill(row: Value) -> Option<TradeFill> {
    let coin = common::str_value(&row, "coin");
    let trade_id = common::text_value(&row, "tid");
    let price = common::f64_value(&row, "px");
    let volume = common::f64_value(&row, "sz");
    if coin.is_empty() || trade_id.is_empty() || volume == 0.0 {
        return None;
    }

    Some(TradeFill {
        exchange: ID.to_string(),
        trade_id,
        order_id: Some(common::text_value(&row, "oid")).filter(|value| !value.is_empty()),
        product_id: format!("{ID}/PERPETUAL/{coin}-USD"),
        direction: hyperliquid_fill_direction(&common::str_value(&row, "side")),
        price,
        volume: volume.abs(),
        value: common::notional_value(volume, price),
        value_currency: Some("USD".to_string()),
        fee: common::f64_value(&row, "fee"),
        fee_currency: Some(common::str_value(&row, "feeToken")).filter(|value| !value.is_empty()),
        created_at: Some(common::text_value(&row, "time")).filter(|value| !value.is_empty()),
    })
}

fn hyperliquid_fill_direction(side: &str) -> Option<PositionDirection> {
    match side {
        "B" | "buy" => Some(PositionDirection::Long),
        "A" | "sell" => Some(PositionDirection::Short),
        _ => None,
    }
}
