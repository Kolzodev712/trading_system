use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::{
    config::BinanceConfig,
    websocket::{
        client::BinanceWSClient,
        types::common::{BinanceWsEventType, ParsedBinanceMessage},
    },
};
use async_trait::async_trait;
use core_logic::{
    channels::{ChannelHandler, ChannelManager},
    error::EngineError,
    traits::PrivateDataClient,
    Exchange,
};
use std::fmt::Debug;
use tracing::{debug, error, info, trace};

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

    async fn start_stream(&mut self, data_type: BinanceWsEventType) -> Result<(), EngineError> {
        let (tx, mut rx) = self.channel_manager.create_channel();

        let signer = BinanceWsAuthenticator::new(
            self.config.api_key_ws.clone(),
            self.config.secret_key_ws.clone(),
        );

        let client = BinanceWSClient::new(signer);
        client.run(tx).await?;
        info!("✅ WebSocket stream started for {:?}", data_type);

        while let Ok(message) = self.channel_manager.receive(&mut rx).await {
            trace!("📩 Received message: {:?}", message);

            match message {
                ParsedBinanceMessage::AccountStatus(account_info) => {
                    debug!("Account update: {:?}", account_info);
                }
                ParsedBinanceMessage::ExecutionReport(exec_report) => {
                    debug!("Execution report update: {:?}", exec_report);
                }
                ParsedBinanceMessage::Raw(raw) => {
                    debug!("Received raw message: {:?}", raw);
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
        self.start_stream(BinanceWsEventType::AccountStatus).await
    }

    async fn orders(&mut self) -> Result<(), EngineError> {
        self.start_stream(BinanceWsEventType::ExecutionReport).await
    }
}
