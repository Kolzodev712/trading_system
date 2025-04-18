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
use tokio::sync::mpsc;
use tracing::{debug, error, info};

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

    pub async fn start_stream(&mut self, data_type: DataType) -> Result<(), EngineError> {
        let (tx, rx) = self.channel_manager.create_channel();
        let channel = self.build_channel(data_type.clone());

        self.run_with_reconnect(channel, tx, rx, data_type).await
    }

    fn build_channel(&self, data_type: DataType) -> BinanceChannel {
        BinanceChannel {
            channel: data_type,
            base: self.config.symbol.base.to_lowercase().into(),
            quote: self.config.symbol.quote.to_lowercase().into(),
            limit: self.config.limit,
        }
    }

    async fn run_with_reconnect(
        &self,
        channel: BinanceChannel,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        mut rx: mpsc::Receiver<ParsedBinanceMessage>,
        data_type: DataType,
    ) -> Result<(), EngineError> {
        let mut base_delay = std::time::Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max_delay = std::time::Duration::from_secs(self.config.max_delay.unwrap_or(60));

        loop {
            match self
                .connect_and_stream(channel.clone(), tx.clone(), &mut rx, data_type.clone())
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
        channel: BinanceChannel,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        rx: &mut mpsc::Receiver<ParsedBinanceMessage>,
        data_type: DataType,
    ) -> Result<(), EngineError> {
        let client = BinanceWSClient::new(channel);
        client.run(tx.clone()).await?;

        info!("✅ WebSocket stream started for {:?}", data_type);

        self.event_loop(rx).await
    }

    async fn event_loop(
        &self,
        rx: &mut mpsc::Receiver<ParsedBinanceMessage>,
    ) -> Result<(), EngineError> {
        while let Ok(message) = self.channel_manager.receive(rx).await {
            self.handle_message(message).await?;
        }

        error!("❌ Event channel closed unexpectedly");
        Err(EngineError::ConnectionClosed)
    }

    async fn handle_message(&self, message: ParsedBinanceMessage) -> Result<(), EngineError> {
        if let Some(handler) = &self.message_handler {
            match message {
                ParsedBinanceMessage::OrderBook(orderbook) => {
                    handler.handle_orderbook(orderbook).await?
                }
                ParsedBinanceMessage::Trade(trade) => handler.handle_trade(trade).await?,
                _ => {
                    debug!("⚠️ Ignored unknown message: {:?}", message);
                }
            }
        } else {
            error!("❌ No message handler assigned!");
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

        debug!("🔁 Reconnecting after {:?}", sleep_time);
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

    async fn orderbook_snapshot(&self, _pair: Pair) -> Result<Orderbook, EngineError> {
        unimplemented!()
    }

    async fn trade_snapshot(&self, _pair: Pair) -> Result<Trade, EngineError> {
        unimplemented!()
    }
}
