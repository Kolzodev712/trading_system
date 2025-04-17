use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::super::binance::websocket::types::user_data::DataHandler;
use super::websocket::client::{BinanceSubscriptionHandler, ConnectionSession};
use super::{
    config::BinanceConfig,
    websocket::{client::BinanceWSClient, types::user_data::events::ParsedBinanceMessage},
};
use async_trait::async_trait;
use core_logic::{
    channels::{ChannelHandler, ChannelManager},
    error::EngineError,
    traits::PrivateDataClient,
    Exchange,
};
use std::{fmt::Debug, sync::Arc};
use tokio::{sync::RwLock, time::Duration};
use tracing::{debug, error};

#[derive(Debug, Clone)]
pub struct BinanceClient {
    pub config: BinanceConfig,
    pub channel_manager: ChannelManager,
    connection_session: Arc<RwLock<Option<ConnectionSession>>>,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, buffer_size: usize) -> Self {
        let channel_manager = ChannelManager::new(buffer_size);
        BinanceClient {
            config,
            channel_manager,
            connection_session: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_connection_session(&self) -> Option<ConnectionSession> {
        self.connection_session.read().await.clone()
    }

    async fn start_event_loop(&mut self, sub_to_user_stream: bool) -> Result<(), EngineError> {
        let (event_tx, mut event_rx) = self.channel_manager.create_channel();
        let (rpc_tx, _) = self.channel_manager.create_channel();

        let signer = BinanceWsAuthenticator::new(
            self.config.ws.api_key.clone(),
            self.config.ws.secret_key.clone(),
        );

        let client = BinanceWSClient::new(signer.clone());
        let mut ws_stream = client.connect_and_authenticate().await?;

        if sub_to_user_stream {
            client.subscribe_to_user_stream(&mut ws_stream).await?;
        }

        let session = client
            .split_stream(ws_stream, event_tx.clone(), rpc_tx, DataHandler)
            .await?;

        *self.connection_session.write().await = Some(session.clone());

        // ✅ spawn periodic task inside the stream method itself
        let outbound = session.outbound_tx.clone();
        tokio::spawn(async move {
            let subscription_handler = BinanceSubscriptionHandler::new(signer);
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;

                match subscription_handler.create_account_status_request() {
                    Ok(rpc) => {
                        if outbound.send(rpc).await.is_err() {
                            error!("Failed to send account.status RPC");
                        }
                    }
                    Err(e) => {
                        error!("Failed to create account.status RPC: {:?}", e);
                    }
                }
            }
        });

        while let Ok(message) = self.channel_manager.receive(&mut event_rx).await {
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
                    debug!("Received message: {:?}", raw);
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

    async fn stream(&mut self) -> Result<(), EngineError> {
        self.start_event_loop(false).await
    }
}
