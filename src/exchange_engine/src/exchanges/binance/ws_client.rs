use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::super::binance::websocket::types::rpc::{
    cancel_all::BinanceWsCancelAllOrdersReqParams, cancel_order::BinanceWsCancelOrderReqParams,
    open_order::BinanceWsOrderReqParams,
};
use super::websocket::client::{BinanceSubscriptionHandler, ConnectionSession};
use super::{
    config::BinanceConfig,
    websocket::{
        client::{BinanceWSClient, InboundHandler},
        types::{
            common::{BinanceInbound, SessionLogonParams},
            rpc::BinanceRpcRequest,
            user_data::events::ParsedBinanceMessage,
        },
    },
};
use async_trait::async_trait;
use core_logic::{
    channels::ChannelManager, error::EngineError, traits::PrivateDataClient, Exchange,
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
            let (inbound_tx, inbound_rx) = mpsc::channel(1024);
            let (ping_tx, ping_rx) = mpsc::channel::<Vec<u8>>(16);
            let (shutdown_tx, _) = broadcast::channel(1);

            let result = self
                .connect_and_start_stream(
                    inbound_tx.clone(),
                    inbound_rx,
                    ping_tx.clone(),
                    ping_rx,
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
        inbound_tx: mpsc::Sender<BinanceInbound>,
        inbound_rx: mpsc::Receiver<BinanceInbound>,
        ping_tx: mpsc::Sender<Vec<u8>>,
        ping_rx: mpsc::Receiver<Vec<u8>>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Result<(), EngineError> {
        let signer = BinanceWsAuthenticator::new(
            self.config.ws.api_key.clone(),
            self.config.ws.secret_key.clone(),
        );
        let client = BinanceWSClient::new(signer, Arc::clone(&self.session));
        let mut ws_stream = client.connect_and_authenticate().await?;
        client.subscribe_to_user_stream(&mut ws_stream).await?;

        let (outbound_tx, outbound_rx) = mpsc::channel::<BinanceRpcRequest>(1024);
        self.outbound_rpc_tx = Some(outbound_tx.clone());

        let session = client
            .split_stream(
                ws_stream,
                inbound_tx.clone(),
                outbound_tx,
                outbound_rx,
                ping_tx,
                ping_rx,
                InboundHandler,
            )
            .await?;
        *self.connection_session.write().await = Some(session);

        // 4) Drive the inbound loop
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            if let Err(e) = BinanceClient::run_stream_loop(inbound_rx, shutdown_rx).await {
                error!("Stream loop exited with error: {:?}", e);
            }
        })
        .await?;

        Ok(())
    }

    async fn run_stream_loop(
        mut inbound_rx: mpsc::Receiver<BinanceInbound>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("Shutdown received, exiting stream loop");
                    break;
                }
                maybe_msg = inbound_rx.recv() => match maybe_msg {
                    Some(BinanceInbound::Event(evt)) => match evt {
                        ParsedBinanceMessage::OutboundAccountPosition(pos) => {
                            debug!("Position update: {:?}", pos);
                        }
                        ParsedBinanceMessage::BalanceUpdate(bal) => {
                            debug!("Balance update: {:?}", bal);
                        }
                        ParsedBinanceMessage::ExecutionReport(rep) => {
                            debug!("Execution report: {:?}", rep);
                        }
                        ParsedBinanceMessage::Raw(raw) => {
                            debug!("Raw user-data message: {}", raw);
                        }
                    },
                    Some(BinanceInbound::Rpc(resp)) => {
                        debug!("✅ RPC Response: {:?}", resp);
                    }
                    None => {
                        error!("Inbound channel closed");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    async fn exponential_backoff_wait(&mut self) {
        let base = Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max = Duration::from_secs(self.config.max_delay.unwrap_or(60));
        let jitter = Duration::from_millis(rand::random::<u64>() % 1000);
        let delay = (base + jitter).min(max);
        debug!("Reconnecting after {:?}", delay);
        sleep(delay).await;
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
