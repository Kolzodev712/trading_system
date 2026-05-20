use crate::exchanges::binance::rest::client::BinanceHttpClient;
use crate::state::manager::ExchangeStateManager;
use smol_str::SmolStr;
use trading_core::{
    error::EngineError,
    rest::HttpMethod,
    types::{
        common::{OrderId, OrderStatus, Pair},
        exchange::{Balance, Order, OrderRequest, Trade},
    },
    utils::hours_ago_from,
    venue::cex::traits::TradingClient,
};

use crate::exchanges::binance::authenticator::BinanceAuthenticator;
use crate::exchanges::binance::config::BinanceConfig;
use crate::exchanges::binance::websocket::types::rpc::{
    amend_order::{map_amend_order_to_core, BinanceWsAmendOrderReqParams},
    cancel_all::BinanceWsCancelAllOrdersReqParams,
    cancel_order::BinanceWsCancelOrderReqParams,
    open_order::{map_open_order_to_core, BinanceWsOrderReqParams},
    BinanceRpcRequest, BinanceRpcResponse,
};
use crate::exchanges::binance::websocket::{
    rpc_client::BinanceRpcClient, streaming_client::BinanceStreamingClient,
    subscriber::BinanceSubscriptionHandler,
};
use async_trait::async_trait;
use std::{fmt::Debug, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, warn};
use trading_core::{types::common::Exchange, venue::cex::traits::PrivateDataClient};

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
        let signer = BinanceAuthenticator::new(config.api_key.clone(), config.secret.clone());
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

    /// Create an event loop for coordinating WebSocket connections
    ///
    /// This method creates an event loop that coordinates the RPC client and streaming client.
    /// The event loop handles connection management, reconnection, and event distribution.
    pub fn create_event_loop(
        &self,
        state_manager: Arc<tokio::sync::RwLock<Option<Arc<ExchangeStateManager>>>>,
    ) -> crate::exchanges::binance::event_loop::BinanceEventLoop {
        use crate::exchanges::binance::event_loop::BinanceEventLoop;
        BinanceEventLoop::new(
            self.config.clone(),
            Arc::clone(&self.rpc_client),
            Arc::clone(&self.streaming_client),
            state_manager,
            self.inbound_capacity,
            self.ping_capacity,
            self.outbound_rpc_capacity,
        )
    }

    // Stream coordination moved to event_loop.rs

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

    pub async fn amend_order_rpc(
        &self,
        params: BinanceWsAmendOrderReqParams,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let req = self
            .subscription_handler
            .create_amend_order_request(params)
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
            BinanceRpcResponse::AmendOrder(r) => {
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

                // Add order to ExchangeState immediately so execution reports can calculate fill deltas correctly
                if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                    if let Err(e) = state_mgr.update_order(order.clone()).await {
                        warn!(
                            "[BINANCE CLIENT] Failed to add order to ExchangeState after placement: {}",
                            e
                        );
                    }
                }

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
                    let symbol = order.instrument.symbol();
                    tracing::info!(
                        "Found order {} in state manager: symbol={}",
                        order_id.as_str(),
                        symbol.as_str()
                    );
                    (symbol, order.client_order_id.clone())
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

        // Request trades for the symbol (trades from past 24 hours, max 500)
        let params = BinanceTradesRequest {
            symbol,
            start_time: Some(hours_ago_from(server_time, 24)),
            end_time: None, // Use server_time implicitly
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
        use std::time::Duration;
        use tracing::warn;

        // Get original order from state to get symbol and determine what changed
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

        // Check if order can be amended (must be NEW or PARTIALLY_FILLED)
        if !matches!(
            original_order.status,
            OrderStatus::New | OrderStatus::PartiallyFilled
        ) {
            // Order is filled or cancelled - cannot amend, must cancel and place new
            warn!(
                "Order {} has status {:?}, cannot amend. Cancelling and placing new order.",
                order_id.as_str(),
                original_order.status
            );
            self.cancel_order_by_id(order_id.clone()).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
            return self.open_order(new_request).await;
        }

        // Check if side changed - if so, must cancel and place new
        // Note: Order struct doesn't store order_type, so we can't check that
        // If side changed, we must cancel and place new
        if original_order.side != new_request.side {
            warn!(
                "Order {} has different side, cannot amend. Cancelling and placing new order.",
                order_id.as_str()
            );
            self.cancel_order_by_id(order_id.clone()).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
            return self.open_order(new_request).await;
        }

        // Use native amend - only price and/or quantity changed
        let symbol: SmolStr = original_order.instrument.symbol();
        let params = if let Some(numeric_id) = order_id.as_numeric_id() {
            BinanceWsAmendOrderReqParams::new_with_order_id(
                symbol,
                numeric_id,
                Some(new_request.price),
                Some(new_request.amount),
            )
        } else if let Some(ref client_order_id) = original_order.client_order_id {
            BinanceWsAmendOrderReqParams::new_with_client_order_id(
                symbol,
                client_order_id.clone(),
                Some(new_request.price),
                Some(new_request.amount),
            )
        } else {
            // Fallback to cancel+place if we can't identify the order
            warn!(
                "Cannot identify order {} for amend (no numeric ID or client_order_id). Cancelling and placing new order.",
                order_id.as_str()
            );
            self.cancel_order_by_id(order_id.clone()).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
            return self.open_order(new_request).await;
        };

        let params_for_map = params.clone();
        let resp = self.amend_order_rpc(params).await?;
        Self::ensure_ok(&resp)?;

        match resp {
            BinanceRpcResponse::AmendOrder(ws_resp) => {
                if let Some(result) = ws_resp.result {
                    let order = map_amend_order_to_core(params_for_map, result)?;
                    Ok(order)
                } else if let Some(error) = ws_resp.error {
                    // If amend fails, fallback to cancel+place
                    warn!(
                        "Amend order failed with error: {} (code: {}). Falling back to cancel+place.",
                        error.msg,
                        error.code
                    );
                    self.cancel_order_by_id(order_id.clone()).await?;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    self.open_order(new_request).await
                } else {
                    Err(EngineError::new_custom_error(
                        "Amend order response missing result and error".into(),
                    ))
                }
            }
            BinanceRpcResponse::Error(msg) => {
                warn!(
                    "Amend order RPC error: {}. Falling back to cancel+place.",
                    msg
                );
                self.cancel_order_by_id(order_id.clone()).await?;
                tokio::time::sleep(Duration::from_millis(100)).await;
                self.open_order(new_request).await
            }
            other => Err(EngineError::new_custom_error(
                format!("unexpected RPC variant in amend_order: {other:?}").into(),
            )),
        }
    }

    async fn bulk_open_orders(
        &self,
        req: trading_core::types::exchange::BulkOrderRequest,
    ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
        // Binance doesn't support bulk operations, fall back to single operations
        let mut results = Vec::new();
        let mut success_count = 0;
        let mut failure_count = 0;

        for (idx, order_req) in req.requests.into_iter().enumerate() {
            match self.open_order(order_req).await {
                Ok(order) => {
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: true,
                        data: Some(order),
                        error: None,
                        index: idx,
                    });
                    success_count += 1;
                }
                Err(e) => {
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: false,
                        data: None,
                        error: Some(format!("{}", e)),
                        index: idx,
                    });
                    failure_count += 1;
                }
            }
        }

        Ok(trading_core::types::exchange::BulkOrderResponse {
            results,
            success_count,
            failure_count,
        })
    }

    async fn bulk_cancel_orders(
        &self,
        req: trading_core::types::exchange::BulkCancelRequest,
    ) -> Result<trading_core::types::exchange::BulkCancelResponse, EngineError> {
        // Binance doesn't support bulk operations, fall back to single operations
        let mut results = Vec::new();
        let mut cancelled_ids = Vec::new();
        let mut failed_ids = Vec::new();

        for (idx, order_id) in req.order_ids.into_iter().enumerate() {
            match self.cancel_order_by_id(order_id.clone()).await {
                Ok(_) => {
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: true,
                        data: Some(order_id.clone()),
                        error: None,
                        index: idx,
                    });
                    cancelled_ids.push(order_id);
                }
                Err(e) => {
                    let error_msg = format!("{}", e);
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: false,
                        data: None,
                        error: Some(error_msg.clone()),
                        index: idx,
                    });
                    failed_ids.push((order_id, error_msg));
                }
            }
        }

        Ok(trading_core::types::exchange::BulkCancelResponse {
            results,
            cancelled_ids,
            failed_ids,
        })
    }

    async fn bulk_amend_orders(
        &self,
        req: trading_core::types::exchange::BulkAmendRequest,
    ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
        // Binance doesn't support bulk operations, fall back to single operations
        let mut results = Vec::new();
        let mut amended_orders = Vec::new();
        let mut failed_amendments = Vec::new();

        for (idx, amendment) in req.amendments.into_iter().enumerate() {
            match self
                .amend_order(amendment.order_id.clone(), amendment.new_request)
                .await
            {
                Ok(order) => {
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: true,
                        data: Some(order.clone()),
                        error: None,
                        index: idx,
                    });
                    amended_orders.push(order);
                }
                Err(e) => {
                    let error_msg = format!("{}", e);
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: false,
                        data: None,
                        error: Some(error_msg.clone()),
                        index: idx,
                    });
                    failed_amendments.push((amendment.order_id, error_msg));
                }
            }
        }

        Ok(trading_core::types::exchange::BulkAmendResponse {
            results,
            amended_orders,
            failed_amendments,
        })
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
