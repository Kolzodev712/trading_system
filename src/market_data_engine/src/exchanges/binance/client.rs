use super::{
    config::BinanceConfig,
    websocket::{
        client::{BinanceChannel, BinanceWSClient},
        types::common::ParsedBinanceMessage,
    },
};
use crate::messages::WSMessageHandler;
use async_trait::async_trait;
use core_logic::{
    channels::{ChannelHandler, ChannelManager},
    data::{
        base_data::{DataType, Pair},
        public_data::{Orderbook, Trade},
    },
    error::EngineError,
    traits::MarketDataClient,
    Exchange,
};
use std::fmt::Debug;
use tracing::{error, info, trace};

#[derive(Debug)]
pub struct BinanceClient {
    pub config: BinanceConfig,
    pub channel_manager: ChannelManager,
    message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
}

impl BinanceClient {
    pub fn new(
        config: BinanceConfig,
        buffer_size: usize,
        message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
    ) -> Self {
        let channel_manager = ChannelManager::new(buffer_size);
        BinanceClient {
            config,
            channel_manager,
            message_handler,
        }
    }

    pub fn set_message_handler(&mut self, handler: Box<dyn WSMessageHandler + Send + Sync>) {
        self.message_handler = Some(handler);
    }

    async fn start_stream(&mut self, data_type: DataType) -> Result<(), EngineError> {
        let (tx, mut rx) = self.channel_manager.create_channel();

        let channel = BinanceChannel {
            channel: data_type.clone(),
            base: self.config.symbol.base.clone().to_lowercase().into(),
            quote: self.config.symbol.quote.clone().to_lowercase().into(),
            limit: self.config.limit,
        };

        let client = BinanceWSClient::new(channel);
        client.run(tx).await?;
        info!("✅ WebSocket stream started for {:?}", data_type);

        while let Ok(message) = self.channel_manager.receive(&mut rx).await {
            trace!("📩 Received message: {:?}", message);

            if let Some(handler) = &self.message_handler {
                match message {
                    ParsedBinanceMessage::OrderBook(orderbook) => {
                        if let Err(e) = handler.handle_orderbook(orderbook).await {
                            error!("❌ Failed to handle orderbook: {}", e);
                        }
                    }
                    ParsedBinanceMessage::Trade(trade) => {
                        if let Err(e) = handler.handle_trade(trade).await {
                            error!("❌ Failed to handle trade: {}", e);
                        }
                    }
                    _ => {
                        error!("⚠️ Received unknown message type.");
                    }
                }
            } else {
                error!("❌ No message handler assigned. Messages will be lost!");
            }
        }

        error!(
            "❌ WebSocket stream unexpectedly stopped for {:?}",
            data_type
        );
        Err(EngineError::ConnectionClosed)
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

    async fn orderbook_snapshot(&self, _pair: Pair) -> Result<Orderbook, EngineError> {
        unimplemented!()
    }

    async fn trade_snapshot(&self, _pair: Pair) -> Result<Trade, EngineError> {
        unimplemented!()
    }
}
