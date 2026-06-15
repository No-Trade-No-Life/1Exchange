use serde_json::{Map, Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::exchanges::ExchangeInfo;

pub fn exchange_info(
    id: &'static str,
    name: &'static str,
    required_fields: &'static [&'static str],
) -> ExchangeInfo {
    ExchangeInfo {
        id,
        name,
        credential_schema: credential_schema(required_fields),
        capabilities: vec!["products", "account", "positions", "orders", "trades"],
    }
}

#[allow(dead_code)]
pub fn not_implemented(exchange: &str, resource: &str) -> anyhow::Error {
    anyhow::anyhow!("{exchange} {resource} adapter is not implemented")
}

pub fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?)
}

pub fn str_value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn text_value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| {
            item.as_str()
                .map(str::to_string)
                .or_else(|| item.as_i64().map(|number| number.to_string()))
                .or_else(|| item.as_u64().map(|number| number.to_string()))
        })
        .unwrap_or_default()
}

pub fn f64_value(value: &Value, key: &str) -> f64 {
    value
        .get(key)
        .and_then(|item| {
            item.as_f64()
                .filter(|value| value.is_finite())
                .or_else(|| parse_f64(item.as_str()?))
        })
        .unwrap_or_default()
}

pub fn opt_f64_value(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|item| {
        item.as_f64()
            .filter(|value| value.is_finite())
            .or_else(|| parse_f64(item.as_str()?))
    })
}

pub fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

pub fn pow_step(decimals: f64) -> f64 {
    10_f64.powf(-decimals)
}

pub fn notional_value(volume: f64, price: f64) -> f64 {
    if price > 0.0 {
        volume.abs() * price
    } else {
        0.0
    }
}

pub fn normalized_volume_step(volume_step: Option<f64>, value_scale: Option<f64>) -> Option<f64> {
    volume_step.map(|step| step * value_scale.unwrap_or(1.0))
}

pub fn now_timestamp_in_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after unix epoch")
        .as_micros() as i64
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parse_f64_rejects_non_finite_values() {
        assert_eq!(parse_f64("1.25"), Some(1.25));
        assert_eq!(parse_f64("NaN"), None);
        assert_eq!(parse_f64("Infinity"), None);
        assert_eq!(parse_f64("-Infinity"), None);
    }

    #[test]
    fn json_number_helpers_reject_non_finite_string_values() {
        let value = json!({ "valid": "2.5", "nan": "NaN" });

        assert_eq!(f64_value(&value, "valid"), 2.5);
        assert_eq!(f64_value(&value, "nan"), 0.0);
        assert_eq!(opt_f64_value(&value, "nan"), None);
    }
}
