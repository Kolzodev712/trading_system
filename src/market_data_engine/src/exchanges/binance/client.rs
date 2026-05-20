use super::{
    config::BinanceConfig,
    websocket::{
        client::{BinanceChannel, BinanceWSClient},
        types::common::ParsedBinanceMessage,
    },
};
use crate::messages::WSMessageHandler;
use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::{
    error::EngineError, types::common::DataType, venue::cex::traits::MarketDataClient, Exchange,
};

#[derive(Debug)]
pub struct BinanceClient {
    pub config: BinanceConfig,
    buffer_size: usize,
    message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
    symbol_registry: Arc<SymbolRegistry>,
}

impl BinanceClient {
    pub fn new(
        config: BinanceConfig,
        buffer_size: usize,
        message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
        _pairs: Vec<trading_core::types::common::Pair>, // Pairs come from config now, kept for API compatibility
    ) -> Self {
        // Initialize symbol registry with pairs from config
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(config.pairs.clone()));

        BinanceClient {
            config,
            buffer_size,
            message_handler,
            symbol_registry,
        }
    }

    /// Get the symbol registry
    pub fn symbol_registry(&self) -> Arc<SymbolRegistry> {
        self.symbol_registry.clone()
    }

    pub fn set_message_handler(&mut self, handler: Box<dyn WSMessageHandler + Send + Sync>) {
        self.message_handler = Some(handler);
    }

    pub async fn start_stream(&mut self, data_type: DataType) -> Result<(), EngineError> {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        let channels = self.build_channels(data_type.clone());

        self.run_with_reconnect(channels, tx, rx, data_type).await
    }

    fn build_channels(&self, data_type: DataType) -> Vec<BinanceChannel> {
        // Use pairs from config (single source of truth)
        self.config
            .pairs
            .iter()
            .map(|pair| BinanceChannel {
                channel: data_type.clone(),
                base: pair.base.to_lowercase().into(),
                quote: pair.quote.to_lowercase().into(),
                limit: self.config.limit,
            })
            .collect()
    }

    async fn run_with_reconnect(
        &self,
        channels: Vec<BinanceChannel>,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        mut rx: mpsc::Receiver<ParsedBinanceMessage>,
        data_type: DataType,
    ) -> Result<(), EngineError> {
        let mut base_delay = std::time::Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max_delay = std::time::Duration::from_secs(self.config.max_delay.unwrap_or(60));

        loop {
            match self
                .connect_and_stream(channels.clone(), tx.clone(), &mut rx, data_type.clone())
                .await
            {
                Ok(_) => debug!("WebSocket stream closed cleanly"),
                Err(e) => error!("WebSocket error: {:?}", e),
            }

            self.wait_before_reconnect(&mut base_delay, max_delay).await;
        }
    }

    async fn connect_and_stream(
        &self,
        channels: Vec<BinanceChannel>,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        rx: &mut mpsc::Receiver<ParsedBinanceMessage>,
        data_type: DataType,
    ) -> Result<(), EngineError> {
        let pair_count = channels.len();
        let client = BinanceWSClient::new(channels, self.symbol_registry.clone());
        client.run(tx.clone()).await?;

        info!(
            "WebSocket stream started for {data_type:?} ({} pair(s))",
            pair_count
        );

        self.event_loop(rx).await
    }

    async fn event_loop(
        &self,
        rx: &mut mpsc::Receiver<ParsedBinanceMessage>,
    ) -> Result<(), EngineError> {
        while let Some(message) = rx.recv().await {
            self.handle_message(message).await?;
        }

        // Channel closed - expected during shutdown
        debug!("Event channel closed (shutdown)");
        Ok(())
    }

    async fn handle_message(&self, message: ParsedBinanceMessage) -> Result<(), EngineError> {
        if let Some(handler) = &self.message_handler {
            match message {
                ParsedBinanceMessage::OrderBook { orderbook } => {
                    // Extract symbol from pair for backward compatibility with handler interface
                    // Use uppercase to ensure it matches registry
                    let symbol = orderbook.pair.convert_to_smolstr().to_uppercase();
                    handler
                        .handle_orderbook_with_symbol(&symbol, orderbook)
                        .await?
                }
                ParsedBinanceMessage::Trade { trade } => {
                    // Extract symbol from pair for backward compatibility with handler interface
                    let symbol = trade.pair.convert_to_smolstr();
                    handler.handle_trade_with_symbol(&symbol, trade).await?
                }
                _ => {
                    debug!("Ignored unknown message: {message:?}");
                }
            }
        } else {
            error!("No message handler assigned!");
        }
        Ok(())
    }

    async fn wait_before_reconnect(
        &self,
        base_delay: &mut std::time::Duration,
        max_delay: std::time::Duration,
    ) {
        let jitter = std::time::Duration::from_millis(rand::random::<u64>() % 1000);
        let sleep_time = (*base_delay + jitter).min(max_delay);

        debug!("Reconnecting after {sleep_time:?}");
        tokio::time::sleep(sleep_time).await;

        *base_delay = (*base_delay * 2).min(max_delay);
    }
}

#[async_trait]
impl MarketDataClient for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn orderbooks(&mut self) -> Result<(), EngineError> {
        if self.message_handler.is_none() {
            return Err(EngineError::NoMessageHandler);
        }

        self.start_stream(DataType::Orderbook).await
    }

    async fn trades(&mut self) -> Result<(), EngineError> {
        if self.message_handler.is_none() {
            return Err(EngineError::NoMessageHandler);
        }

        self.start_stream(DataType::Trade).await
    }
}
