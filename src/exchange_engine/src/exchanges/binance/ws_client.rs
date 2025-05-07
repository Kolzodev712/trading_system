use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::super::binance::websocket::types::{
    rpc::{
        cancel_all::BinanceWsCancelAllOrdersReqParams, cancel_order::BinanceWsCancelOrderReqParams,
        open_order::BinanceWsOrderReqParams,
    },
    user_data::DataHandler,
};
use super::websocket::client::{BinanceSubscriptionHandler, ConnectionSession};
use super::{
    config::BinanceConfig,
    websocket::{
        client::BinanceWSClient,
        types::{
            common::SessionLogonParams,
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
    pub outbound_rpc_tx: Option<mpsc::Sender<BinanceRpcRequest>>,
    connection_session: Arc<RwLock<Option<ConnectionSession>>>,
    subscription_handler: BinanceSubscriptionHandler,
    session: Arc<RwLock<Option<SessionLogonParams>>>,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, buffer_size: usize) -> Self {
        let channel_manager = ChannelManager::new(buffer_size);
        let session = Arc::new(RwLock::new(None));
        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());

        let subscription_handler = BinanceSubscriptionHandler::new(signer, session.clone());

        BinanceClient {
            config,
            channel_manager,
            outbound_rpc_tx: None,
            connection_session: Arc::new(RwLock::new(None)),
            subscription_handler,
            session,
        }
    }

    pub async fn get_connection_session(&self) -> Option<ConnectionSession> {
        self.connection_session.read().await.clone()
    }

    async fn start_event_loop(&mut self) -> Result<(), EngineError> {
        loop {
            let (event_tx, event_rx) = self.channel_manager.create_channel();
            let (rpc_tx, rpc_rx) = self.channel_manager.create_channel();
            let (shutdown_tx, _) = broadcast::channel(1);

            let result = self
                .connect_and_start_stream(
                    event_tx.clone(),
                    event_rx,
                    rpc_tx.clone(),
                    rpc_rx,
                    shutdown_tx.clone(),
                )
                .await;

            match result {
                Ok(_) => {
                    debug!("🎉 start_event_loop succeeded, returning to main");
                    return Ok(());
                }
                Err(e) => {
                    error!("Connection or stream failed: {:?}", e);
                    self.exponential_backoff_wait().await;
                    continue;
                }
            }
        }
    }

    async fn connect_and_start_stream(
        &mut self,
        event_tx: mpsc::Sender<ParsedBinanceMessage>,
        event_rx: mpsc::Receiver<ParsedBinanceMessage>,
        rpc_tx: mpsc::Sender<BinanceRpcResponse>,
        rpc_rx: mpsc::Receiver<BinanceRpcResponse>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Result<(), EngineError> {
        let signer = BinanceWsAuthenticator::new(
            self.config.ws.api_key.clone(),
            self.config.ws.secret_key.clone(),
        );

        let client = BinanceWSClient::new(signer.clone(), Arc::clone(&self.session));
        let mut ws_stream = client.connect_and_authenticate().await?;

        client.subscribe_to_user_stream(&mut ws_stream).await?;

        let (outbound_tx, outbound_rx) = mpsc::channel::<BinanceRpcRequest>(1024);
        self.outbound_rpc_tx = Some(outbound_tx.clone());

        let session = client
            .split_stream(
                ws_stream,
                event_tx.clone(),
                rpc_tx.clone(),
                outbound_tx,
                outbound_rx,
                DataHandler,
            )
            .await?;

        {
            let mut session_guard = self.connection_session.write().await;
            *session_guard = Some(session.clone());
        }

        let event_handle = tokio::spawn({
            let shutdown_rx = shutdown_tx.subscribe();
            let event_rx = event_rx;
            let rpc_rx = rpc_rx;

            async move { Self::run_stream_loop_static(event_rx, rpc_rx, shutdown_rx).await }
        });
        event_handle.await??;
        Ok(())
    }

    async fn run_stream_loop_static(
        mut event_rx: mpsc::Receiver<ParsedBinanceMessage>,
        mut rpc_rx: mpsc::Receiver<BinanceRpcResponse>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("Shutdown received, exiting event loop");
                    return Ok(());
                }
                maybe_msg = event_rx.recv() => {
                    match maybe_msg {
                        Some(msg) => {
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
                        None => {
                            error!("Event channel closed, exiting stream loop");
                            break;
                        }
                    }
                }
                maybe_rpc = rpc_rx.recv() => {
                    match maybe_rpc {
                        Some(rpc_response) => {
                            debug!("✅ RPC Response received: {:?}", rpc_response);
                        }
                        None => {
                            error!("RPC channel closed");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn exponential_backoff_wait(&mut self) {
        let base_delay = Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max_delay = Duration::from_secs(self.config.max_delay.unwrap_or(60));

        let jitter = Duration::from_millis(rand::random::<u64>() % 1000);
        let sleep_time = (base_delay + jitter).min(max_delay);

        debug!("Reconnecting after {:?}", sleep_time);
        sleep(sleep_time).await;
    }

    pub async fn account_status(&self) -> Result<(), EngineError> {
        let rpc_request = self
            .subscription_handler
            .create_account_status_request()
            .await?;

        debug!("🔄 Attempting to send account status request...");
        if let Some(tx) = &self.outbound_rpc_tx {
            tx.send(rpc_request).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send account status request".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "Outbound RPC channel not initialized".into(),
            ))
        }
    }

    pub async fn test_order(&self, params: BinanceWsOrderReqParams) -> Result<(), EngineError> {
        let rpc_request = self
            .subscription_handler
            .create_test_order_request(params)
            .await?;

        debug!("Sending Test Order RPC: {:?}", rpc_request);

        if let Some(tx) = &self.outbound_rpc_tx {
            tx.send(rpc_request).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send test order request".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "Outbound RPC channel not initialized".into(),
            ))
        }
    }

    pub async fn open_order(&self, params: BinanceWsOrderReqParams) -> Result<(), EngineError> {
        let rpc_request = self
            .subscription_handler
            .create_open_order_request(params)
            .await?;

        if let Some(tx) = &self.outbound_rpc_tx {
            tx.send(rpc_request).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send open order request".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "Outbound RPC channel not initialized".into(),
            ))
        }
    }

    pub async fn cancel_order(
        &self,
        params: BinanceWsCancelOrderReqParams,
    ) -> Result<(), EngineError> {
        let rpc_request = self
            .subscription_handler
            .create_cancel_order_request(params)
            .await?;

        if let Some(tx) = &self.outbound_rpc_tx {
            tx.send(rpc_request).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send cancel order request".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "Outbound RPC channel not initialized".into(),
            ))
        }
    }

    pub async fn cancel_all_orders(
        &self,
        params: BinanceWsCancelAllOrdersReqParams,
    ) -> Result<(), EngineError> {
        let rpc_request = self
            .subscription_handler
            .create_cancel_all_orders_request(params)
            .await?;

        if let Some(tx) = &self.outbound_rpc_tx {
            tx.send(rpc_request).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send cancel all orders request".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "Outbound RPC channel not initialized".into(),
            ))
        }
    }
}

#[async_trait]
impl PrivateDataClient for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn stream(&mut self) -> Result<(), EngineError> {
        self.start_event_loop().await
    }
}
