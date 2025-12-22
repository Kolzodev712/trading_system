use crate::exchanges::binance::{
    config::BinanceConfig,
    rate_limit::{BinanceRateLimitType, BinanceRateLimiter},
    sign::BinanceWsAuthenticator,
    websocket::{
        client::{BinanceSubscriptionHandler, BinanceWSClient},
        types::rpc::{BinanceRpcRequest, BinanceRpcResponse},
    },
};
use futures_util::{SinkExt, StreamExt};
use smol_str::ToSmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, error, info, warn};
use trading_core::error::EngineError;

/// Pending request with timestamp for timeout cleanup
#[derive(Debug)]
pub(crate) struct PendingRequest {
    sender: oneshot::Sender<BinanceRpcResponse>,
    created_at: Instant,
}

/// RPC client for Binance - handles request/response operations only
/// This connection can be shorter-lived and reconnected independently
#[derive(Debug)]
pub struct BinanceRpcClient {
    #[allow(dead_code)]
    config: BinanceConfig,
    #[allow(dead_code)]
    ws_client: BinanceWSClient,
    #[allow(dead_code)]
    subscription_handler: BinanceSubscriptionHandler,
    pub(crate) rpc_request_tx: Arc<Mutex<Option<mpsc::Sender<BinanceRpcRequest>>>>,
    pub(crate) pending_rpc_requests: Arc<Mutex<HashMap<String, PendingRequest>>>,
    outbound_rpc_capacity: usize,
    ping_capacity: usize,
    rate_limiter: Arc<BinanceRateLimiter>,
}

impl BinanceRpcClient {
    pub fn new(config: BinanceConfig, outbound_rpc_capacity: usize, ping_capacity: usize) -> Self {
        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());
        let session = Arc::new(RwLock::new(None));
        let ws_client = BinanceWSClient::new(signer.clone(), session.clone());
        let subscription_handler = BinanceSubscriptionHandler::new(signer, session);

        let rate_limiter = Arc::new(BinanceRateLimiter::new());
        // Initialize with default limits
        let limiter_clone = rate_limiter.clone();
        tokio::spawn(async move {
            limiter_clone.initialize_defaults().await;
        });

        let client = Self {
            config,
            ws_client,
            subscription_handler,
            rpc_request_tx: Arc::new(Mutex::new(None)),
            pending_rpc_requests: Arc::new(Mutex::new(HashMap::new())),
            outbound_rpc_capacity,
            ping_capacity,
            rate_limiter,
        };

        // Start cleanup task for stale pending requests
        client.start_cleanup_task();

        client
    }

    /// Start background task to clean up stale pending requests
    fn start_cleanup_task(&self) {
        let pending = Arc::clone(&self.pending_rpc_requests);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                let mut guard = pending.lock().await;
                let now = Instant::now();
                let timeout = Duration::from_secs(35); // Clean up requests older than 35s (longer than 30s timeout)

                let mut to_remove = Vec::new();
                for (id, req) in guard.iter() {
                    if now.duration_since(req.created_at) > timeout {
                        to_remove.push(id.clone());
                    }
                }

                for id in to_remove {
                    if let Some(pending_req) = guard.remove(&id) {
                        debug!("Cleaning up stale pending request: {}", id);
                        let _ = pending_req.sender.send(BinanceRpcResponse::Error(
                            format!("Request timeout (cleaned up after {}s)", timeout.as_secs())
                                .into(),
                        ));
                    }
                }
            }
        });
    }

    pub async fn get_rpc_request_tx(&self) -> Option<mpsc::Sender<BinanceRpcRequest>> {
        self.rpc_request_tx.lock().await.clone()
    }

    /// Handle RPC responses from unified connection
    pub async fn run_rpc_response_handler(
        &self,
        mut rpc_response_rx: mpsc::Receiver<BinanceRpcResponse>,
    ) -> Result<(), EngineError> {
        info!("[RPC HANDLER] RPC response handler started");
        let mut response_count = 0u64;
        while let Some(response) = rpc_response_rx.recv().await {
            response_count += 1;

            // Extract and update rate limits from response
            self.update_rate_limits_from_response(&response).await;

            // Handle 429 errors
            if let Some(status) = response.get_status() {
                if status == 429 {
                    warn!("[RPC HANDLER] Rate limited (429) - handling retry");
                    // Extract retryAfter if available
                    if let Some(retry_after) = response.get_retry_after() {
                        let retry_after_ms = retry_after;
                        let retry_after_duration = std::time::Duration::from_millis(retry_after_ms);
                        warn!(
                            "[RPC HANDLER] Rate limited, waiting {}ms before retry",
                            retry_after_ms
                        );
                        tokio::time::sleep(retry_after_duration).await;
                    }
                }
            }

            let request_id_opt = response.get_id().map(|id| id.to_string());
            if let Some(request_id) = request_id_opt {
                tracing::info!(
                    "[RPC HANDLER] Received RPC response #{}: request_id={}, type={}",
                    response_count,
                    request_id,
                    ToSmolStr::to_smolstr(&response)
                );
                let mut guard = self.pending_rpc_requests.lock().await;
                if let Some(pending_req) = guard.remove(&request_id) {
                    let elapsed = pending_req.created_at.elapsed();
                    if pending_req.sender.send(response).is_err() {
                        tracing::warn!(
                            "[RPC HANDLER] Response receiver dropped for request {} (took {:?})",
                            request_id,
                            elapsed
                        );
                    } else {
                        tracing::info!("[RPC HANDLER] Successfully matched response to request {} (took {:?}, pending requests remaining: {})", request_id, elapsed, guard.len());
                    }
                } else {
                    tracing::warn!("[RPC HANDLER] No pending request found for response {} (pending requests: {:?})", request_id, guard.keys().collect::<Vec<_>>());
                }
            } else {
                tracing::warn!(
                    "[RPC HANDLER] Response #{} has no ID: {}",
                    response_count,
                    ToSmolStr::to_smolstr(&response)
                );
            }
        }
        info!("[RPC HANDLER] RPC response handler ended (channel closed)");
        Ok(())
    }

    /// Update rate limits from RPC response
    async fn update_rate_limits_from_response(&self, response: &BinanceRpcResponse) {
        let rate_limits = match response {
            BinanceRpcResponse::OpenOrder(resp) => resp.rate_limits.as_ref(),
            BinanceRpcResponse::CancelOrder(resp) => resp.rate_limits.as_ref(),
            BinanceRpcResponse::CancelAll(resp) => resp.rate_limits.as_ref(),
            BinanceRpcResponse::AmendOrder(resp) => resp.rate_limits.as_ref(),
            BinanceRpcResponse::AccountStatus(_) => None, // AccountStatus doesn't have rate_limits
            BinanceRpcResponse::TestOrder(resp) => resp.rate_limits.as_ref(),
            BinanceRpcResponse::Error(_) => None,
        };

        if let Some(limits) = rate_limits {
            if !limits.is_empty() {
                debug!(
                    "[RPC] Updating rate limits from response: {} limits",
                    limits.len()
                );
                self.rate_limiter.update_from_response(limits).await;
            }
        }
    }

    /// Send an RPC request and wait for the response
    pub async fn send_rpc_and_wait(
        &self,
        req: BinanceRpcRequest,
    ) -> Result<BinanceRpcResponse, EngineError> {
        let id = req.get_id().to_string();
        let (tx, rx) = oneshot::channel();

        tracing::info!(
            "[RPC] send_rpc_and_wait: request ID={}, method={:?}",
            id,
            req.get_method()
        );

        // Store pending request with timestamp
        {
            let mut guard = self.pending_rpc_requests.lock().await;
            guard.insert(
                id.clone(),
                PendingRequest {
                    sender: tx,
                    created_at: Instant::now(),
                },
            );
            tracing::info!(
                "[RPC] Stored pending request ID={}, total pending={}",
                id,
                guard.len()
            );
        }

        // Get RPC channel - wait for it to be available and open
        const MAX_RETRIES: u32 = 20;
        const RETRY_DELAY_MS: u64 = 50;

        let mut retries = 0;
        let rpc_tx = loop {
            // Check if channel is available
            let guard = self.rpc_request_tx.lock().await;
            let tx_opt = guard.as_ref().map(|tx| (tx.is_closed(), tx.clone()));
            drop(guard);

            match tx_opt {
                Some((false, tx)) => {
                    // Channel exists and is open - ready to use
                    debug!("[RPC] Channel is ready, using it for request {}", id);
                    break tx;
                }
                Some((true, _)) => {
                    // Channel exists but is closed
                    warn!("[RPC] Channel is closed (writer task may have died)");
                }
                None => {
                    // Channel not set yet
                    if retries == 0 {
                        info!("[RPC] Channel not set yet (connection may still be establishing), waiting...");
                    }
                }
            }

            // Check retry limit
            if retries >= MAX_RETRIES {
                let mut pending = self.pending_rpc_requests.lock().await;
                pending.remove(&id);
                let error_msg = match tx_opt {
                    Some((true, _)) => format!("RPC channel closed after {} retries (writer task may have died)", retries),
                    _ => format!("RPC channel not ready after {} retries (connection may not be established)", retries),
                };
                return Err(EngineError::new_custom_error(error_msg.into()));
            }

            // Log progress periodically
            if retries > 0 && retries % 5 == 0 {
                info!(
                    "[RPC] Still waiting for channel (attempt {}/{})",
                    retries + 1,
                    MAX_RETRIES
                );
            }

            retries += 1;
            tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
        };

        // Check rate limits before sending request
        let request_weight = self.get_request_weight(&req);
        let rate_limit_type = self.get_rate_limit_type(&req);

        self.rate_limiter
            .wait_if_needed(rate_limit_type.clone(), request_weight)
            .await;

        // Send request
        tracing::info!("[RPC] Sending request ID={} to RPC channel...", id);
        if rpc_tx.send(req).await.is_err() {
            tracing::error!(
                "[RPC] Failed to send request ID={} - RPC channel closed",
                id
            );
            // Remove from pending
            let mut pending = self.pending_rpc_requests.lock().await;
            pending.remove(&id);
            return Err(EngineError::new_custom_error("RPC channel closed".into()));
        }
        tracing::info!(
            "[RPC] Request ID={} sent successfully, waiting for response (30s timeout)...",
            id
        );

        // Record the request after sending
        self.rate_limiter
            .record(rate_limit_type, request_weight)
            .await;

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => {
                tracing::info!("[RPC] Received response for request ID={}", id);
                // Remove from pending
                let mut pending = self.pending_rpc_requests.lock().await;
                pending.remove(&id);
                Ok(response)
            }
            Ok(Err(_)) => {
                tracing::error!("[RPC] Response receiver dropped for request ID={}", id);
                // Remove from pending
                let mut pending = self.pending_rpc_requests.lock().await;
                pending.remove(&id);
                Err(EngineError::new_custom_error(
                    "Response receiver dropped".into(),
                ))
            }
            Err(_) => {
                tracing::error!(
                    "[RPC] Timeout waiting for response for request ID={} (30s timeout)",
                    id
                );
                // Remove from pending
                let mut pending = self.pending_rpc_requests.lock().await;
                pending.remove(&id);
                Err(EngineError::new_custom_error("RPC request timeout".into()))
            }
        }
    }

    /// Start the RPC connection loop
    pub async fn start_rpc_loop(&self) -> Result<(), EngineError> {
        info!("[RPC] Starting RPC loop");
        let mut backoff_iter = 0u32;
        let mut connection_id = 0u64;

        loop {
            connection_id += 1;
            info!(
                "[RPC {}] Starting RPC connection attempt {} (backoff_iter={})",
                connection_id, connection_id, backoff_iter
            );

            match self.connect_and_run_rpc().await {
                Ok(_) => {
                    info!(
                        "[RPC {}] RPC connection ended unexpectedly, reconnecting...",
                        connection_id
                    );
                }
                Err(e) => {
                    warn!(
                        "[RPC {}] RPC connection error: {:?}, reconnecting...",
                        connection_id, e
                    );
                }
            }

            // Reset rpc_request_tx and fail pending requests on disconnect
            {
                let mut guard = self.rpc_request_tx.lock().await;
                *guard = None;
            }
            {
                let mut guard = self.pending_rpc_requests.lock().await;
                Self::fail_all_pending(&mut guard);
            }

            // Exponential backoff
            self.exponential_backoff_wait(backoff_iter).await;
            backoff_iter = backoff_iter.saturating_add(1);
        }
    }

    async fn connect_and_run_rpc(&self) -> Result<(), EngineError> {
        info!("[RPC] Starting RPC connection...");

        // Create RPC channels
        let (rpc_request_tx, mut rpc_request_rx) =
            mpsc::channel::<BinanceRpcRequest>(self.outbound_rpc_capacity);
        let (_ping_tx, mut ping_rx) = mpsc::channel::<Vec<u8>>(self.ping_capacity);

        let mut ws = match self.ws_client.connect_and_authenticate().await {
            Ok(ws) => {
                info!("[RPC] Authentication successful");
                ws
            }
            Err(e) => {
                error!("[RPC] Authentication failed: {:?}", e);
                return Err(e);
            }
        };

        // Store rpc_request_tx AFTER successful connection
        {
            let mut guard = self.rpc_request_tx.lock().await;
            *guard = Some(rpc_request_tx.clone());
            info!("[RPC] RPC channel set to ready (connection established)");
        }

        info!("[RPC] RPC connection ready, starting read/write loop...");

        // Keepalive interval (30 seconds)
        let mut keepalive_interval = interval(Duration::from_secs(30));
        keepalive_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Main read/write loop
        loop {
            tokio::select! {
                // Handle incoming messages (responses)
                msg_result = ws.next() => {
                    match msg_result {
                        Some(Ok(Message::Text(text))) => {
                            // Parse RPC response
                            if let Ok(response) = self.parse_rpc_response(&text) {
                                let request_id_opt = response.get_id().map(|id| id.to_string());
                                if let Some(request_id) = request_id_opt {
                                    let mut guard = self.pending_rpc_requests.lock().await;
                                    if let Some(pending_req) = guard.remove(&request_id) {
                                        if pending_req.sender.send(response).is_err() {
                                            warn!("[RPC] Response receiver dropped for request {}", request_id);
                                        } else {
                                            debug!("[RPC] Matched response to request {}", request_id);
                                        }
                                    } else {
                                        warn!("[RPC] No pending request found for response {}", request_id);
                                    }
                                } else {
                                    warn!("[RPC] Response has no ID: {}", text);
                                }
                            } else {
                                debug!("[RPC] Received non-RPC message: {}", text);
                            }
                        }
                        Some(Ok(Message::Ping(ping))) => {
                            if let Err(e) = ws.send(Message::Pong(ping)).await {
                                error!("[RPC] Failed to send Pong: {:?}", e);
                                break;
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("[RPC] Received Pong");
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("[RPC] Received Close frame, connection closed");
                            break;
                        }
                        Some(Ok(Message::Binary(_))) => {
                            debug!("[RPC] Received binary message");
                        }
                        Some(Err(e)) => {
                            error!("[RPC] WebSocket error: {:?}", e);
                            break;
                        }
                        None => {
                            info!("[RPC] WebSocket stream ended");
                            break;
                        }
                    }
                }
                // Handle outgoing RPC requests
                Some(request) = rpc_request_rx.recv() => {
                    let request_id = request.get_id().to_string();
                    let message = request.to_message()?;

                    // Store pending request (oneshot sender will be used when response arrives)
                    let (tx, _rx) = oneshot::channel();
                    {
                        let mut guard = self.pending_rpc_requests.lock().await;
                        guard.insert(request_id.clone(), PendingRequest {
                            sender: tx,
                            created_at: Instant::now(),
                        });
                    }

                    // Send request
                    if let Err(e) = ws.send(message).await {
                        error!("[RPC] Failed to send RPC request {}: {:?}", request_id, e);
                        // Remove from pending and send error response
                        {
                            let mut guard = self.pending_rpc_requests.lock().await;
                            if let Some(pending_req) = guard.remove(&request_id) {
                                let _ = pending_req.sender.send(BinanceRpcResponse::Error(format!("Failed to send: {:?}", e)));
                            }
                        }
                        break;
                    }
                    debug!("[RPC] Sent request {}, waiting for response...", request_id);
                }
                // Handle ping requests
                Some(ping) = ping_rx.recv() => {
                    if let Err(e) = ws.send(Message::Pong(ping)).await {
                        error!("[RPC] Failed to send Pong: {:?}", e);
                        break;
                    }
                }
                // Proactive keepalive
                _ = keepalive_interval.tick() => {
                    if let Err(e) = ws.send(Message::Ping(vec![])).await {
                        error!("[RPC] Failed to send proactive Ping: {:?}", e);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_rpc_response(&self, text: &str) -> Result<BinanceRpcResponse, EngineError> {
        // Try to parse as different RPC response types
        use crate::exchanges::binance::websocket::types::rpc::{
            balance::BinanceAccountStatusResponse, cancel_all::BinanceWsCancelAllOrdersResponse,
            cancel_order::BinanceWsCancelOrderResponse, open_order::BinanceWsOrderResponse,
        };

        if let Ok(resp) = serde_json::from_str::<BinanceAccountStatusResponse>(text) {
            return Ok(BinanceRpcResponse::AccountStatus(resp));
        }
        if let Ok(resp) = serde_json::from_str::<BinanceWsOrderResponse>(text) {
            return Ok(BinanceRpcResponse::OpenOrder(resp));
        }
        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelOrderResponse>(text) {
            return Ok(BinanceRpcResponse::CancelOrder(resp));
        }
        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelAllOrdersResponse>(text) {
            return Ok(BinanceRpcResponse::CancelAll(resp));
        }

        Err(EngineError::new_custom_error(
            format!("Failed to parse RPC response: {}", text).into(),
        ))
    }

    fn fail_all_pending(guard: &mut HashMap<String, PendingRequest>) {
        for (request_id, pending_req) in guard.drain() {
            // Create error response
            let error_response =
                BinanceRpcResponse::Error(format!("Connection dropped for request {}", request_id));
            let _ = pending_req.sender.send(error_response);
        }
    }

    async fn exponential_backoff_wait(&self, iteration: u32) {
        let base_delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);
        let delay = std::cmp::min(base_delay * 2_u32.pow(iteration.min(6)), max_delay);
        warn!("[RPC] Waiting {:?} before reconnecting...", delay);
        tokio::time::sleep(delay).await;
    }

    /// Get request weight for rate limiting
    /// WebSocket API: connecting costs 2 weight, most requests cost 1 weight
    fn get_request_weight(&self, req: &BinanceRpcRequest) -> u64 {
        match req {
            BinanceRpcRequest::AccountStatus(_) => 1,
            BinanceRpcRequest::TestOrder(_) => 1,
            BinanceRpcRequest::OpenOrder(_) => 1,
            BinanceRpcRequest::CancelOrder(_) => 1,
            BinanceRpcRequest::CancelAll(_) => 1,
            BinanceRpcRequest::AmendOrder(_) => 1,
        }
    }

    /// Get rate limit type for a request
    /// Order operations use ORDERS limit, others use REQUEST_WEIGHT
    fn get_rate_limit_type(&self, req: &BinanceRpcRequest) -> BinanceRateLimitType {
        match req {
            BinanceRpcRequest::OpenOrder(_)
            | BinanceRpcRequest::TestOrder(_)
            | BinanceRpcRequest::AmendOrder(_) => BinanceRateLimitType::Orders,
            BinanceRpcRequest::CancelOrder(_) | BinanceRpcRequest::CancelAll(_) => {
                BinanceRateLimitType::Orders
            }
            BinanceRpcRequest::AccountStatus(_) => BinanceRateLimitType::RequestWeight,
        }
    }
}
