//! k-paper WebSocket client: connects to k-paper, maps messages to channels, implements TradingClient.
//! REST is used for fetch_balance, fetch_open_orders, fetch_trades (reconciliation).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client as HttpClient;
use rust_decimal::Decimal;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};
use trading_core::channels::market_data::{MarketDataUpdate, MarketDataUpdateSender};
use trading_core::error::EngineError;
use trading_core::types::common::{
    Exchange, Instrument, InstrumentType, OrderId, OrderStatus, Pair, Side,
};
use trading_core::types::exchange::{
    Balance, BulkAmendRequest, BulkAmendResponse, BulkOrderRequest, BulkOrderResponse,
    ClientOrderLookup, Order, OrderRequest, Trade,
};
use trading_core::venue::cex::traits::{ConnectionState, TradingClient};

use super::config::PaperConfig;
use super::map::{
    balance_to_core, cancel_all_after_rest, execution_ack_to_order,
    execution_may_complete_pending_place, execution_to_order, execution_to_trade,
    fill_trade_id_is_new, merge_fill_into_working_order, orderbook_to_structured, position_to_core,
    public_trade_to_structured, to_instrument_id,
};
use super::messages::{KPaperCommand, KPaperMessage, KPaperPlaceOrderRequest};
use super::rest::{
    rest_balance_to_core, rest_order_to_core, rest_trade_to_core, ws_url_to_http_base, RestBalance,
    RestOrder, RestTrade,
};

/// Config for k-paper connection (internal use).
#[derive(Debug, Clone)]
pub struct KPaperConfig {
    pub url: String,
    pub pairs: Vec<Pair>,
    pub instrument_type: InstrumentType,
    pub leverage: Option<u32>,
}

impl From<PaperConfig> for KPaperConfig {
    fn from(c: PaperConfig) -> Self {
        KPaperConfig {
            url: c.paper_exchange_url,
            pairs: c.pairs,
            instrument_type: c.instrument_type,
            leverage: c.leverage,
        }
    }
}

/// Shared state for the recv loop and TradingClient.
#[derive(Default)]
pub struct KPaperState {
    balances: Vec<Balance>,
    positions: Vec<trading_core::types::exchange::Position>,
    open_orders: HashMap<OrderId, Order>,
    trades: Vec<Trade>,
    /// Fill trade ids already merged into `open_orders` filled_amount.
    applied_fill_ids: HashMap<OrderId, HashSet<String>>,
    /// Single in-flight place_order. Completed only by a correlated ack/reject, never by a fill.
    pending_place: Option<(oneshot::Sender<Result<Order, EngineError>>, OrderRequest)>,
    /// Single in-flight cancel: fulfilled when we get CancelAck.
    pending_cancel: Option<oneshot::Sender<Result<(), EngineError>>>,
    /// Single in-flight amend: fulfilled when we get AmendAck.
    pending_amend: Option<oneshot::Sender<Result<Order, EngineError>>>,
}

/// k-paper client: implements TradingClient. Exchange updates are driven via run_websocket_loop (caller passes ExchangeStateManager).
/// REST base URL is used for fetch_balance, fetch_open_orders, fetch_trades.
pub struct KPaperClient {
    config: KPaperConfig,
    command_tx: mpsc::UnboundedSender<KPaperCommand>,
    state: Arc<RwLock<KPaperState>>,
    rest_base: String,
    http_client: HttpClient,
}

impl std::fmt::Debug for KPaperClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KPaperClient")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl KPaperClient {
    /// Create a new k-paper client. Start the recv loop via run_websocket_loop (e.g. from PaperDriver).
    /// REST base URL is derived from the WebSocket URL for reconciliation endpoints.
    pub fn new(config: KPaperConfig) -> (Self, mpsc::UnboundedReceiver<KPaperCommand>) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let rest_base = ws_url_to_http_base(&config.url);
        let client = Self {
            config: config.clone(),
            command_tx: command_tx.clone(),
            state: Arc::new(RwLock::new(KPaperState::default())),
            rest_base: rest_base.clone(),
            http_client: HttpClient::new(),
        };
        (client, command_rx)
    }

    /// Get a clone of state for the recv task.
    pub fn state(&self) -> Arc<RwLock<KPaperState>> {
        Arc::clone(&self.state)
    }

    /// Get config for the recv task.
    pub fn config(&self) -> &KPaperConfig {
        &self.config
    }

    /// Send a command (used by TradingClient methods).
    fn send_command(&self, cmd: KPaperCommand) -> Result<(), EngineError> {
        self.command_tx.send(cmd).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("k-paper command channel closed: {}", e))
        })
    }
}

#[async_trait]
impl TradingClient<OrderRequest> for KPaperClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Paper
    }

    async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
        Ok(())
    }

    async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.state.write().await;
            if state.pending_place.is_some() {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: only one in-flight place_order supported"
                )));
            }
            state.pending_place = Some((tx, req.clone()));
        }

        let instrument_id = to_instrument_id(&req.instrument.pair);
        let side = format!("{}", req.side);
        let order_type = match req.order_type {
            trading_core::types::common::OrderType::Limit => "Limit",
            trading_core::types::common::OrderType::Market => "Market",
            _ => "Limit",
        };
        let quantity = req.amount;
        let limit_price = match req.order_type {
            trading_core::types::common::OrderType::Market => None,
            _ => Some(req.price),
        };
        let client_order_id = req
            .exchange_params
            .get("orderLinkId")
            .map(|id| vec![id.to_string()])
            .or_else(|| Some(vec![uuid::Uuid::new_v4().to_string()]));

        let reduce_only = req
            .exchange_params
            .get("reduceOnly")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let cmd = KPaperCommand::PlaceOrder {
            request: KPaperPlaceOrderRequest {
                instrument_id: instrument_id.to_string(),
                side,
                order_type: order_type.to_string(),
                quantity,
                limit_price,
                client_order_id,
                reduce_only,
            },
        };
        self.send_command(cmd)?;

        let timeout_dur = tokio::time::Duration::from_secs(15);
        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(Ok(order))) => Ok(order),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(EngineError::CustomError(anyhow::anyhow!(
                "k-paper: pending place dropped"
            ))),
            Err(_) => {
                let mut state = self.state.write().await;
                state.pending_place = None;
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: open_order timeout (15s)"
                )))
            }
        }
    }

    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.state.write().await;
            if state.pending_cancel.is_some() {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: only one in-flight cancel supported"
                )));
            }
            state.pending_cancel = Some(tx);
        }
        self.send_command(KPaperCommand::CancelOrder {
            order_id: order_id.as_str().to_string(),
        })?;
        let timeout_dur = tokio::time::Duration::from_secs(10);
        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                let mut state = self.state.write().await;
                state.pending_cancel = None;
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: cancel ack channel dropped"
                )))
            }
            Err(_) => {
                let mut state = self.state.write().await;
                state.pending_cancel = None;
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: cancel_order timeout (10s) waiting for CancelAck"
                )))
            }
        }
    }

    async fn cancel_all_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        let mut order_ids = HashSet::new();
        {
            let state = self.state.read().await;
            for order in state.open_orders.values() {
                if order.instrument.pair == *pair {
                    order_ids.insert(order.order_id.clone());
                }
            }
        }
        let rest_ok = match self.fetch_open_orders().await {
            Ok(rest) => {
                for order in rest {
                    if order.instrument.pair == *pair {
                        order_ids.insert(order.order_id);
                    }
                }
                true
            }
            Err(e) => {
                warn!("k-paper: REST open_orders for cancel_all failed: {e}");
                false
            }
        };
        let mut first_err = None;
        for id in order_ids {
            if let Err(e) = self.cancel_order_by_id(id).await {
                if first_err.is_none() {
                    first_err = Some(e.to_string());
                }
            }
        }
        match cancel_all_after_rest(rest_ok, first_err) {
            Ok(()) => {}
            Err(msg) => return Err(EngineError::CustomError(anyhow::anyhow!(msg))),
        }
        let remaining = self.fetch_open_orders().await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!(
                "CancelWorking unverified: REST GET /open_orders after cancel failed: {e}"
            ))
        })?;
        let still: Vec<_> = remaining
            .iter()
            .filter(|order| order.instrument.pair == *pair && order.is_working())
            .map(|order| order.order_id.as_str().to_string())
            .collect();
        if !still.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "CancelWorking unverified: working orders remain after cancel ({})",
                still.join(",")
            )));
        }
        Ok(())
    }

    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        let url = format!("{}/balances", self.rest_base);
        let res =
            self.http_client.get(&url).send().await.map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("REST GET /balances: {}", e))
            })?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /balances: {} {}",
                status,
                body
            )));
        }
        let rest_balances: Vec<RestBalance> = res
            .json()
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("REST balances JSON: {}", e)))?;
        Ok(rest_balances.iter().map(rest_balance_to_core).collect())
    }

    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        let url = format!("{}/open_orders", self.rest_base);
        let res = self.http_client.get(&url).send().await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("REST GET /open_orders: {}", e))
        })?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /open_orders: {} {}",
                status,
                body
            )));
        }
        let rest_orders: Vec<RestOrder> = res.json().await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("REST open_orders JSON: {}", e))
        })?;
        let instrument_type = self.config.instrument_type.clone();
        Ok(rest_orders
            .iter()
            .filter_map(|o| rest_order_to_core(o, instrument_type.clone()))
            .collect())
    }

    async fn fetch_order_by_client_id(
        &self,
        client_order_id: &str,
    ) -> Result<ClientOrderLookup, EngineError> {
        let url = format!(
            "{}/orders/by_client_id/{}",
            self.rest_base,
            urlencoding::encode(client_order_id)
        );
        let res = match self.http_client.get(&url).send().await {
            Ok(res) => res,
            Err(err) => {
                warn!("k-paper GET /orders/by_client_id failed: {err}");
                return Ok(ClientOrderLookup::Inconclusive);
            }
        };
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(ClientOrderLookup::Inconclusive);
        }
        if !res.status().is_success() {
            warn!(
                "k-paper GET /orders/by_client_id: {} {}",
                res.status(),
                res.text().await.unwrap_or_default()
            );
            return Ok(ClientOrderLookup::Inconclusive);
        }
        let rest_order: RestOrder = match res.json().await {
            Ok(order) => order,
            Err(err) => {
                warn!("k-paper order-by-client JSON: {err}");
                return Ok(ClientOrderLookup::Inconclusive);
            }
        };
        match rest_order_to_core(&rest_order, self.config.instrument_type.clone()) {
            Some(order) => Ok(ClientOrderLookup::from_order(order)),
            None => Ok(ClientOrderLookup::Inconclusive),
        }
    }

    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        let mut url = format!("{}/trades", self.rest_base);
        if let Some(p) = pair {
            let instrument_id = format!("{}-{}", p.base, p.quote);
            url.push_str(&format!(
                "?instrument_id={}",
                urlencoding::encode(&instrument_id)
            ));
        }
        let res =
            self.http_client.get(&url).send().await.map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("REST GET /trades: {}", e))
            })?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /trades: {} {}",
                status,
                body
            )));
        }
        let rest_trades: Vec<RestTrade> = res
            .json()
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("REST trades JSON: {}", e)))?;
        Ok(rest_trades.iter().filter_map(rest_trade_to_core).collect())
    }

    async fn fetch_positions(
        &self,
    ) -> Result<Vec<trading_core::types::exchange::Position>, EngineError> {
        let url = format!("{}/positions", self.rest_base);
        let res =
            self.http_client.get(&url).send().await.map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("REST GET /positions: {}", e))
            })?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /positions: {} {}",
                status,
                body
            )));
        }
        let rest_positions: Vec<crate::exchanges::k_paper::rest::RestPosition> =
            res.json().await.map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("REST positions JSON: {}", e))
            })?;
        Ok(rest_positions
            .iter()
            .filter_map(|p| {
                crate::exchanges::k_paper::rest::rest_position_to_core(
                    p,
                    self.config.instrument_type.clone(),
                )
            })
            .collect())
    }

    async fn set_leverage(
        &self,
        _pair: &trading_core::types::common::Pair,
        leverage: u32,
    ) -> Result<trading_core::types::futures::LeverageInfo, EngineError> {
        match self.config.leverage {
            Some(expected) if expected == leverage => {
                Ok(trading_core::types::futures::LeverageInfo {
                    pair: _pair.clone(),
                    leverage,
                    max_leverage: leverage,
                    last_update: 0,
                })
            }
            Some(expected) => Err(EngineError::CustomError(anyhow::anyhow!(
                "k-paper leverage {expected}x does not match requested {leverage}x"
            ))),
            None => Err(EngineError::CustomError(anyhow::anyhow!(
                "k-paper futures leverage must be set explicitly in bot.json"
            ))),
        }
    }

    async fn set_position_mode(
        &self,
        mode: trading_core::types::futures::PositionMode,
    ) -> Result<(), EngineError> {
        if !matches!(mode, trading_core::types::futures::PositionMode::OneWay) {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "k-paper linear contract is one-way only"
            )));
        }
        Ok(())
    }

    async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
        Ok(())
    }

    async fn amend_order(
        &self,
        order_id: OrderId,
        new_request: OrderRequest,
    ) -> Result<Order, EngineError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.state.write().await;
            if state.pending_amend.is_some() {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: only one in-flight amend supported"
                )));
            }
            state.pending_amend = Some(tx);
        }
        self.send_command(KPaperCommand::AmendOrder {
            order_id: order_id.as_str().to_string(),
            quantity: Some(new_request.amount),
            limit_price: Some(new_request.price),
        })?;
        let timeout_dur = tokio::time::Duration::from_secs(10);
        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(Ok(order))) => Ok(order),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                let mut state = self.state.write().await;
                state.pending_amend = None;
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: amend ack channel dropped"
                )))
            }
            Err(_) => {
                let mut state = self.state.write().await;
                state.pending_amend = None;
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "k-paper: amend_order timeout (10s) waiting for AmendAck"
                )))
            }
        }
    }

    /// No native bulk; k-paper does not support bulk. Fallback: single-op loop (trait requirement).
    async fn bulk_open_orders(
        &self,
        req: BulkOrderRequest,
    ) -> Result<BulkOrderResponse, EngineError> {
        let mut results = Vec::new();
        let mut success_count = 0usize;
        let mut failure_count = 0usize;
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
                        error: Some(e.to_string()),
                        index: idx,
                    });
                    failure_count += 1;
                }
            }
        }
        Ok(BulkOrderResponse {
            results,
            success_count,
            failure_count,
        })
    }

    /// No native bulk; k-paper does not support bulk. Fallback: single-op loop (trait requirement).
    async fn bulk_amend_orders(
        &self,
        req: BulkAmendRequest,
    ) -> Result<BulkAmendResponse, EngineError> {
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
                    let msg = e.to_string();
                    results.push(trading_core::types::exchange::BulkOperationResult {
                        success: false,
                        data: None,
                        error: Some(msg.clone()),
                        index: idx,
                    });
                    failed_amendments.push((amendment.order_id, msg));
                }
            }
        }
        Ok(BulkAmendResponse {
            results,
            amended_orders,
            failed_amendments,
        })
    }
}

/// Optional signal fired when the first balance message is received (so callers can wait for snapshot).
pub type BalanceReadySender =
    Option<Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>>;

/// Run the k-paper WebSocket loop: connect, then process incoming messages and outgoing commands.
/// Pass None for market_data_tx when this connection is exchange-only (MD comes from market_data_engine).
/// If balance_ready is Some, it is signalled when the first balance message is processed.
async fn fail_inflight(state: &Arc<RwLock<KPaperState>>, reason: &str) {
    let mut st = state.write().await;
    if let Some((tx, _)) = st.pending_place.take() {
        let _ = tx.send(Err(EngineError::CustomError(anyhow::anyhow!("{reason}"))));
    }
    if let Some(tx) = st.pending_cancel.take() {
        let _ = tx.send(Err(EngineError::CustomError(anyhow::anyhow!("{reason}"))));
    }
    if let Some(tx) = st.pending_amend.take() {
        let _ = tx.send(Err(EngineError::CustomError(anyhow::anyhow!("{reason}"))));
    }
}

async fn set_connection_state(
    connection_state: &Option<Arc<RwLock<ConnectionState>>>,
    next: ConnectionState,
) {
    if let Some(cs) = connection_state {
        *cs.write().await = next;
    }
}

/// Inputs for the k-paper trading WebSocket reconnect loop.
pub struct KPaperWebsocketLoop {
    pub config: KPaperConfig,
    pub command_rx: mpsc::UnboundedReceiver<KPaperCommand>,
    pub state: Arc<RwLock<KPaperState>>,
    pub market_data_tx: Option<MarketDataUpdateSender>,
    pub exchange_state_manager: Arc<crate::state::manager::ExchangeStateManager>,
    pub shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    pub balance_ready: BalanceReadySender,
    pub connection_state: Option<Arc<RwLock<ConnectionState>>>,
}

/// Trading WS loop. Reconnects after a drop so a lab disconnect cannot leave a stale Connected state.
pub async fn run_websocket_loop(
    args: KPaperWebsocketLoop,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let KPaperWebsocketLoop {
        config,
        mut command_rx,
        state,
        market_data_tx,
        exchange_state_manager,
        mut shutdown_rx,
        balance_ready,
        connection_state,
    } = args;
    let snapshot_cmd = serde_json::to_string(&KPaperCommand::GetAccountSnapshot)
        .map_err(|e| format!("serialize get_account_snapshot: {}", e))?;
    let instrument_type = config.instrument_type.clone();

    loop {
        set_connection_state(&connection_state, ConnectionState::Connecting).await;
        info!("k-paper: connecting to {}", config.url);
        let ws_stream = tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("k-paper: shutdown signal received");
                set_connection_state(&connection_state, ConnectionState::Disconnected).await;
                return Ok(());
            }
            result = tokio_tungstenite::connect_async(&config.url) => {
                match result {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        error!("k-paper: connect failed: {}", e);
                        fail_inflight(&state, "k-paper: trade WS connect failed").await;
                        set_connection_state(&connection_state, ConnectionState::Disconnected).await;
                        tokio::select! {
                            _ = shutdown_rx.recv() => {
                                set_connection_state(&connection_state, ConnectionState::Disconnected).await;
                                return Ok(());
                            }
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => continue,
                        }
                    }
                }
            }
        };
        info!("k-paper: connected to {}", config.url);
        set_connection_state(&connection_state, ConnectionState::Connected).await;
        let (mut write, mut read) = ws_stream.split();
        let mut recv_count: u64 = 0;
        let mut snapshot_sent = false;

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("k-paper: shutdown signal received");
                    set_connection_state(&connection_state, ConnectionState::Disconnected).await;
                    return Ok(());
                }
                msg = read.next() => {
                    let msg = match msg {
                        Some(Ok(Message::Text(text))) => text,
                        Some(Ok(Message::Close(_))) => {
                            info!("k-paper: server closed connection");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("k-paper: WebSocket error: {}", e);
                            break;
                        }
                        Some(Ok(_)) => continue,
                        None => break,
                    };
                    recv_count += 1;
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg) {
                        let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                        if recv_count == 1 {
                            info!("k-paper exchange: first message received (type={}), read loop active", typ);
                        }
                        tracing::debug!("k-paper exchange: recv #{} type={}", recv_count, typ);
                    }
                    if let Err(e) = handle_message(
                        &msg,
                        &state,
                        market_data_tx.as_ref(),
                        &exchange_state_manager,
                        &instrument_type,
                        balance_ready.as_ref(),
                    ).await {
                        warn!("k-paper: handle_message error: {} (raw type from JSON may not match)", e);
                    }
                    if !snapshot_sent {
                        snapshot_sent = true;
                        if let Err(e) = write.send(Message::Text(snapshot_cmd.clone())).await {
                            error!("k-paper: send get_account_snapshot: {}", e);
                            break;
                        }
                        info!("k-paper: sent get_account_snapshot (after first read)");
                    }
                }
                cmd = command_rx.recv() => {
                    let cmd = match cmd {
                        Some(c) => c,
                        None => {
                            set_connection_state(&connection_state, ConnectionState::Disconnected).await;
                            return Ok(());
                        }
                    };
                    if let KPaperCommand::PlaceOrder { request } = &cmd {
                        info!(
                            "k-paper: sent place_order instrument={} side={} qty={:?}",
                            request.instrument_id, request.side, request.quantity
                        );
                    }
                    let json = serde_json::to_string(&cmd).map_err(|e| format!("serialize: {}", e))?;
                    if let Err(e) = write.send(Message::Text(json)).await {
                        error!("k-paper: send: {}", e);
                        break;
                    }
                }
            }
        }

        fail_inflight(&state, "k-paper: trade WS disconnected").await;
        set_connection_state(&connection_state, ConnectionState::Disconnected).await;
        info!("k-paper: reconnecting trade WS in 2s");
        tokio::select! {
            _ = shutdown_rx.recv() => {
                set_connection_state(&connection_state, ConnectionState::Disconnected).await;
                return Ok(());
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {}
        }
    }
}

async fn handle_message(
    text: &str,
    state: &Arc<RwLock<KPaperState>>,
    market_data_tx: Option<&MarketDataUpdateSender>,
    exchange_state_manager: &crate::state::manager::ExchangeStateManager,
    instrument_type: &InstrumentType,
    balance_ready: Option<&Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let msg: KPaperMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            let raw_type = serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|v| v.get("type").cloned());
            warn!(
                "k-paper: failed to parse message (type={:?}): {}",
                raw_type, e
            );
            return Err(e.into());
        }
    };
    match msg {
        KPaperMessage::Connected(_) => {
            info!("k-paper: received connected (WebSocket live)");
        }
        KPaperMessage::Orderbook(ob) => {
            if let Some(tx) = market_data_tx {
                if let Some(book) = orderbook_to_structured(&ob) {
                    let _ = tx.send(MarketDataUpdate::Orderbook {
                        orderbook: book,
                        trace_context: None,
                    });
                }
            }
        }
        KPaperMessage::PublicTrade(t) => {
            if let Some(tx) = market_data_tx {
                if let Some(trade) = public_trade_to_structured(&t) {
                    let _ = tx.send(MarketDataUpdate::Trade {
                        trade,
                        trace_context: None,
                    });
                }
            }
        }
        KPaperMessage::Execution(exec) => {
            info!(
                "k-paper: received Execution order_id={} instrument_id={}",
                exec.order_id, exec.instrument_id
            );
            let exec_qty = exec.quantity;
            let is_reject = exec
                .reject_reason
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let is_ack = !is_reject && exec_qty.is_zero();

            let mut st = state.write().await;
            let completes_place = st
                .pending_place
                .as_ref()
                .is_some_and(|(_, req)| execution_may_complete_pending_place(&exec, req));
            let (order, pending_tx) = if is_ack {
                // Order accepted (quantity=0): build Order from ack + stored request
                if completes_place {
                    if let Some((tx, req)) = st.pending_place.take() {
                        let order = execution_ack_to_order(&exec, &req, instrument_type.clone());
                        (order, Some(tx))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            } else {
                let order_id = OrderId::new(exec.order_id.clone().into());
                let duplicate_fill = if !is_reject {
                    let applied = st.applied_fill_ids.entry(order_id.clone()).or_default();
                    !fill_trade_id_is_new(applied, exec.trade_id.as_deref())
                } else {
                    false
                };
                if duplicate_fill {
                    info!(
                        "k-paper: ignoring duplicate fill order_id={} trade_id={:?}",
                        exec.order_id, exec.trade_id
                    );
                }
                let incoming = execution_to_order(&exec, instrument_type.clone());
                let order = if duplicate_fill {
                    None
                } else {
                    match incoming {
                        Some(fill_order)
                            if !is_reject && st.open_orders.contains_key(&order_id) =>
                        {
                            Some(merge_fill_into_working_order(
                                st.open_orders.get(&order_id).expect("contains_key"),
                                fill_order.filled_amount,
                            ))
                        }
                        other => other,
                    }
                };
                let pending_tx = if completes_place {
                    st.pending_place.take().map(|(tx, _)| tx)
                } else {
                    if st.pending_place.is_some() && !is_reject && exec_qty > Decimal::ZERO {
                        info!(
                            "k-paper: fill does not complete in-flight place order_id={}",
                            exec.order_id
                        );
                    }
                    None
                };
                (order, pending_tx)
            };

            if let Some(ref o) = order {
                let _ = exchange_state_manager.update_order(o.clone()).await;
            }
            let order_id = OrderId::new(exec.order_id.clone().into());
            if let Some(o) = order.clone() {
                if is_reject || o.status == trading_core::types::common::OrderStatus::Filled {
                    st.open_orders.remove(&order_id);
                    st.applied_fill_ids.remove(&order_id);
                } else {
                    st.open_orders.insert(order_id.clone(), o);
                }
            }
            // Only add trade for fills (not for ack with quantity=0)
            if !is_ack {
                if let Some(t) = execution_to_trade(&exec) {
                    let _ = exchange_state_manager.add_trade(t.clone()).await;
                    if !st
                        .trades
                        .iter()
                        .any(|prev| prev.trade_id == t.trade_id && !t.trade_id.as_str().is_empty())
                    {
                        st.trades.push(t);
                    }
                }
            }
            if let Some(tx) = pending_tx {
                let result = order.map(Ok).unwrap_or_else(|| {
                    Err(EngineError::CustomError(anyhow::anyhow!(
                        "{}",
                        exec.reject_reason.as_deref().unwrap_or("rejected")
                    )))
                });
                let _ = tx.send(result);
            }
        }
        KPaperMessage::Balance(b) => {
            let balance = balance_to_core(&b);
            info!(
                "k-paper: balance update {} free={} used={} total={}",
                balance.symbol, balance.free, balance.used, balance.total
            );
            let _ = exchange_state_manager.update_balance(balance.clone()).await;
            let mut st = state.write().await;
            if let Some(pos) = st.balances.iter().position(|x| x.symbol == balance.symbol) {
                st.balances[pos] = balance;
            } else {
                st.balances.push(balance);
            }
            // Signal after state is updated so waiters see balances
            if let Some(guard) = balance_ready {
                if let Some(tx) = guard.lock().await.take() {
                    let _ = tx.send(());
                }
            }
        }
        KPaperMessage::Position(pos) => {
            if let Some(p) = position_to_core(&pos, instrument_type.clone()) {
                let _ = exchange_state_manager.update_position(p.clone()).await;
                let mut st = state.write().await;
                let key = p.instrument.pair.clone();
                st.positions.retain(|x| x.instrument.pair != key);
                if p.quantity.abs() > Decimal::new(1, 6) {
                    st.positions.push(p);
                }
            }
        }
        KPaperMessage::CancelAck(ack) => {
            let order_id = OrderId::new(ack.order_id.clone().into());
            let mut st = state.write().await;
            let cancelled_order = st.open_orders.get(&order_id).map(|o| {
                let mut ord = o.clone();
                ord.status = OrderStatus::Canceled;
                ord
            });
            st.open_orders.remove(&order_id);
            st.applied_fill_ids.remove(&order_id);
            if let Some(pending) = st.pending_cancel.take() {
                let _ = pending.send(Ok(()));
            }
            drop(st);
            if let Some(order) = cancelled_order {
                let _ = exchange_state_manager.update_order(order).await;
            }
        }
        KPaperMessage::AmendAck(ack) => {
            let order_id = OrderId::new(ack.order_id.clone().into());
            let mut st = state.write().await;
            let order = st.open_orders.get(&order_id).cloned().unwrap_or_else(|| {
                // Placeholder if order not in state (e.g. already filled)
                Order {
                    order_id: order_id.clone(),
                    client_order_id: None,
                    instrument: Instrument::new(instrument_type.clone(), Pair::new("BTC", "USDT")),
                    side: Side::Buy,
                    price: Decimal::ZERO,
                    amount: Decimal::ZERO,
                    internal_created_at: 0,
                    exchange_created_at: None,
                    status: trading_core::types::common::OrderStatus::New,
                    filled_amount: Decimal::ZERO,
                }
            });
            if let Some(pending) = st.pending_amend.take() {
                let _ = pending.send(Ok(order));
            }
        }
        KPaperMessage::Error(e) => {
            warn!("k-paper error: {}", e.message);
            let mut st = state.write().await;
            if let Some((tx, _)) = st.pending_place.take() {
                let _ = tx.send(Err(EngineError::CustomError(anyhow::anyhow!(
                    "{}", e.message
                ))));
            }
            if let Some(pending) = st.pending_cancel.take() {
                let _ = pending.send(Err(EngineError::CustomError(anyhow::anyhow!(
                    "{}", e.message
                ))));
            }
            if let Some(pending) = st.pending_amend.take() {
                let _ = pending.send(Err(EngineError::CustomError(anyhow::anyhow!(
                    "{}", e.message
                ))));
            }
        }
    }
    Ok(())
}
