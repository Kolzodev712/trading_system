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
            common::BinanceInbound, rpc::BinanceRpcRequest, user_data::events::ParsedBinanceMessage,
        },
    },
};
use async_trait::async_trait;
use core_logic::{
    channels::ChannelManager, error::EngineError, traits::PrivateDataClient, Exchange,
};
use std::{fmt::Debug, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc, Mutex, RwLock},
    time::{sleep, Duration},
};
use tracing::{debug, error};

#[derive(Debug, Clone)]
pub struct BinanceClient {
    pub config: BinanceConfig,
    pub channel_manager: ChannelManager,
    pub outbound_rpc_tx: Arc<Mutex<Option<mpsc::Sender<BinanceRpcRequest>>>>,
    ws_client: BinanceWSClient,
    connection_session: Arc<RwLock<Option<ConnectionSession>>>,
    subscription_handler: BinanceSubscriptionHandler,
    shutdown_tx: broadcast::Sender<()>,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, buffer_size: usize) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        let channel_manager = ChannelManager::new(buffer_size);
        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());
        let session = Arc::new(RwLock::new(None));
        let ws_client = BinanceWSClient::new(signer.clone(), session.clone());
        let subscription_handler = BinanceSubscriptionHandler::new(signer, session);

        BinanceClient {
            config,
            channel_manager,
            outbound_rpc_tx: Arc::new(Mutex::new(None)),
            ws_client,
            connection_session: Arc::new(RwLock::new(None)),
            subscription_handler,
            shutdown_tx,
        }
    }

    pub async fn get_connection_session(&self) -> Option<ConnectionSession> {
        self.connection_session.read().await.clone()
    }

    pub async fn stream(&mut self) -> Result<(), EngineError> {
        self.start_event_loop().await
    }

    async fn start_event_loop(&mut self) -> Result<(), EngineError> {
        let mut backoff_iter = 0u32;

        loop {
            let (inbound_tx, inbound_rx) = mpsc::channel::<BinanceInbound>(1024);
            let (ping_tx, ping_rx) = mpsc::channel::<Vec<u8>>(16);

            match self
                .connect_and_start_stream(inbound_tx, ping_tx, ping_rx)
                .await
            {
                Ok(session) => {
                    *self.connection_session.write().await = Some(session);

                    let shutdown_rx = self.shutdown_tx.subscribe();
                    if let Err(e) = Self::run_stream_loop(inbound_rx, shutdown_rx).await {
                        error!("Stream loop exited with error: {:?}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to connect / auth: {:?}", e);
                }
            }

            self.exponential_backoff_wait(backoff_iter).await;
            backoff_iter = backoff_iter.saturating_add(1);
        }
    }

    async fn connect_and_start_stream(
        &mut self,
        inbound_tx: mpsc::Sender<BinanceInbound>,
        ping_tx: mpsc::Sender<Vec<u8>>,
        ping_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<ConnectionSession, EngineError> {
        let mut ws = self.ws_client.connect_and_authenticate().await?;
        self.ws_client.subscribe_to_user_stream(&mut ws).await?;

        let (outbound_tx, outbound_rx) = mpsc::channel::<BinanceRpcRequest>(1024);
        *self.outbound_rpc_tx.lock().await = Some(outbound_tx.clone());

        let session = self
            .ws_client
            .split_stream(
                ws,
                inbound_tx,
                outbound_tx,
                outbound_rx,
                ping_tx,
                ping_rx,
                InboundHandler,
            )
            .await?;

        Ok(session)
    }

    async fn run_stream_loop(
        mut inbound_rx: mpsc::Receiver<BinanceInbound>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("🚪 Received external shutdown, stopping stream loop");
                    break;
                }
                maybe = inbound_rx.recv() => match maybe {
                    Some(BinanceInbound::Event(evt)) => {
                        match evt {
                            ParsedBinanceMessage::OutboundAccountPosition(p) => debug!("Position: {:?}", p),
                            ParsedBinanceMessage::BalanceUpdate(b) => debug!("Balance: {:?}", b),
                            ParsedBinanceMessage::ExecutionReport(r) => debug!("Exec rpt: {:?}", r),
                            ParsedBinanceMessage::Raw(r) => debug!("Raw msg: {}", r),
                        }
                    }
                    Some(BinanceInbound::Rpc(resp)) => {
                        debug!("✅ RPC Response: {:?}", resp);
                    }
                    None => {
                        error!("🚨 Inbound channel closed");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn account_status(&self) -> Result<(), EngineError> {
        let req = self
            .subscription_handler
            .create_account_status_request()
            .await?;
        debug!("🔄 Attempting to send account status request…");

        let guard = self.outbound_rpc_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            tx.send(req).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send account status request".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "RPC channel not yet ready".into(),
            ))
        }
    }

    pub async fn test_order(&self, params: BinanceWsOrderReqParams) -> Result<(), EngineError> {
        let req = self
            .subscription_handler
            .create_test_order_request(params)
            .await?;
        debug!("📤 Sending Test Order RPC: {:?}", req);

        let guard = self.outbound_rpc_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            tx.send(req)
                .await
                .map_err(|_| EngineError::new_custom_error("Failed to send test order".into()))
        } else {
            Err(EngineError::new_custom_error(
                "RPC channel not yet ready".into(),
            ))
        }
    }

    pub async fn open_order(&self, params: BinanceWsOrderReqParams) -> Result<(), EngineError> {
        let req = self
            .subscription_handler
            .create_open_order_request(params)
            .await?;

        let guard = self.outbound_rpc_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            tx.send(req)
                .await
                .map_err(|_| EngineError::new_custom_error("Failed to send open order".into()))
        } else {
            Err(EngineError::new_custom_error(
                "RPC channel not yet ready".into(),
            ))
        }
    }

    pub async fn cancel_order(
        &self,
        params: BinanceWsCancelOrderReqParams,
    ) -> Result<(), EngineError> {
        let req = self
            .subscription_handler
            .create_cancel_order_request(params)
            .await?;

        let guard = self.outbound_rpc_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            tx.send(req)
                .await
                .map_err(|_| EngineError::new_custom_error("Failed to send cancel order".into()))
        } else {
            Err(EngineError::new_custom_error(
                "RPC channel not yet ready".into(),
            ))
        }
    }

    pub async fn cancel_all_orders(
        &self,
        params: BinanceWsCancelAllOrdersReqParams,
    ) -> Result<(), EngineError> {
        let req = self
            .subscription_handler
            .create_cancel_all_orders_request(params)
            .await?;

        let guard = self.outbound_rpc_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            tx.send(req).await.map_err(|_| {
                EngineError::new_custom_error("Failed to send cancel all orders".into())
            })
        } else {
            Err(EngineError::new_custom_error(
                "RPC channel not yet ready".into(),
            ))
        }
    }

    async fn exponential_backoff_wait(&self, iter: u32) {
        let base = Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max = Duration::from_secs(self.config.max_delay.unwrap_or(60));

        let delay = (base * 2u32.saturating_pow(iter)).min(max);
        let jitter = Duration::from_millis(rand::random::<u64>() % 1000);
        let to_sleep = (delay + jitter).min(max);

        debug!("⏳ reconnecting in {:?}", to_sleep);
        sleep(to_sleep).await;
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

    fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
