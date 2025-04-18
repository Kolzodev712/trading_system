use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::super::binance::websocket::types::user_data::DataHandler;
use super::websocket::client::{BinanceSubscriptionHandler, ConnectionSession};
use super::{
    config::BinanceConfig,
    websocket::{
        client::BinanceWSClient,
        types::{
            rpc::{BinanceRpcRequest, BinanceRpcResponse},
            user_data::events::ParsedBinanceMessage,
        },
    },
};
use async_trait::async_trait;
use core_logic::{
    channels::{ChannelHandler, ChannelManager},
    error::EngineError,
    traits::PrivateDataClient,
    Exchange,
};
use std::{fmt::Debug, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    time::{sleep, Duration},
};
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
        loop {
            let (event_tx, event_rx) = self.channel_manager.create_channel();
            let (rpc_tx, _) = self.channel_manager.create_channel();
            let (shutdown_tx, _) = broadcast::channel(1);

            let result = self
                .connect_and_start_stream(
                    sub_to_user_stream,
                    event_tx.clone(),
                    event_rx,
                    rpc_tx.clone(),
                    shutdown_tx.clone(),
                )
                .await;

            if let Err(e) = result {
                error!("Connection or stream failed: {:?}", e);
            }

            self.exponential_backoff_wait().await;
        }
    }

    async fn connect_and_start_stream(
        &self,
        sub_to_user_stream: bool,
        event_tx: mpsc::Sender<ParsedBinanceMessage>,
        event_rx: mpsc::Receiver<ParsedBinanceMessage>,
        rpc_tx: mpsc::Sender<BinanceRpcResponse>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Result<(), EngineError> {
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
            .split_stream(ws_stream, event_tx.clone(), rpc_tx.clone(), DataHandler)
            .await?;

        *self.connection_session.write().await = Some(session.clone());

        self.spawn_rpc_sender(session.outbound_tx.clone(), signer, shutdown_tx.subscribe())
            .await;
        self.run_stream_loop(event_rx, shutdown_tx.subscribe())
            .await
    }

    async fn run_stream_loop(
        &self,
        mut event_rx: mpsc::Receiver<ParsedBinanceMessage>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("Shutdown received, exiting event loop");
                    return Ok(());
                }
                message = self.channel_manager.receive(&mut event_rx) => {
                    match message {
                        Ok(msg) => {
                            match msg {
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
                        Err(_) => {
                            error!("Event channel closed");
                            return Err(EngineError::ConnectionClosed);
                        }
                    }
                }
            }
        }
    }

    async fn exponential_backoff_wait(&mut self) {
        let base_delay = Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max_delay = Duration::from_secs(self.config.max_delay.unwrap_or(60));

        let jitter = Duration::from_millis(rand::random::<u64>() % 1000);
        let sleep_time = (base_delay + jitter).min(max_delay);

        debug!("Reconnecting after {:?}", sleep_time);
        sleep(sleep_time).await;
    }

    // Test function to showcase the rpc sending logic
    async fn spawn_rpc_sender(
        &self,
        outbound_tx: mpsc::Sender<BinanceRpcRequest>,
        signer: BinanceWsAuthenticator,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        tokio::spawn(async move {
            let subscription_handler = BinanceSubscriptionHandler::new(signer);
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Shutdown received in RPC sender");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        if let Ok(rpc) = subscription_handler.create_account_status_request() {
                            if outbound_tx.send(rpc).await.is_err() {
                                error!("Failed to send account.status RPC");
                            }
                        }
                    }
                }
            }
        });
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
