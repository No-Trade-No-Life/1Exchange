use serde_json::{Map, Value, json};

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
        capabilities: vec!["products", "account", "positions", "orders"],
    }
}

pub fn not_implemented(exchange: &str, resource: &str) -> anyhow::Error {
    anyhow::anyhow!("{exchange} {resource} adapter is not implemented")
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
