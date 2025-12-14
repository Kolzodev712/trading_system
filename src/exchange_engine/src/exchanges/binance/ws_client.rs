use crate::exchanges::binance::common_types::BinanceOrderType;
use crate::exchanges::binance::rest::client::BinanceHttpClient;
use crate::state::manager::ExchangeStateManager;
use smol_str::SmolStr;
use trading_core::{
    error::EngineError,
    rest::HttpMethod,
    traits::TradingClient,
    types::{
        common::{OrderId, OrderStatus, Pair, Symbol},
        exchange::{Balance, Order, OrderRequest, Trade},
    },
};

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
use std::{collections::HashMap, fmt::Debug, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex, RwLock},
    time::{sleep, timeout, Duration},
};
use tracing::{debug, error, warn};
use trading_core::{traits::PrivateDataClient, types::common::Exchange};

#[derive(Debug, Clone)]
pub struct BinanceClient {
    pub(crate) config: BinanceConfig,
    pub(crate) outbound_rpc_tx: Arc<Mutex<Option<mpsc::Sender<BinanceRpcRequest>>>>,
    outbound_rpc_rx: Arc<Mutex<HashMap<String, oneshot::Sender<BinanceRpcResponse>>>>,
    ws_client: BinanceWSClient,
    connection_session: Arc<RwLock<Option<ConnectionSession>>>,
    subscription_handler: BinanceSubscriptionHandler,
    shutdown_tx: broadcast::Sender<()>,
    /// REST HTTP client for reconciliation calls
    rest_client: BinanceHttpClient,
    /// Exchange state manager (optional - for state management and publishing)
    state_manager: Arc<tokio::sync::Mutex<Option<Arc<ExchangeStateManager>>>>,
    /// Channel capacities for internal communication
    inbound_capacity: usize,
    ping_capacity: usize,
    outbound_rpc_capacity: usize,
}

impl BinanceClient {
    pub fn new(
        config: BinanceConfig,
        inbound_capacity: usize,
        ping_capacity: usize,
        outbound_rpc_capacity: usize,
        shutdown_capacity: usize,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(shutdown_capacity);

        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());
        let session = Arc::new(RwLock::new(None));
        let ws_client = BinanceWSClient::new(signer.clone(), session.clone());
        let subscription_handler = BinanceSubscriptionHandler::new(signer, session);
        let rest_client = BinanceHttpClient::new(config.clone());

        BinanceClient {
            config,
            outbound_rpc_tx: Arc::new(Mutex::new(None)),
            outbound_rpc_rx: Arc::new(Mutex::new(HashMap::new())),
            ws_client,
            connection_session: Arc::new(RwLock::new(None)),
            subscription_handler,
            shutdown_tx,
            rest_client,
            state_manager: Arc::new(tokio::sync::Mutex::new(None)),
            inbound_capacity,
            ping_capacity,
            outbound_rpc_capacity,
        }
    }

    /// Set the exchange state manager
    pub async fn set_state_manager(&self, state_manager: Arc<ExchangeStateManager>) {
        *self.state_manager.lock().await = Some(state_manager);
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
            let (inbound_tx, inbound_rx) = mpsc::channel::<BinanceInbound>(self.inbound_capacity);
            let (ping_tx, ping_rx) = mpsc::channel::<Vec<u8>>(self.ping_capacity);

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

        let (outbound_tx, outbound_rx) =
            mpsc::channel::<BinanceRpcRequest>(self.outbound_rpc_capacity);
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
                            ParsedBinanceMessage::OutboundAccountPosition(position) => {
                                debug!("OutboundAccountPosition: {:?}", position);

                                // Update state manager if available
                                if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                                    let balances: Vec<Balance> = position.into();
                                    if let Err(e) = state_mgr.update_balances(balances).await {
                                        warn!("Failed to update balances in state manager: {}", e);
                                    }
                                }
                            }
                            ParsedBinanceMessage::BalanceUpdate(update) => {
                                debug!("BalanceUpdate (delta): asset={}, delta={}", update.asset, update.delta);

                                // Apply delta to current balance
                                if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                                    let state_arc = state_mgr.get_state();
                                    let state = state_arc.read().await;

                                    // Get current balance or create new one
                                    let current_balance = state.get_balance(update.asset.as_str())
                                        .cloned()
                                        .unwrap_or_else(|| Balance {
                                            symbol: update.asset.clone().into(),
                                            free: rust_decimal::Decimal::ZERO,
                                            used: rust_decimal::Decimal::ZERO,
                                            total: rust_decimal::Decimal::ZERO,
                                        });

                                    drop(state);

                                    // Apply delta to free balance (delta can be positive or negative)
                                    let updated_balance = Balance {
                                        symbol: update.asset.clone().into(),
                                        free: current_balance.free + update.delta,
                                        used: current_balance.used,
                                        total: current_balance.free + update.delta + current_balance.used,
                                    };

                                    if let Err(e) = state_mgr.update_balance(updated_balance).await {
                                        warn!("Failed to update balance from delta: {}", e);
                                    }
                                }
                            }
                            ParsedBinanceMessage::ExecutionReport(report) => {
                                debug!("Exec rpt: {:?}", report);

                                // Clone report for trade creation (before converting to Order which takes ownership)
                                let report_for_trade = report.clone();
                                let order_status: OrderStatus = report.order_status.clone().into();

                                // Convert to Order and update state manager if available
                                let order: Order = report.clone().into();

                                if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                                    // Update order in state
                                    if let Err(e) = state_mgr.update_order(order.clone()).await {
                                        warn!("Failed to update order in state manager: {}", e);
                                    }

                                    // Create Trade when order is filled (Filled or PartiallyFilled)
                                    if matches!(order_status, OrderStatus::Filled | OrderStatus::PartiallyFilled) {
                                        // Get the cumulative filled quantity (total filled so far)
                                        let cumulative_filled = report_for_trade.cumulative_filled_quantity
                                            .unwrap_or_else(|| {
                                                if order_status == OrderStatus::Filled {
                                                    report_for_trade.quantity
                                                } else {
                                                    rust_decimal::Decimal::ZERO
                                                }
                                            });

                                        // Calculate delta (new fill amount) by comparing with previous filled_amount from state
                                        let order_id = report_for_trade.order_id.into();
                                        let previous_filled = {
                                            let state_arc = state_mgr.get_state();
                                            let state = state_arc.read().await;
                                            state.get_order_by_id(&order_id)
                                                .map(|o| o.filled_amount)
                                                .unwrap_or(rust_decimal::Decimal::ZERO)
                                        };

                                        // Delta is the new fill amount in this ExecutionReport
                                        let fill_delta = cumulative_filled - previous_filled;

                                        // Only create trade if there's actually a new fill (delta > 0)
                                        if fill_delta > rust_decimal::Decimal::ZERO {
                                            // Get pair from symbol registry
                                            let state_arc = state_mgr.get_state();
                                            let state = state_arc.read().await;
                                            let pair = state.symbol_registry()
                                                .get_pair(&report_for_trade.symbol.clone().into())
                                                .cloned();
                                            drop(state);

                                            if let Some(pair) = pair {
                                                // Create trade from execution report
                                                let trade = Trade {
                                                    trade_id: format!("{}_{}", report_for_trade.order_id, report_for_trade.transaction_time).into(),
                                                    order_id: report_for_trade.order_id.into(),
                                                    pair,
                                                    price: report_for_trade.price,
                                                    quantity: fill_delta, // Use delta, not cumulative
                                                    side: report_for_trade.side.clone().into(),
                                                    timestamp: report_for_trade.transaction_time,
                                                    fee: rust_decimal::Decimal::ZERO, // Fee not available in ExecutionReport
                                                    fee_currency: Symbol::from("USDT"), // Default, will be updated from REST if needed
                                                };

                                                if let Err(e) = state_mgr.add_trade(trade).await {
                                                    warn!("Failed to add trade from execution report: {}", e);
                                                }
                                            } else {
                                                warn!("Could not find pair for symbol {} when creating trade", report_for_trade.symbol);
                                            }
                                        }
                                    }
                                }

                                // Note: Execution reports are published via ExchangeStateManager's
                                // exchange_update channel. Strategies should subscribe to ExchangeUpdate::Order
                                // instead of using a separate execution_report channel.
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
                Ok(order)
            }
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant in open_order: {other:?}").into(),
            )),
        }
    }

    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        // Binance WebSocket API requires symbol, orig_client_order_id, and order_type.
        // Get order from state manager (authoritative source).
        let (symbol, order_type) = {
            if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                if let Some(order) = state_mgr.get_order_by_id(&order_id).await {
                    // Extract symbol and order_type from the order
                    let symbol = order.symbol.clone();
                    // Infer order_type from order (we don't store it, so default to Limit)
                    // This is acceptable since Binance accepts cancellation without exact order_type match
                    (symbol.as_ref().into(), BinanceOrderType::Limit)
                } else {
                    // Order not in state - cannot determine pair, return error
                    return Err(EngineError::new_custom_error(
                        format!(
                            "Order {} not found in state manager. Cannot determine trading pair for cancellation.",
                            order_id.as_str()
                        )
                        .into(),
                    ));
                }
            } else {
                // State manager not available - cannot determine pair, return error
                return Err(EngineError::new_custom_error(
                    "State manager not available. Cannot determine trading pair for order cancellation.".into(),
                ));
            }
        };

        let orig_client_order_id = SmolStr::new(order_id.as_str());
        let params = BinanceWsCancelOrderReqParams::new(symbol, orig_client_order_id, order_type);
        let resp = self.cancel_order_rpc(params).await?;
        Self::ensure_ok(&resp)?;

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
        // Use REST API for reconciliation (RPC requires WebSocket connection)
        use crate::exchanges::binance::rest::types::{
            balance::{BinanceAccountInfo, BinanceBalanceRequest},
            common::BinanceServerTime,
        };
        use trading_core::types::common::MillisecondTimestamp;

        // Get server time for timestamp
        let time_response: Option<Option<BinanceServerTime>> = self
            .rest_client
            .call::<(), Option<BinanceServerTime>>("time", HttpMethod::GET, None, None, None, false)
            .await?;

        let server_time = time_response
            .flatten()
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to get server time for balance request"
                ))
            })?
            .server_time;

        // Request account balance
        let params = BinanceBalanceRequest {
            omit_zero_balances: Some(true),
            timestamp: MillisecondTimestamp(server_time),
        };

        let response: Option<BinanceAccountInfo> = self
            .rest_client
            .call::<BinanceBalanceRequest, Option<BinanceAccountInfo>>(
                "account",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true, // Signed request
            )
            .await?
            .flatten();

        let account_info = response.ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("Expected account info, but received None"))
        })?;

        let balances: Vec<Balance> = account_info
            .balances
            .into_iter()
            .map(|binance_balance| Balance {
                symbol: binance_balance.asset,
                free: binance_balance.free,
                used: binance_balance.locked,
                total: binance_balance.free + binance_balance.locked,
            })
            .collect();

        Ok(balances)
    }

    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        use crate::exchanges::binance::rest::types::{
            common::BinanceServerTime,
            open_orders::{BinanceOpenOrdersRequest, BinanceOpenOrdersResponse},
        };

        // Get server time for timestamp
        let time_response: Option<Option<BinanceServerTime>> = self
            .rest_client
            .call::<(), Option<BinanceServerTime>>("time", HttpMethod::GET, None, None, None, false)
            .await?;

        let server_time = time_response
            .flatten()
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to get server time for open orders request"
                ))
            })?
            .server_time;

        // Request all open orders (no symbol filter)
        let params = BinanceOpenOrdersRequest {
            symbol: None, // None = all symbols
            timestamp: server_time,
        };

        let response: Option<BinanceOpenOrdersResponse> = self
            .rest_client
            .call::<BinanceOpenOrdersRequest, BinanceOpenOrdersResponse>(
                "openOrders",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true, // Signed request
            )
            .await?;

        let orders = response.unwrap_or_default();
        Ok(orders.into_iter().map(Order::from).collect())
    }

    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        use crate::exchanges::binance::rest::types::{
            common::BinanceServerTime,
            trades::{BinanceTradesRequest, BinanceTradesResponse},
        };

        // If no pair specified, we can't fetch trades (Binance requires symbol)
        let symbol = if let Some(p) = pair {
            p.convert_to_smolstr()
        } else {
            // Return empty if no pair specified
            // In a real scenario, you might want to fetch for all configured pairs
            return Ok(Vec::new());
        };

        let _ = pair; // Acknowledge parameter usage

        // Get server time for timestamp
        let time_response: Option<Option<BinanceServerTime>> = self
            .rest_client
            .call::<(), Option<BinanceServerTime>>("time", HttpMethod::GET, None, None, None, false)
            .await?;

        let server_time = time_response
            .flatten()
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to get server time for trades request"
                ))
            })?
            .server_time;

        // Request trades for the symbol (last 500 trades)
        let params = BinanceTradesRequest {
            symbol,
            start_time: None,
            end_time: None,
            from_id: None,
            limit: Some(500), // Max 1000, but 500 is reasonable
            timestamp: server_time,
        };

        let response: Option<BinanceTradesResponse> = self
            .rest_client
            .call::<BinanceTradesRequest, BinanceTradesResponse>(
                "myTrades",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true, // Signed request
            )
            .await?;

        let trades = response.unwrap_or_default();
        Ok(trades.into_iter().map(Trade::from).collect())
    }

    async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
        // TODO: Implement fetch_market_data_snapshot
        // This should fetch asset/symbol data (trading rules, symbol info, price filters, lot sizes, etc.)
        // from Binance /api/v3/exchangeInfo endpoint
        // This data is used to simplify level configuration
        // See Proposal 08 for details on ExchangeMarketData structure
        //
        // For now, this is a placeholder. The return type will be ExchangeMarketData
        // once that type is defined in Proposal 08 implementation.
        unimplemented!("fetch_market_data_snapshot() - to be implemented for asset data (trading rules, symbol info, etc.)")
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
