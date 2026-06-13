use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountInfo {
    pub account_id: String,
    /// Positions are the account's atomic assets. Product specifications are resolved by product_id.
    pub positions: Vec<Position>,
    pub orders: Vec<Order>,
    pub timestamp_in_us: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub position_id: String,
    /// Keeps the exchange-native product id. The matching Product describes its contract or asset specs.
    pub product_id: String,
    pub direction: Option<PositionDirection>,
    pub volume: f64,
    pub free_volume: f64,
    pub position_price: f64,
    pub closable_price: f64,
    pub notional_value: f64,
    pub notional_currency: Option<String>,
    pub floating_profit: f64,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionDirection {
    Long,
    Short,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    pub order_id: String,
    pub account_id: String,
    pub product_id: String,
    pub direction: Option<PositionDirection>,
    pub volume: f64,
    pub traded_volume: f64,
    pub price: Option<f64>,
    pub status: OrderStatus,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    Pending,
    Submitted,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub datasource_id: String,
    /// Exchange-native product id. Do not normalize it before adapter-specific handling.
    pub product_id: String,
    pub name: Option<String>,
    pub quote_currency: Option<String>,
    pub base_currency: Option<String>,
    pub price_step: Option<f64>,
    pub volume_step: Option<f64>,
    pub value_scale: Option<f64>,
    pub value_scale_unit: Option<String>,
    pub margin_rate: Option<f64>,
    pub value_based_cost: Option<f64>,
    pub volume_based_cost: Option<f64>,
    pub max_position: Option<f64>,
    pub max_volume: Option<f64>,
    pub allow_long: Option<bool>,
    pub allow_short: Option<bool>,
    pub spread: Option<f64>,
}
