use crate::exchanges::binance::common_types::BinanceOrderType;
use core_logic::{
    data::{
        base_data::{OrderId, Pair},
        private_data::{Balance, Order, OrderRequest},
    },
    traits::TradingClient,
};
use smol_str::SmolStr;

use super::super::binance::websocket::types::rpc::{
    cancel_all::BinanceWsCancelAllOrdersReqParams,
    cancel_order::BinanceWsCancelOrderReqParams,
    open_order::{map_open_order_to_core, BinanceWsOrderReqParams},
};
use super::websocket::client::{BinanceSubscriptionHandler, ConnectionSession};
use super::{
    config::BinanceConfig,
    websocket::{
        client::{BinanceWSClient, InboundHandler},
        types::{
            common::BinanceInbound,
            rpc::{BinanceRpcRequest, BinanceRpcResponse},
            user_data::events::ParsedBinanceMessage,
        },
    },
};
use crate::exchanges::binance::sign::BinanceWsAuthenticator;
use async_trait::async_trait;
use core_logic::{error::EngineError, traits::PrivateDataClient, Exchange};
use std::{collections::HashMap, fmt::Debug, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex, RwLock},
    time::{sleep, timeout, Duration},
};
use tracing::{debug, error, warn};

/// Metadata stored for each order to enable cancellation by ID
#[derive(Debug, Clone)]
struct OrderMetadata {
    symbol: SmolStr,
    order_type: BinanceOrderType,
}

#[derive(Debug, Clone)]
pub struct BinanceClient {
    pub(crate) config: BinanceConfig,
    pub(crate) outbound_rpc_tx: Arc<Mutex<Option<mpsc::Sender<BinanceRpcRequest>>>>,
    outbound_rpc_rx: Arc<Mutex<HashMap<String, oneshot::Sender<BinanceRpcResponse>>>>,
    ws_client: BinanceWSClient,
    connection_session: Arc<RwLock<Option<ConnectionSession>>>,
    subscription_handler: BinanceSubscriptionHandler,
    shutdown_tx: broadcast::Sender<()>,
    /// Tracks order metadata (symbol, order_type) by OrderId for cancellation
    order_tracker: Arc<RwLock<HashMap<OrderId, OrderMetadata>>>,
    /// Channel for forwarding execution reports to strategy
    execution_report_tx: Arc<
        Mutex<
            Option<
                mpsc::Sender<super::websocket::types::user_data::execution_report::ExecutionReport>,
            >,
        >,
    >,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, _buffer_size: usize) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());
        let session = Arc::new(RwLock::new(None));
        let ws_client = BinanceWSClient::new(signer.clone(), session.clone());
        let subscription_handler = BinanceSubscriptionHandler::new(signer, session);

        BinanceClient {
            config,
            outbound_rpc_tx: Arc::new(Mutex::new(None)),
            outbound_rpc_rx: Arc::new(Mutex::new(HashMap::new())),
            ws_client,
            connection_session: Arc::new(RwLock::new(None)),
            subscription_handler,
            shutdown_tx,
            order_tracker: Arc::new(RwLock::new(HashMap::new())),
            execution_report_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_connection_session(&self) -> Option<ConnectionSession> {
        self.connection_session.read().await.clone()
    }

    /// Set the execution report channel for forwarding execution reports to strategy
    pub async fn set_execution_report_channel(
        &self,
        tx: mpsc::Sender<super::websocket::types::user_data::execution_report::ExecutionReport>,
    ) {
        *self.execution_report_tx.lock().await = Some(tx);
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
                    if let Err(e) = self.run_stream_loop(inbound_rx, shutdown_rx).await {
                        error!("Stream loop exited with error: {:?}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to connect / auth: {:?}", e);
                    let mut guard = self.outbound_rpc_rx.lock().await;
                    Self::fail_all_pending(&mut guard);
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
        debug!("Starting WebSocket connection and authentication...");
        let mut ws = self.ws_client.connect_and_authenticate().await?;
        debug!("Authentication complete, subscribing to user stream...");

        self.ws_client.subscribe_to_user_stream(&mut ws).await?;
        debug!("Subscription complete, splitting stream into reader/writer tasks...");

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

        debug!("WebSocket stream split complete, connection ready");
        Ok(session)
    }

    async fn run_stream_loop(
        &self,
        mut inbound_rx: mpsc::Receiver<BinanceInbound>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("🚪 Received external shutdown, stopping stream loop");

                    let mut guard = self.outbound_rpc_rx.lock().await;
                    Self::fail_all_pending(&mut guard);

                    break;
                }
                maybe = inbound_rx.recv() => match maybe {
                    Some(BinanceInbound::Rpc(resp)) => {
                        // extract id
                        let id = match &resp {
                            BinanceRpcResponse::AccountStatus(r) => r.id.to_string(),
                            BinanceRpcResponse::TestOrder(r)     => r.id.to_string(),
                            BinanceRpcResponse::OpenOrder(r)     => r.id.to_string(),
                            BinanceRpcResponse::CancelOrder(r)   => r.id.to_string(),
                            BinanceRpcResponse::CancelAll(r)     => r.id.to_string(),
                        };
                        debug!("Received RPC response: id={id}, looking for pending request...", id = id);
                        // try to fulfill pending waiter
                        let mut pending = self.outbound_rpc_rx.lock().await;
                        if let Some(tx) = pending.remove(&id) {
                            debug!("Found pending request for id={id}, sending response", id = id);
                            let _ = tx.send(resp);
                        } else {
                            warn!("RPC response with id {id} had no pending waiter (may be duplicate or unsolicited)", id = id);
                        }
                    }
                    Some(BinanceInbound::Event(evt)) => {
                        match evt {
                            ParsedBinanceMessage::OutboundAccountPosition(p) => debug!("Position: {:?}", p),
                            ParsedBinanceMessage::BalanceUpdate(b) => debug!("Balance: {:?}", b),
                            ParsedBinanceMessage::ExecutionReport(r) => {
                                debug!("Exec rpt: {:?}", r);
                                // Forward execution report to strategy if channel exists
                                if let Some(tx) = self.execution_report_tx.lock().await.as_ref() {
                                    if tx.send(r.clone()).await.is_err() {
                                        debug!("Execution report channel closed");
                                    }
                                }
                            }
                            ParsedBinanceMessage::Raw(r) => debug!("Raw msg: {r}"),
                        }
                    }
                    None => {
                        warn!(
                            "Inbound channel closed - WebSocket connection lost. Will attempt reconnection..."
                        );

                        let mut guard = self.outbound_rpc_rx.lock().await;
                        Self::fail_all_pending(&mut guard);

                        break;
                    }
                }
            }
        }
        Ok(())
    }

    async fn send_rpc_and_wait(
        &self,
        req: BinanceRpcRequest,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let id = match &req {
            BinanceRpcRequest::AccountStatus(r) => r.id.to_string(),
            BinanceRpcRequest::TestOrder(r) => r.id.to_string(),
            BinanceRpcRequest::OpenOrder(r) => {
                debug!(
                    "Sending open_order RPC: id={id}, method={method}",
                    id = r.id,
                    method = r.method
                );
                r.id.to_string()
            }
            BinanceRpcRequest::CancelOrder(r) => r.id.to_string(),
            BinanceRpcRequest::CancelAll(r) => r.id.to_string(),
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.outbound_rpc_rx.lock().await;
            pending.insert(id.clone(), tx);
        }

        let tx_rpc = {
            let guard = self.outbound_rpc_tx.lock().await;
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| EngineError::new_custom_error("RPC channel not yet ready".into()))?
        };

        debug!("Sending RPC request: id={id}", id = id);
        if tx_rpc.send(req).await.is_err() {
            error!("Failed to send RPC to writer channel: id={id}", id = id);
            let mut pending = self.outbound_rpc_rx.lock().await;
            pending.remove(&id);
            return Err(EngineError::new_custom_error("Failed to send RPC".into()));
        }
        debug!(
            "RPC request sent to writer, waiting for response (timeout=30s): id={id}",
            id = id
        );

        // Wait for response with timeout (30 seconds for order operations)
        match timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => {
                debug!("RPC response received: id={id}", id = id);
                Self::ensure_ok(&resp)?;
                Ok(resp)
            }
            Ok(Err(_)) => {
                // Channel closed
                error!("RPC response channel closed: id={id}", id = id);
                let mut pending = self.outbound_rpc_rx.lock().await;
                pending.remove(&id);
                Err(EngineError::ChannelClosedError)
            }
            Err(_) => {
                // Timeout - check if request is still pending
                let pending_count = {
                    let pending = self.outbound_rpc_rx.lock().await;
                    pending.len()
                };
                error!(
                    "RPC request timed out after 30 seconds: id={id}, pending_requests={pending_count}",
                    id = id,
                    pending_count = pending_count
                );
                let mut pending = self.outbound_rpc_rx.lock().await;
                pending.remove(&id);
                Err(EngineError::new_custom_error(
                    format!("RPC request timed out after 30 seconds (id: {id})").into(),
                ))
            }
        }
    }

    pub async fn account_status(&self) -> Result<BinanceRpcResponse, EngineError> {
        let req = self
            .subscription_handler
            .create_account_status_request()
            .await?;
        let resp = self.send_rpc_and_wait(req).await?;
        Ok(resp)
    }

    pub async fn test_order(
        &self,
        params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let req = self
            .subscription_handler
            .create_test_order_request(params)
            .await?;
        let resp = self.send_rpc_and_wait(req).await?;
        Ok(resp)
    }

    // Low-level RPC methods for direct use with Binance-specific types
    // For high-level API, use the TradingClient trait methods instead
    pub async fn open_order_rpc(
        &self,
        params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let req = self
            .subscription_handler
            .create_open_order_request(params)
            .await?;
        let resp = self.send_rpc_and_wait(req).await?;
        Ok(resp)
    }

    pub async fn cancel_order_rpc(
        &self,
        params: BinanceWsCancelOrderReqParams,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let req = self
            .subscription_handler
            .create_cancel_order_request(params)
            .await?;
        let resp = self.send_rpc_and_wait(req).await?;
        Ok(resp)
    }

    pub async fn cancel_all_orders_rpc(
        &self,
        params: BinanceWsCancelAllOrdersReqParams,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let req = self
            .subscription_handler
            .create_cancel_all_orders_request(params)
            .await?;
        let resp = self.send_rpc_and_wait(req).await?;
        Ok(resp)
    }

    fn ensure_ok(resp: &BinanceRpcResponse) -> Result<(), EngineError> {
        match resp {
            BinanceRpcResponse::AccountStatus(r) => {
                if r.status != 200 {
                    return Err(EngineError::new_custom_error(
                        format!("WS RPC error status={status}", status = r.status).into(),
                    ));
                }
            }
            BinanceRpcResponse::TestOrder(r) | BinanceRpcResponse::OpenOrder(r) => {
                if r.status != 200 {
                    let error_msg = r
                        .error
                        .as_ref()
                        .map(|e| format!("code={code}, msg={msg}", code = e.code, msg = e.msg))
                        .unwrap_or_else(|| format!("status={status}", status = r.status));
                    return Err(EngineError::new_custom_error(
                        format!("WS RPC error: {error_msg}", error_msg = error_msg).into(),
                    ));
                }
            }
            BinanceRpcResponse::CancelOrder(r) => {
                if r.status != 200 {
                    return Err(EngineError::new_custom_error(
                        format!("WS RPC error status={status}", status = r.status).into(),
                    ));
                }
            }
            BinanceRpcResponse::CancelAll(r) => {
                if r.status != 200 {
                    return Err(EngineError::new_custom_error(
                        format!("WS RPC error status={status}", status = r.status).into(),
                    ));
                }
            }
        }
        Ok(())
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

    fn fail_all_pending(
        map: &mut std::collections::HashMap<String, oneshot::Sender<BinanceRpcResponse>>,
    ) {
        // Draining drops each Sender, which closes the oneshot.
        for _ in map.drain() {}
    }
}

#[async_trait::async_trait]
impl TradingClient<OrderRequest, Pair> for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn test_open_order(&self, req: OrderRequest) -> Result<(), EngineError> {
        let params = BinanceWsOrderReqParams::try_from(req)?;
        let rpc = self
            .subscription_handler
            .create_test_order_request(params)
            .await?;
        let resp = self.send_rpc_and_wait(rpc).await?;
        Self::ensure_ok(&resp)?;
        match resp {
            BinanceRpcResponse::TestOrder(_) => Ok(()),
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant: {:?}", other).into(),
            )),
        }
    }

    async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        let params = BinanceWsOrderReqParams::try_from(req.clone())?;
        let params_for_map = params.clone();

        let resp = self.open_order_rpc(params).await?;
        Self::ensure_ok(&resp)?;

        match resp {
            BinanceRpcResponse::OpenOrder(ws_resp) => {
                let order = map_open_order_to_core(params_for_map.clone(), ws_resp.clone())?;

                // Store order metadata for future cancellation
                let symbol = SmolStr::from(&req.symbol);
                let order_type = BinanceOrderType::from(req.order_type);
                let metadata = OrderMetadata { symbol, order_type };
                {
                    let mut tracker = self.order_tracker.write().await;
                    tracker.insert(order.order_id.clone(), metadata);
                }

                Ok(order)
            }
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant in open_order: {other:?}").into(),
            )),
        }
    }

    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        // Binance WebSocket API requires symbol, orig_client_order_id, and order_type.
        // Look up the order metadata from our tracker.
        let metadata = {
            let tracker = self.order_tracker.read().await;
            tracker.get(&order_id).cloned()
        };

        let (symbol, order_type) = match metadata {
            Some(meta) => (meta.symbol, meta.order_type),
            None => {
                // Fallback: use config symbol and default to Limit if order not found in tracker
                // This handles cases where orders were created before tracking was implemented
                // or orders created outside this client instance
                debug!(
                    "Order {order_id} not found in tracker, using config symbol and default order type",
                    order_id = order_id.as_str()
                );
                (
                    self.config.symbol.convert_to_smolstr(),
                    BinanceOrderType::Limit,
                )
            }
        };

        let orig_client_order_id = SmolStr::new(order_id.as_str());
        let params = BinanceWsCancelOrderReqParams::new(symbol, orig_client_order_id, order_type);
        let resp = self.cancel_order_rpc(params).await?;
        Self::ensure_ok(&resp)?;

        // Remove from tracker after successful cancellation
        {
            let mut tracker = self.order_tracker.write().await;
            tracker.remove(&order_id);
        }

        Ok(())
    }

    async fn cancel_all_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        let params = BinanceWsCancelAllOrdersReqParams::new(pair.convert_to_smolstr());
        let resp = self.cancel_all_orders_rpc(params).await?;
        Self::ensure_ok(&resp)?;

        match resp {
            BinanceRpcResponse::CancelAll(_) => Ok(()),
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant: {:?}", other).into(),
            )),
        }
    }

    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        let rpc = self
            .subscription_handler
            .create_account_status_request()
            .await?;
        let resp = self.send_rpc_and_wait(rpc).await?;
        Self::ensure_ok(&resp)?;

        match resp {
            BinanceRpcResponse::AccountStatus(ws_resp) => Ok(ws_resp.into()),
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant in fetch_balance: {:?}", other).into(),
            )),
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

    fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());

        tokio::spawn({
            let pending = self.outbound_rpc_rx.clone();
            async move {
                let mut guard = pending.lock().await;
                Self::fail_all_pending(&mut guard);
            }
        });
    }
}
