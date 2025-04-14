use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::super::binance::websocket::types::user_data::DataHandler;
use super::{
    config::BinanceConfig,
    websocket::{
        client::BinanceWSClient,
        types::user_data::events::{BinanceWsEventType, ParsedBinanceMessage},
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
use tracing::debug;

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
            self.config.ws.api_key.clone(),
            self.config.ws.secret_key.clone(),
        );

        let client = BinanceWSClient::new(signer);
        let mut ws_stream = client.connect_and_authenticate().await?;

        if data_type != BinanceWsEventType::Unknown {
            client.subscribe_to_user_stream(&mut ws_stream).await?;
        }

        client.start_event_loop(ws_stream, tx, DataHandler).await?;

        while let Ok(message) = self.channel_manager.receive(&mut rx).await {
            match message {
                ParsedBinanceMessage::OutboundAccountPosition(position) => {
                    debug!("Outbound Account Position update: {:?}", position);
                }
                ParsedBinanceMessage::BalanceUpdate(balance) => {
                    debug!("Balance update: {:?}", balance);
                }
                ParsedBinanceMessage::ExecutionReport(exec_report) => {
                    debug!("Execution report update: {:?}", exec_report);
                }
                ParsedBinanceMessage::Raw(raw) => {
                    debug!("Received raw message: {:?}", raw);
                }
            }
        }

        Err(EngineError::ConnectionClosed)
    }
}

#[async_trait]
impl PrivateDataClient for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn raw_stream(&mut self) -> Result<(), EngineError> {
        self.start_stream(BinanceWsEventType::Unknown).await
    }

    async fn balances(&mut self) -> Result<(), EngineError> {
        self.start_stream(BinanceWsEventType::BalanceUpdate).await
    }

    async fn orders(&mut self) -> Result<(), EngineError> {
        self.start_stream(BinanceWsEventType::ExecutionReport).await
    }
}
