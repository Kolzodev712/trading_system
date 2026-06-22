//! DeepBook V3 indexer HTTP client (read path).

use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use smol_str::SmolStr;
use std::str::FromStr;
use std::time::Duration;
use tracing::debug;

use trading_core::venue::dex::DexError;

use super::types::DeepBookPoolMeta;
use trading_core::venue::dex::types::DexOpenOrder;

fn rpc_err(msg: impl Into<SmolStr>) -> DexError {
    DexError::Rpc(msg.into())
}

/// Raw orderbook JSON from indexer.
#[derive(Debug, Clone, Deserialize)]
pub struct IndexerOrderbook {
    pub timestamp: String,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

/// Parsed orderbook levels in human units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderbookSnapshot {
    pub timestamp_ms: u64,
    pub bids: Vec<(Decimal, Decimal)>,
    pub asks: Vec<(Decimal, Decimal)>,
}

/// Trade record from indexer `GET /trades/:pool`.
#[derive(Debug, Clone, Deserialize)]
pub struct IndexerTrade {
    pub trade_id: Option<String>,
    pub timestamp: u64,
    #[serde(rename = "type")]
    pub side: String,
    pub price: f64,
    pub base_volume: f64,
}

/// Open order from indexer `GET /orders/:pool/:balance_manager_id`.
#[derive(Debug, Clone, Deserialize)]
pub struct IndexerOpenOrder {
    pub order_id: String,
    #[serde(default)]
    pub balance_manager_id: Option<String>,
    #[serde(rename = "type")]
    pub side: String,
    pub current_status: String,
    pub price: f64,
    pub original_quantity: f64,
    #[serde(default)]
    pub filled_quantity: f64,
    pub remaining_quantity: f64,
}

impl IndexerOpenOrder {
    pub fn to_dex_open_order(&self, pool_name: &str) -> Result<DexOpenOrder, DexError> {
        use smol_str::SmolStr;
        Ok(DexOpenOrder {
            order_id: SmolStr::new(&self.order_id),
            pool_name: SmolStr::new(pool_name),
            side: SmolStr::new(self.side.to_ascii_lowercase()),
            price: decimal_from_f64(self.price, "price")?,
            quantity: decimal_from_f64(self.remaining_quantity, "remaining_quantity")?,
            filled_quantity: decimal_from_f64(self.filled_quantity, "filled_quantity")?,
            status: SmolStr::new(&self.current_status),
        })
    }
}

fn decimal_from_f64(value: f64, field: &str) -> Result<rust_decimal::Decimal, DexError> {
    rust_decimal::Decimal::try_from(value)
        .map_err(|e| rpc_err(format!("invalid {field} {value}: {e}")))
}

/// HTTP client for DeepBook indexer endpoints.
#[derive(Debug, Clone)]
pub struct DeepBookIndexerClient {
    base_url: SmolStr,
    http: Client,
}

impl DeepBookIndexerClient {
    pub fn new(base_url: impl Into<SmolStr>) -> Result<Self, DexError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| rpc_err(format!("indexer HTTP client: {e}")))?;
        Ok(Self {
            base_url: base_url.into(),
            http,
        })
    }

    fn url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}{path}")
    }

    pub async fn get_pools(&self) -> Result<Vec<DeepBookPoolMeta>, DexError> {
        let url = self.url("/get_pools");
        debug!("DeepBook indexer GET {url}");
        let pools = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| rpc_err(format!("get_pools request: {e}")))?
            .error_for_status()
            .map_err(|e| rpc_err(format!("get_pools HTTP: {e}")))?
            .json::<Vec<DeepBookPoolMeta>>()
            .await
            .map_err(|e| rpc_err(format!("get_pools parse: {e}")))?;
        Ok(pools)
    }

    pub async fn get_pools_by_name(
        &self,
        pool_names: &[SmolStr],
    ) -> Result<Vec<DeepBookPoolMeta>, DexError> {
        let all = self.get_pools().await?;
        let wanted: std::collections::HashSet<&str> =
            pool_names.iter().map(|s| s.as_str()).collect();
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|p| wanted.contains(p.pool_name.as_str()))
            .collect();
        if filtered.len() != pool_names.len() {
            let found: Vec<_> = filtered.iter().map(|p| p.pool_name.as_str()).collect();
            return Err(DexError::InvalidConfig(
                format!(
                    "indexer missing pools: requested {:?}, found {:?}",
                    pool_names, found
                )
                .into(),
            ));
        }
        Ok(filtered)
    }

    pub async fn get_orderbook(
        &self,
        pool_name: &str,
        depth: u32,
    ) -> Result<OrderbookSnapshot, DexError> {
        let url = self.url(&format!("/orderbook/{pool_name}?level=2&depth={depth}"));
        debug!("DeepBook indexer GET {url}");
        let raw = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| rpc_err(format!("orderbook request: {e}")))?
            .error_for_status()
            .map_err(|e| rpc_err(format!("orderbook HTTP: {e}")))?
            .json::<IndexerOrderbook>()
            .await
            .map_err(|e| rpc_err(format!("orderbook parse: {e}")))?;

        Ok(OrderbookSnapshot {
            timestamp_ms: raw.timestamp.parse().unwrap_or(0),
            bids: parse_levels(&raw.bids)?,
            asks: parse_levels(&raw.asks)?,
        })
    }

    pub async fn get_trades(
        &self,
        pool_name: &str,
        limit: u32,
    ) -> Result<Vec<IndexerTrade>, DexError> {
        let url = self.url(&format!("/trades/{pool_name}?limit={limit}"));
        debug!("DeepBook indexer GET {url}");
        self.http
            .get(&url)
            .send()
            .await
            .map_err(|e| rpc_err(format!("trades request: {e}")))?
            .error_for_status()
            .map_err(|e| rpc_err(format!("trades HTTP: {e}")))?
            .json::<Vec<IndexerTrade>>()
            .await
            .map_err(|e| rpc_err(format!("trades parse: {e}")))
    }

    pub async fn get_open_orders(
        &self,
        pool_name: &str,
        balance_manager_id: &str,
    ) -> Result<Vec<IndexerOpenOrder>, DexError> {
        let url = self.url(&format!(
            "/orders/{pool_name}/{balance_manager_id}?status=Placed"
        ));
        debug!("DeepBook indexer GET {url}");
        self.http
            .get(&url)
            .send()
            .await
            .map_err(|e| rpc_err(format!("orders request: {e}")))?
            .error_for_status()
            .map_err(|e| rpc_err(format!("orders HTTP: {e}")))?
            .json::<Vec<IndexerOpenOrder>>()
            .await
            .map_err(|e| rpc_err(format!("orders parse: {e}")))
    }
}

fn parse_levels(levels: &[[String; 2]]) -> Result<Vec<(Decimal, Decimal)>, DexError> {
    levels
        .iter()
        .map(|[price, qty]| {
            Ok((
                Decimal::from_str(price)
                    .map_err(|e| rpc_err(format!("invalid price {price}: {e}")))?,
                Decimal::from_str(qty).map_err(|e| rpc_err(format!("invalid qty {qty}: {e}")))?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn converts_indexer_open_order() {
        let raw = IndexerOpenOrder {
            order_id: "123".into(),
            balance_manager_id: None,
            side: "buy".into(),
            current_status: "Placed".into(),
            price: 3.715,
            original_quantity: 100.0,
            filled_quantity: 0.0,
            remaining_quantity: 100.0,
        };
        let order = raw.to_dex_open_order("SUI_DBUSDC").unwrap();
        assert_eq!(order.pool_name.as_str(), "SUI_DBUSDC");
        assert_eq!(order.side.as_str(), "buy");
        assert_eq!(order.price, Decimal::from_str("3.715").unwrap());
        assert_eq!(order.quantity, Decimal::from_str("100").unwrap());
        assert_eq!(order.status.as_str(), "Placed");
    }
}
