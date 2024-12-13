use super::{
    config::BinanceConfig,
    websocket::{
        binance_ws::{BinanceChannel, BinanceWSClient},
        types::common::ParsedBinanceMessage,
    },
};
use core_logic::{data::base_data::DataType, error::EngineError};
use tokio::sync::mpsc;

pub struct BinanceClient {
    pub config: BinanceConfig,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig) -> Self {
        BinanceClient { config }
    }

    async fn start_stream(
        &self,
        data_type: DataType,
        buffer_size: usize,
    ) -> Result<mpsc::Receiver<ParsedBinanceMessage>, EngineError> {
        let (tx, rx) = mpsc::channel(buffer_size);

        let channel = BinanceChannel {
            channel: data_type.clone(),
            base: self.config.symbol.base.clone().to_lowercase().into(),
            quote: self.config.symbol.quote.clone().to_lowercase().into(),
        };

        let client = BinanceWSClient::new(channel);
        client.run(tx).await?;

        Ok(rx)
    }

    pub async fn orderbook(&self) -> Result<(), EngineError> {
        let mut rx = self.start_stream(DataType::Orderbook, 100).await?;

        while let Some(message) = rx.recv().await {
            match message {
                ParsedBinanceMessage::OrderBook(orderbook) => {
                    println!("Received OrderBook: {:?}", orderbook);
                }
                _ => {
                    println!("Received other message: {:?}", message);
                }
            }
        }

        Ok(())
    }

    pub async fn trades(&self) -> Result<(), EngineError> {
        let mut rx = self.start_stream(DataType::Trade, 100).await?;

        // TODO: This is just sample code. We need to do something with this data from here.
        while let Some(message) = rx.recv().await {
            match message {
                ParsedBinanceMessage::Trade(trade) => {
                    println!("Received Trade: {:?}", trade);
                }
                _ => {
                    println!("Received other message: {:?}", message);
                }
            }
        }
        Ok(())
    }
}
