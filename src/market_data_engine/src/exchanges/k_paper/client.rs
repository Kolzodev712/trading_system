//! k-paper market data client.
//! Connects to k-paper WebSocket; handles only orderbook and public_trade (ignores execution/balance/position/error).

use std::collections::HashSet;

use crate::messages::WSMessageHandler;
use async_trait::async_trait;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};
use trading_core::{
    error::EngineError,
    types::common::{Exchange, Pair},
    venue::cex::traits::MarketDataClient,
};

use super::config::{to_market_ws_url, PaperConfig};
use super::map::{orderbook_to_structured, parse_instrument_id, public_trade_to_structured};
use super::messages::{KPaperOrderbook, KPaperPublicTrade};

#[derive(Debug)]
pub struct PaperClient {
    config: PaperConfig,
    allowed_pairs: HashSet<Pair>,
    message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
}

impl PaperClient {
    pub fn new(
        config: PaperConfig,
        message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
    ) -> Self {
        let allowed_pairs: HashSet<Pair> = config.pairs.iter().cloned().collect();
        if allowed_pairs.is_empty() {
            warn!(
                "k-paper market data: no pairs configured; all instruments from the feed will be forwarded"
            );
        } else {
            let listed: Vec<String> = allowed_pairs.iter().map(|p| p.to_string()).collect();
            info!(
                "k-paper market data: forwarding {} configured pair(s): {}",
                listed.len(),
                listed.join(", ")
            );
        }
        Self {
            config,
            allowed_pairs,
            message_handler,
        }
    }

    fn is_allowed(&self, pair: &Pair) -> bool {
        self.allowed_pairs.is_empty() || self.allowed_pairs.contains(pair)
    }

    async fn run_loop(&self) -> Result<(), EngineError> {
        let handler = match &self.message_handler {
            Some(h) => h,
            None => return Err(EngineError::NoMessageHandler),
        };
        let url = to_market_ws_url(&self.config.url);
        info!("k-paper market data: connecting to {}", url);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("k-paper connect: {}", e)))?;
        info!("k-paper market data: connected to {}", url);
        let (mut _write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            let text = match msg {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) => {
                    info!("k-paper market data: server closed");
                    break;
                }
                Err(e) => {
                    error!("k-paper market data: WebSocket error: {}", e);
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "WebSocket: {}",
                        e
                    )));
                }
                _ => continue,
            };
            let v: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    warn!("k-paper market data: invalid JSON: {}", e);
                    continue;
                }
            };
            let typ = v.get("type").and_then(|t| t.as_str());
            match typ {
                Some("orderbook") => {
                    if let Ok(ob) = serde_json::from_value::<KPaperOrderbook>(v) {
                        let Some(pair) = parse_instrument_id(ob.instrument_id.as_str()) else {
                            continue;
                        };
                        if !self.is_allowed(&pair) {
                            continue;
                        }
                        if let Some(book) = orderbook_to_structured(&ob) {
                            if let Err(e) = handler.handle_orderbook(book).await {
                                warn!("k-paper market data: handle_orderbook error: {}", e);
                            }
                        }
                    }
                }
                Some("public_trade") => {
                    if let Ok(t) = serde_json::from_value::<KPaperPublicTrade>(v) {
                        let Some(pair) = parse_instrument_id(t.instrument_id.as_str()) else {
                            continue;
                        };
                        if !self.is_allowed(&pair) {
                            continue;
                        }
                        if let Some(trade) = public_trade_to_structured(&t) {
                            if let Err(e) = handler.handle_trade(trade).await {
                                warn!("k-paper market data: handle_trade error: {}", e);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MarketDataClient for PaperClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Paper
    }

    async fn orderbooks(&mut self) -> Result<(), EngineError> {
        loop {
            match self.run_loop().await {
                Ok(()) => {}
                Err(e) => {
                    error!("k-paper market data loop error: {}", e);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    async fn trades(&mut self) -> Result<(), EngineError> {
        self.orderbooks().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn btc_usdt() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn eth_usdt() -> Pair {
        Pair::new("ETH", "USDT")
    }

    #[test]
    fn empty_pairs_allows_any() {
        let client = PaperClient::new(
            PaperConfig {
                url: "ws://127.0.0.1:19999/ws/trade".into(),
                pairs: vec![],
            },
            None,
        );
        assert!(client.is_allowed(&btc_usdt()));
        assert!(client.is_allowed(&Pair::new("SOL", "USDT")));
    }

    #[test]
    fn configured_pairs_filter() {
        let client = PaperClient::new(
            PaperConfig {
                url: "ws://127.0.0.1:19999/ws/trade".into(),
                pairs: vec![btc_usdt(), eth_usdt()],
            },
            None,
        );
        assert!(client.is_allowed(&btc_usdt()));
        assert!(client.is_allowed(&eth_usdt()));
        assert!(!client.is_allowed(&Pair::new("SOL", "USDT")));
    }
}
