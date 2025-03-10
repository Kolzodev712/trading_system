use super::{
    config::BinanceConfig,
    utils::BinanceTimestampService,
    websocket::{
        client::{BinanceChannel, BinanceWSClient},
        types::common::ParsedBinanceMessage,
    },
};
use async_trait::async_trait;
use core_logic::{
    channels::{ChannelHandler, ChannelManager},
    data::base_data::DataType,
    error::EngineError,
    traits::PrivateDataClient,
    Exchange,
};
use std::fmt::Debug;
use tracing::{error, info, trace};

#[derive(Debug, Clone)]
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

    async fn start_stream(&mut self, data_type: DataType) -> Result<(), EngineError> {
        let (tx, mut rx) = self.channel_manager.create_channel();

        let channel = BinanceChannel {
            channel: data_type.clone(),
            base: self.config.symbol.base.clone().to_lowercase().into(),
            quote: self.config.symbol.quote.clone().to_lowercase().into(),
            limit: self.config.limit.unwrap_or_default(),
        };

        let timestamp_service = BinanceTimestampService::new();
        let server_time = timestamp_service.get_server_timestamp().await?;

        let client = BinanceWSClient::new(channel);
        client
            .run(
                tx,
                self.config.api_key.clone(),
                self.config.secret_key.clone(),
                server_time,
            )
            .await?;
        info!("✅ WebSocket stream started for {:?}", data_type);

        while let Ok(message) = self.channel_manager.receive(&mut rx).await {
            trace!("📩 Received message: {:?}", message);

            match message {
                ParsedBinanceMessage::Login(login) => {
                    info!("Login response: {:?}", login);
                }
                ParsedBinanceMessage::Balance(balance) => {
                    info!("Balance update: {:?}", balance);
                }
                ParsedBinanceMessage::Raw(raw) => {
                    info!("Received raw message: {:?}", raw);
                }
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
impl PrivateDataClient for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn balances(&mut self) -> Result<(), EngineError> {
        self.start_stream(DataType::Account).await
    }
}
