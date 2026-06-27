use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const OKX_TICKER_URL: &str = "https://www.okx.com/api/v5/market/ticker";
const LIVE_OKX_INSTRUMENTS: &[&str] = &["OKSOL-USDT", "ETH-USDT"];

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

pub async fn live_snapshot(target_currency: &str) -> CurrencyRateSnapshot {
    let mut snapshot = snapshot(target_currency);
    match live_okx_edges().await {
        Ok(edges) => snapshot.edges.extend(edges),
        // RECOVERY: Public market data is optional for the rates endpoint; stablecoin
        // parity still gives the GUI a useful USD total while operators can retry.
        Err(error) => eprintln!("failed to fetch live currency rates: {error:#}"),
    }
    snapshot
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

async fn live_okx_edges() -> anyhow::Result<Vec<CurrencyRateEdge>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut edges = Vec::new();
    for inst_id in LIVE_OKX_INSTRUMENTS {
        edges.push(fetch_okx_edge(&client, inst_id).await?);
    }
    Ok(edges)
}

async fn fetch_okx_edge(
    client: &reqwest::Client,
    inst_id: &str,
) -> anyhow::Result<CurrencyRateEdge> {
    let response = client
        .get(OKX_TICKER_URL)
        .query(&[("instId", inst_id)])
        .send()
        .await?
        .error_for_status()?
        .json::<OkxTickerResponse>()
        .await?;

    okx_edge_from_response(inst_id, response)
}

fn okx_edge_from_response(
    expected_inst_id: &str,
    response: OkxTickerResponse,
) -> anyhow::Result<CurrencyRateEdge> {
    let ticker = response
        .data
        .into_iter()
        .next()
        .context("OKX ticker response did not include data")?;
    let rate = parse_live_rate(&ticker.last)?;
    let (base, quote) = expected_inst_id
        .split_once('-')
        .context("OKX instrument id must contain base and quote")?;

    Ok(CurrencyRateEdge {
        base_currency: base.to_string(),
        quote_currency: quote.to_string(),
        rate,
        source: format!("okx:{}", ticker.inst_id),
        updated_at: ticker.ts,
    })
}

fn parse_live_rate(value: &str) -> anyhow::Result<f64> {
    let rate = value
        .parse::<f64>()
        .context("OKX ticker last price was not numeric")?;
    anyhow::ensure!(
        rate.is_finite() && rate > 0.0,
        "OKX ticker last price must be positive and finite"
    );
    Ok(rate)
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

#[derive(Debug, Deserialize)]
struct OkxTickerResponse {
    data: Vec<OkxTicker>,
}

#[derive(Debug, Deserialize)]
struct OkxTicker {
    #[serde(rename = "instId")]
    inst_id: String,
    last: String,
    ts: String,
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

    #[test]
    fn maps_okx_ticker_to_rate_edge() {
        let response = OkxTickerResponse {
            data: vec![OkxTicker {
                inst_id: "OKSOL-USDT".to_string(),
                last: "71.16".to_string(),
                ts: "1782600000000".to_string(),
            }],
        };

        let edge = okx_edge_from_response("OKSOL-USDT", response).unwrap();

        assert_eq!(edge.base_currency, "OKSOL");
        assert_eq!(edge.quote_currency, "USDT");
        assert_eq!(edge.rate, 71.16);
        assert_eq!(edge.source, "okx:OKSOL-USDT");
        assert_eq!(edge.updated_at, "1782600000000");
    }
}
