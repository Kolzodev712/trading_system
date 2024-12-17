use super::{
    config::BinanceConfig,
    websocket::{
        binance_ws::{BinanceChannel, BinanceWSClient},
        types::common::ParsedBinanceMessage,
    },
};
use crate::channels::ChannelManager;
use core_logic::{channels::ChannelHandler, data::base_data::DataType, error::EngineError};
use tokio::sync::mpsc;

pub struct BinanceClient {
    pub config: BinanceConfig,
    pub channel_manager: ChannelManager,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, buffer_size: usize) -> Self {
        let channel_manager = ChannelManager::new(buffer_size);
        BinanceClient {
            config,
            channel_manager,
        }
    }

    async fn start_stream(
        &mut self,
        data_type: DataType,
    ) -> Result<mpsc::Receiver<ParsedBinanceMessage>, EngineError> {
        let (tx, mut rx) = self.channel_manager.create_channel();

        let channel = BinanceChannel {
            channel: data_type.clone(),
            base: self.config.symbol.base.clone().to_lowercase().into(),
            quote: self.config.symbol.quote.clone().to_lowercase().into(),
            limit: self.config.limit,
        };

        let client = BinanceWSClient::new(channel);
        client.run(tx).await?;

        while let Ok(message) = self.channel_manager.receive(&mut rx).await {
            match (data_type.clone(), message.clone()) {
                (DataType::Orderbook, ParsedBinanceMessage::OrderBook(orderbook)) => {
                    println!("Received OrderBook: {:?}", orderbook);
                }
                (DataType::Trade, ParsedBinanceMessage::Trade(trade)) => {
                    println!("Received Trade: {:?}", trade);
                }
                _ => {
                    println!("Received other message: {:?}", message);
                }
            }
        }

        Ok(rx)
    }

    /// Expose Receiver to whoever needs this data:
    pub async fn orderbook(&mut self) -> Result<mpsc::Receiver<ParsedBinanceMessage>, EngineError> {
        self.start_stream(DataType::Orderbook).await
    }

    /// Expose Receiver to whoever needs this data:
    pub async fn trades(&mut self) -> Result<mpsc::Receiver<ParsedBinanceMessage>, EngineError> {
        self.start_stream(DataType::Trade).await
    }
}
