use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountInfo {
    /// Cross-market account identity, usually EXCHANGE/UID.
    pub account_id: String,
    /// Positions are the account's atomic assets. Product specifications are resolved by product_id.
    pub positions: Vec<Position>,
    #[serde(default)]
    pub orders: Vec<Order>,
    #[serde(default)]
    pub timestamp_in_us: i64,
}

impl AccountInfo {
    pub fn normalized(mut self) -> Self {
        let account_id = self.account_id.clone();
        self.positions = self
            .positions
            .into_iter()
            .map(|position| position.normalized(&account_id))
            .collect();

        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub position_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datasource_id: Option<String>,
    /// Keeps the exchange-native product id. The matching Product describes its contract or asset specs.
    pub product_id: String,
    /// 1Earn/Yuants composition owner. None means this row belongs to the enclosing AccountInfo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub base_currency: Option<String>,
    pub quote_currency: Option<String>,
    pub direction: Option<PositionDirection>,
    /// Yuants-normalized signed size. Long assets/positions are positive; shorts are negative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Yuants-normalized signed tradable size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_size: Option<String>,
    pub volume: f64,
    pub free_volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidation_price: Option<String>,
    pub position_price: f64,
    pub closable_price: f64,
    pub notional_value: f64,
    pub notional_currency: Option<String>,
    /// 1Earn/Yuants field name for the position valuation, in account/notional currency.
    #[serde(default)]
    pub valuation: f64,
    pub floating_profit: f64,
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_interval: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_scheduled_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interest_to_settle: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realized_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_opened_volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_closed_volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
}

impl Position {
    pub fn normalized(mut self, account_id: &str) -> Self {
        if self.account_id.is_none() {
            self.account_id = Some(account_id.to_string());
        }
        if self.datasource_id.is_none() {
            self.datasource_id = self.product_id.split('/').next().map(str::to_string);
        }
        if self.valuation == 0.0 && self.notional_value != 0.0 {
            self.valuation = self.notional_value;
        }
        if self.notional_value == 0.0 && self.valuation != 0.0 {
            self.notional_value = self.valuation;
        }
        if self.size.is_none() {
            self.size = Some(format_position_size(self.signed_volume()));
        }
        if self.free_size.is_none() {
            self.free_size = Some(format_position_size(self.signed_free_volume()));
        }

        self
    }

    pub fn scale(&mut self, coefficient: f64) {
        let signed_volume = self.signed_volume() * coefficient;
        let signed_free_volume = self.signed_free_volume() * coefficient;
        self.volume = signed_volume.abs();
        self.free_volume = signed_free_volume.abs();
        self.notional_value *= coefficient;
        self.valuation *= coefficient;
        self.floating_profit *= coefficient;
        self.margin = self.margin.map(|value| value * coefficient);
        self.realized_pnl = self.realized_pnl.map(|value| value * coefficient);
        self.interest_to_settle = self.interest_to_settle.map(|value| value * coefficient);
        self.size = Some(format_position_size(signed_volume));
        self.free_size = Some(format_position_size(signed_free_volume));
    }

    fn signed_volume(&self) -> f64 {
        match self.direction {
            Some(PositionDirection::Short) => -self.volume.abs(),
            _ => self.volume,
        }
    }

    fn signed_free_volume(&self) -> f64 {
        match self.direction {
            Some(PositionDirection::Short) => -self.free_volume.abs(),
            _ => self.free_volume,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            position_id: String::new(),
            datasource_id: None,
            product_id: String::new(),
            account_id: None,
            base_currency: None,
            quote_currency: None,
            direction: None,
            size: None,
            free_size: None,
            volume: 0.0,
            free_volume: 0.0,
            liquidation_price: None,
            position_price: 0.0,
            closable_price: 0.0,
            notional_value: 0.0,
            notional_currency: None,
            valuation: 0.0,
            floating_profit: 0.0,
            comment: None,
            settlement_interval: None,
            settlement_scheduled_at: None,
            interest_to_settle: None,
            margin: None,
            realized_pnl: None,
            total_opened_volume: None,
            total_closed_volume: None,
            created_at: None,
            updated_at: None,
        }
    }
}

fn format_position_size(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_position_with_yuants_fields() {
        let position = Position {
            position_id: "BTC".to_string(),
            product_id: "BINANCE/SPOT/BTCUSDT".to_string(),
            base_currency: Some("BTC".to_string()),
            volume: 2.0,
            free_volume: 1.5,
            notional_value: 100_000.0,
            ..Position::default()
        }
        .normalized("BINANCE/42");

        assert_eq!(position.account_id.as_deref(), Some("BINANCE/42"));
        assert_eq!(position.datasource_id.as_deref(), Some("BINANCE"));
        assert_eq!(position.size.as_deref(), Some("2"));
        assert_eq!(position.free_size.as_deref(), Some("1.5"));
        assert_eq!(position.valuation, 100_000.0);
    }

    #[test]
    fn scales_virtual_position_with_signed_size_and_non_negative_volume() {
        let mut position = Position {
            direction: Some(PositionDirection::Long),
            volume: 3.0,
            free_volume: 2.0,
            notional_value: 30.0,
            valuation: 30.0,
            floating_profit: 1.0,
            interest_to_settle: Some(0.5),
            ..Position::default()
        };

        position.scale(-2.0);

        assert_eq!(position.volume, 6.0);
        assert_eq!(position.free_volume, 4.0);
        assert_eq!(position.size.as_deref(), Some("-6"));
        assert_eq!(position.free_size.as_deref(), Some("-4"));
        assert_eq!(position.valuation, -60.0);
        assert_eq!(position.notional_value, -60.0);
        assert_eq!(position.floating_profit, -2.0);
        assert_eq!(position.interest_to_settle, Some(-1.0));
    }
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
pub struct TradeFill {
    pub exchange: String,
    pub trade_id: String,
    pub order_id: Option<String>,
    pub product_id: String,
    pub direction: Option<PositionDirection>,
    pub price: f64,
    pub volume: f64,
    pub value: f64,
    pub value_currency: Option<String>,
    pub fee: f64,
    pub fee_currency: Option<String>,
    pub created_at: Option<String>,
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
    /// Minimum order quantity after applying exchange contract multipliers.
    pub volume_step: Option<f64>,
    /// Normalized to 1.0; adapter contract multipliers are folded into volume_step.
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
