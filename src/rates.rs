use std::collections::{HashMap, VecDeque};

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CurrencyRateEdge {
    pub base_currency: String,
    pub quote_currency: String,
    pub rate: f64,
    pub source: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CurrencyRateSnapshot {
    pub target_currency: String,
    pub edges: Vec<CurrencyRateEdge>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CurrencyConversion {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: Option<f64>,
}

pub fn snapshot(target_currency: &str) -> CurrencyRateSnapshot {
    CurrencyRateSnapshot {
        target_currency: target_currency.to_string(),
        edges: stablecoin_edges(),
    }
}

pub fn convert_rate(edges: &[CurrencyRateEdge], from: &str, to: &str) -> Option<f64> {
    if from == to {
        return Some(1.0);
    }

    let graph = adjacency(edges);
    let mut queue = VecDeque::from([(from.to_string(), 1.0)]);
    let mut seen = vec![from.to_string()];

    while let Some((currency, rate)) = queue.pop_front() {
        for (next, edge_rate) in graph.get(&currency).into_iter().flatten() {
            if seen.contains(next) {
                continue;
            }
            let next_rate = rate * edge_rate;
            if next == to {
                return Some(next_rate);
            }
            seen.push(next.clone());
            queue.push_back((next.clone(), next_rate));
        }
    }

    None
}

pub fn conversion(edges: &[CurrencyRateEdge], from: &str, to: &str) -> CurrencyConversion {
    CurrencyConversion {
        from_currency: from.to_string(),
        to_currency: to.to_string(),
        rate: convert_rate(edges, from, to),
    }
}

fn stablecoin_edges() -> Vec<CurrencyRateEdge> {
    ["USD", "USDT", "USDC", "USDD"]
        .into_iter()
        .flat_map(|base| {
            ["USD", "USDT", "USDC", "USDD"]
                .into_iter()
                .filter(move |quote| *quote != base)
                .map(move |quote| rate_edge(base, quote, 1.0, "stablecoin-parity"))
        })
        .collect()
}

fn rate_edge(base: &str, quote: &str, rate: f64, source: &str) -> CurrencyRateEdge {
    CurrencyRateEdge {
        base_currency: base.to_string(),
        quote_currency: quote.to_string(),
        rate,
        source: source.to_string(),
        updated_at: "static".to_string(),
    }
}

fn adjacency(edges: &[CurrencyRateEdge]) -> HashMap<String, Vec<(String, f64)>> {
    let mut graph: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for edge in edges {
        if edge.rate.is_finite() && edge.rate > 0.0 {
            graph
                .entry(edge.base_currency.clone())
                .or_default()
                .push((edge.quote_currency.clone(), edge.rate));
        }
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_through_rate_graph() {
        let edges = vec![
            rate_edge("OKSOL", "USDC", 100.0, "test"),
            rate_edge("USDC", "USD", 1.0, "test"),
        ];

        assert_eq!(convert_rate(&edges, "OKSOL", "USD"), Some(100.0));
    }

    #[test]
    fn returns_none_for_unconnected_currency() {
        let edges = stablecoin_edges();

        assert_eq!(convert_rate(&edges, "OKSOL", "USD"), None);
    }
}
