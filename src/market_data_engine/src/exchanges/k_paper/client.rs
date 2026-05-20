//! k-paper market data client.
//! Connects to k-paper WebSocket; handles only orderbook and public_trade (ignores execution/balance/position/error).

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
use super::map::{orderbook_to_structured, public_trade_to_structured};
use super::messages::{KPaperOrderbook, KPaperPublicTrade};

#[derive(Debug)]
pub struct PaperClient {
    config: PaperConfig,
    message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
}

impl PaperClient {
    pub fn new(
        config: PaperConfig,
        message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
        _pairs: Vec<Pair>,
    ) -> Self {
        Self {
            config,
            message_handler,
        }
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
                        if let Some(book) = orderbook_to_structured(&ob) {
                            if let Err(e) = handler.handle_orderbook(book).await {
                                warn!("k-paper market data: handle_orderbook error: {}", e);
                            }
                        }
                    }
                }
                Some("public_trade") => {
                    if let Ok(t) = serde_json::from_value::<KPaperPublicTrade>(v) {
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
