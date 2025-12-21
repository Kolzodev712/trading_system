use crate::exchanges::binance::rest::client::BinanceHttpClient;
use crate::state::manager::ExchangeStateManager;
use smol_str::SmolStr;
use trading_core::{
    error::EngineError,
    rest::HttpMethod,
    traits::TradingClient,
    types::{
        common::{OrderId, OrderStatus, Pair},
        exchange::{Balance, Order, OrderRequest, Trade},
    },
};

use super::super::binance::websocket::types::rpc::{
    cancel_all::BinanceWsCancelAllOrdersReqParams,
    cancel_order::BinanceWsCancelOrderReqParams,
    open_order::{map_open_order_to_core, BinanceWsOrderReqParams},
};
use super::rpc_client::BinanceRpcClient;
use super::streaming_client::BinanceStreamingClient;
use super::{
    config::BinanceConfig,
    websocket::{
        client::BinanceSubscriptionHandler,
        types::{
            rpc::{BinanceRpcRequest, BinanceRpcResponse},
            user_data::events::ParsedBinanceMessage,
        },
    },
};
use crate::exchanges::binance::sign::BinanceWsAuthenticator;
use async_trait::async_trait;
use std::{fmt::Debug, sync::Arc};
use tokio::{
    sync::{mpsc, RwLock},
    time::{sleep, Duration},
};
use tracing::{error, info, warn};
use trading_core::{traits::PrivateDataClient, types::common::Exchange};

#[derive(Debug, Clone)]
pub struct BinanceClient {
    pub(crate) config: BinanceConfig,
    /// RPC client for request/response operations
    pub(crate) rpc_client: Arc<BinanceRpcClient>,
    /// Streaming client for subscription events
    streaming_client: Arc<BinanceStreamingClient>,
    /// Subscription handler for creating RPC requests
    subscription_handler: BinanceSubscriptionHandler,
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
        _shutdown_capacity: usize,
    ) -> Self {
        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());
        let session = Arc::new(RwLock::new(None));
        let subscription_handler = BinanceSubscriptionHandler::new(signer, session);

        let rpc_client = Arc::new(BinanceRpcClient::new(
            config.clone(),
            outbound_rpc_capacity,
            ping_capacity,
        ));
        let streaming_client = Arc::new(BinanceStreamingClient::new(
            config.clone(),
            inbound_capacity,
        ));
        let rest_client = BinanceHttpClient::new(config.clone());

        BinanceClient {
            config,
            rpc_client,
            streaming_client,
            subscription_handler,
            rest_client,
            state_manager: Arc::new(tokio::sync::Mutex::new(None)),
            inbound_capacity,
            ping_capacity,
            outbound_rpc_capacity,
        }
    }

    /// Set the exchange state manager
    pub async fn set_state_manager(&self, state_manager: Arc<ExchangeStateManager>) {
        *self.state_manager.lock().await = Some(state_manager.clone());
        // Set on both clients
        self.streaming_client
            .set_state_manager(state_manager.clone())
            .await;
        // RPC client doesn't need state manager
    }

    /// Get a reference to the RPC client (for readiness checks)
    pub fn rpc_client(&self) -> &Arc<BinanceRpcClient> {
        &self.rpc_client
    }

    pub async fn stream(&mut self) -> Result<(), EngineError> {
        info!(
            "[BINANCE CLIENT] Starting stream - using single connection for both streaming and RPC"
        );

        // Use ONE authenticated connection for both streaming and RPC
        // This is the correct approach for Binance - RPC requests work on the same connection as userDataStream
        let mut backoff_iter = 0u32;
        let mut connection_id = 0u64;

        loop {
            connection_id += 1;
            info!(
                "[BINANCE CLIENT {}] Connection attempt {} (backoff_iter={})",
                connection_id, connection_id, backoff_iter
            );

            match self.connect_and_run_unified().await {
                Ok(_) => {
                    info!(
                        "[BINANCE CLIENT {}] Connection ended, reconnecting...",
                        connection_id
                    );
                }
                Err(e) => {
                    warn!(
                        "[BINANCE CLIENT {}] Connection error: {:?}, reconnecting...",
                        connection_id, e
                    );
                }
            }

            // Reset RPC channel on disconnect
            {
                let mut guard = self.rpc_client.rpc_request_tx.lock().await;
                if guard.is_some() {
                    warn!("[BINANCE CLIENT {}] ========== Resetting RPC channel (connection dropped) ==========", connection_id);
                }
                *guard = None;
            }

            // Exponential backoff
            self.exponential_backoff_wait(backoff_iter).await;
            backoff_iter = backoff_iter.saturating_add(1);
        }
    }

    async fn connect_and_run_unified(&mut self) -> Result<(), EngineError> {
        info!("[BINANCE CLIENT] Starting unified connection...");

        // Create channels for unified connection
        let (rpc_request_tx, rpc_request_rx) =
            mpsc::channel::<BinanceRpcRequest>(self.outbound_rpc_capacity);
        let (rpc_response_tx, rpc_response_rx) =
            mpsc::channel::<BinanceRpcResponse>(self.outbound_rpc_capacity);
        let (event_tx, event_rx) = mpsc::channel::<ParsedBinanceMessage>(self.inbound_capacity);
        let (ping_tx, ping_rx) = mpsc::channel::<Vec<u8>>(self.ping_capacity);

        // DO NOT set RPC channel yet - wait until writer task is spawned

        // Connect and authenticate ONCE - use streaming client's ws_client
        info!("[BINANCE CLIENT] Connecting and authenticating...");
        let mut ws = self
            .streaming_client
            .ws_client
            .connect_and_authenticate()
            .await?;
        info!("[BINANCE CLIENT] Authentication successful");

        // Subscribe to user stream
        info!("[BINANCE CLIENT] Subscribing to userDataStream...");
        self.streaming_client
            .ws_client
            .subscribe_to_user_stream(&mut ws)
            .await?;
        info!("[BINANCE CLIENT] User stream subscription successful");

        // Now split the stream and handle both RPC and events
        info!("[BINANCE CLIENT] Splitting stream for unified handling...");
        use crate::exchanges::binance::websocket::client::InboundHandler;
        let mut session = self
            .streaming_client
            .ws_client
            .split_stream(
                ws,
                rpc_request_tx.clone(),
                rpc_request_rx,
                rpc_response_tx.clone(),
                event_tx.clone(),
                ping_tx,
                ping_rx,
                InboundHandler,
            )
            .await?;
        info!("[BINANCE CLIENT] Stream split successful - writer task spawned");

        // Wait for writer task to be ready (proper signaling instead of arbitrary sleep)
        if let Some(writer_ready_rx) = session.writer_ready_rx.take() {
            match tokio::time::timeout(tokio::time::Duration::from_secs(5), writer_ready_rx).await {
                Ok(Ok(_)) => {
                    info!("[BINANCE CLIENT] Writer task is ready");
                }
                Ok(Err(_)) => {
                    warn!("[BINANCE CLIENT] Writer ready signal channel closed");
                }
                Err(_) => {
                    warn!("[BINANCE CLIENT] Timeout waiting for writer ready signal (5s)");
                }
            }
        }

        // Mark RPC channel as ready (writer task is confirmed running)
        {
            let mut guard = self.rpc_client.rpc_request_tx.lock().await;
            *guard = Some(rpc_request_tx.clone());
            info!("[BINANCE CLIENT] RPC channel marked as ready");
        }

        // Spawn event handler
        let state_manager_clone = self.state_manager.clone();
        let event_handler_handle = tokio::spawn(async move {
            BinanceStreamingClient::run_event_handler(event_rx, state_manager_clone).await;
        });

        // Spawn RPC response handler
        let rpc_client_clone = Arc::clone(&self.rpc_client);
        let rpc_handler_handle = tokio::spawn(async move {
            if let Err(e) = rpc_client_clone
                .run_rpc_response_handler(rpc_response_rx)
                .await
            {
                warn!("[BINANCE CLIENT] RPC response handler error: {:?}", e);
            }
        });

        // Extract handles from session
        let mut reader_handle = session.reader_handle.take().expect("Reader handle missing");
        let mut writer_handle = session.writer_handle.take().expect("Writer handle missing");

        info!("[BINANCE CLIENT] All handlers spawned, monitoring connection...");

        // Wait for reader or writer to exit (connection dropped)
        tokio::select! {
            result = &mut reader_handle => {
                warn!("[BINANCE CLIENT] Reader task exited: {:?}", result);
            }
            result = &mut writer_handle => {
                warn!("[BINANCE CLIENT] Writer task exited: {:?}", result);
            }
            _ = event_handler_handle => {
                warn!("[BINANCE CLIENT] Event handler task exited");
            }
            _ = rpc_handler_handle => {
                warn!("[BINANCE CLIENT] RPC handler task exited");
            }
        }

        info!("[BINANCE CLIENT] Connection dropped, exiting unified connection");
        // Channel will be reset by the caller (stream() loop)
        Ok(())
    }

    async fn exponential_backoff_wait(&self, iter: u32) {
        let base = Duration::from_secs(self.config.base_delay.unwrap_or(2));
        let max = Duration::from_secs(self.config.max_delay.unwrap_or(60));
        let delay = (base * 2u32.saturating_pow(iter)).min(max);
        // Simple jitter using system time
        let jitter_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 1000) as u64;
        let jitter = Duration::from_millis(jitter_ms);
        let to_sleep = (delay + jitter).min(max);
        info!(
            "[BINANCE CLIENT] Waiting {:?} before reconnecting...",
            to_sleep
        );
        sleep(to_sleep).await;
    }

    // Old event loop code removed - now using separate streaming and RPC clients

    async fn send_rpc_and_wait(
        &self,
        req: BinanceRpcRequest,
    ) -> Result<BinanceRpcResponse, EngineError> {
        // Delegate to RPC client
        self.rpc_client.send_rpc_and_wait(req).await
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
            BinanceRpcResponse::Error(msg) => {
                return Err(EngineError::new_custom_error(msg.clone().into()));
            }
        }
        Ok(())
    }

    // Old helper methods removed - now handled by separate clients
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
            BinanceRpcResponse::Error(msg) => Err(EngineError::new_custom_error(msg.into())),
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
            BinanceRpcResponse::Error(msg) => Err(EngineError::new_custom_error(msg.into())),
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant in open_order: {other:?}").into(),
            )),
        }
    }

    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        // Binance WebSocket API requires symbol and either orderId (numeric) or origClientOrderId (string)
        // According to Binance docs: prefer numeric orderId, fallback to origClientOrderId
        // Get order from state manager (authoritative source) to get symbol and client_order_id
        let (symbol, client_order_id) = {
            if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                if let Some(order) = state_mgr.get_order_by_id(&order_id).await {
                    // Extract symbol and client_order_id from the order
                    let symbol = order.symbol.clone();
                    tracing::info!(
                        "Found order {} in state manager: symbol={}",
                        order_id.as_str(),
                        symbol.as_ref()
                    );
                    (symbol.as_ref().into(), order.client_order_id.clone())
                } else {
                    // Order not in state - cannot determine pair, return error
                    tracing::error!("Order {} not found in state manager. Cannot determine trading pair for cancellation.", order_id.as_str());
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
                tracing::error!("State manager not available. Cannot determine trading pair for order cancellation.");
                return Err(EngineError::new_custom_error(
                    "State manager not available. Cannot determine trading pair for order cancellation.".into(),
                ));
            }
        };

        tracing::info!("Cancelling order {}: symbol={}", order_id.as_str(), symbol);
        // According to Binance docs: prefer numeric orderId (more reliable), fallback to origClientOrderId
        let params = if let Some(numeric_id) = order_id.as_numeric_id() {
            // Use numeric orderId (preferred by Binance - more reliable)
            BinanceWsCancelOrderReqParams::new_with_order_id(symbol, numeric_id)
        } else if let Some(client_id) = client_order_id {
            // Try client_order_id from stored order if order_id is not numeric
            BinanceWsCancelOrderReqParams::new_with_client_order_id(symbol, client_id)
        } else {
            // Last resort: use order_id as string (shouldn't happen for Binance orders)
            BinanceWsCancelOrderReqParams::new_with_client_order_id(
                symbol,
                SmolStr::new(order_id.as_str()),
            )
        };
        let resp = self.cancel_order_rpc(params).await?;
        Self::ensure_ok(&resp)?;

        tracing::info!("Successfully cancelled order {} via RPC", order_id.as_str());
        Ok(())
    }

    async fn cancel_all_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        tracing::info!(
            "[BinanceClient] cancel_all_orders called for {}/{}",
            pair.base,
            pair.quote
        );
        let symbol = pair.convert_to_smolstr();
        let params = BinanceWsCancelAllOrdersReqParams::new(symbol.clone());

        tracing::info!(
            "[BinanceClient] Calling cancel_all_orders_rpc for symbol {}...",
            symbol
        );
        let resp = match self.cancel_all_orders_rpc(params).await {
            Ok(r) => {
                tracing::info!("[BinanceClient] cancel_all_orders_rpc returned successfully");
                r
            }
            Err(e) => {
                tracing::error!("[BinanceClient] cancel_all_orders_rpc failed: {}", e);
                return Err(e);
            }
        };

        tracing::info!("[BinanceClient] Checking response status...");
        Self::ensure_ok(&resp)?;

        match resp {
            BinanceRpcResponse::CancelAll(_result) => {
                tracing::info!(
                    "[BinanceClient] cancel_all_orders succeeded for {}/{}",
                    pair.base,
                    pair.quote
                );
                Ok(())
            }
            BinanceRpcResponse::Error(msg) => {
                tracing::error!("[BinanceClient] cancel_all_orders RPC error: {}", msg);
                Err(EngineError::new_custom_error(msg.into()))
            }
            other => {
                tracing::error!(
                    "[BinanceClient] cancel_all_orders unexpected response variant: {:?}",
                    other
                );
                Err(EngineError::new_custom_error(
                    format!("unexpected RPC variant: {:?}", other).into(),
                ))
            }
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

    async fn amend_order(
        &self,
        order_id: OrderId,
        new_request: OrderRequest,
    ) -> Result<Order, EngineError> {
        // Get original order from state
        let original_order = if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
            state_mgr.get_order_by_id(&order_id).await.ok_or_else(|| {
                EngineError::new_custom_error(
                    format!("Order {} not found in state", order_id.as_str()).into(),
                )
            })?
        } else {
            return Err(EngineError::new_custom_error(
                "State manager not available".into(),
            ));
        };

        // 1. Cancel existing order
        self.cancel_order_by_id(order_id.clone()).await?;

        // 2. Mark original order as Amended in state
        if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
            let mut amended_order = original_order.clone();
            amended_order.status = OrderStatus::Amended;
            if let Err(e) = state_mgr.update_order(amended_order).await {
                warn!("Failed to mark order as amended: {}", e);
            }
        }

        // 3. Wait a bit for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 4. Place new order
        let new_order = self.open_order(new_request).await?;

        // 5. Create AmendedOrder record in state (if state manager supports it)
        // For now, just return the new order
        // TODO: Add AmendedOrder tracking to ExchangeStateManager if needed

        Ok(new_order)
    }
}

#[async_trait]
impl PrivateDataClient for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn stream(&mut self) -> Result<(), EngineError> {
        // Start both clients in separate tasks
        let streaming_client = Arc::clone(&self.streaming_client);
        let rpc_client = Arc::clone(&self.rpc_client);

        let streaming_handle = tokio::spawn(async move {
            if let Err(e) = streaming_client.start_streaming_loop().await {
                error!("Streaming client error: {:?}", e);
            }
        });

        let rpc_handle = tokio::spawn(async move {
            if let Err(e) = rpc_client.start_rpc_loop().await {
                error!("RPC client error: {:?}", e);
            }
        });

        // Wait for either to exit (shouldn't happen in normal operation)
        tokio::select! {
            result = streaming_handle => {
                error!("Streaming client task exited: {:?}", result);
            }
            result = rpc_handle => {
                error!("RPC client task exited: {:?}", result);
            }
        }

        Ok(())
    }

    fn shutdown(&self) {
        // Clients handle their own shutdown via connection drops
        // No explicit shutdown needed
    }
}
