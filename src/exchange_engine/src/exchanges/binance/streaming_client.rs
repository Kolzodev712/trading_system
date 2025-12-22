use crate::exchanges::binance::config::BinanceConfig;
use crate::exchanges::binance::sign::BinanceWsAuthenticator;
use crate::exchanges::binance::websocket::client::BinanceSubscriptionHandler;
use crate::exchanges::binance::websocket::client::BinanceWSClient;
use crate::exchanges::binance::websocket::types::user_data::events::ParsedBinanceMessage;
use crate::state::manager::ExchangeStateManager;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, error, info, warn};
use trading_core::error::EngineError;
use trading_core::types::{
    common::{OrderStatus, Symbol},
    exchange::{Balance, Order, Trade},
};

/// Streaming client for Binance - handles subscription events only (read-only)
/// This connection is long-lived and independent from RPC operations
#[derive(Debug)]
pub struct BinanceStreamingClient {
    #[allow(dead_code)]
    config: BinanceConfig,
    pub(crate) ws_client: BinanceWSClient,
    #[allow(dead_code)]
    subscription_handler: BinanceSubscriptionHandler,
    state_manager: Arc<tokio::sync::Mutex<Option<Arc<ExchangeStateManager>>>>,
    event_tx: Arc<Mutex<Option<mpsc::Sender<ParsedBinanceMessage>>>>,
    inbound_capacity: usize,
}

impl BinanceStreamingClient {
    pub fn new(config: BinanceConfig, inbound_capacity: usize) -> Self {
        let signer =
            BinanceWsAuthenticator::new(config.ws.api_key.clone(), config.ws.secret_key.clone());
        let session = Arc::new(RwLock::new(None));
        let ws_client = BinanceWSClient::new(signer.clone(), session.clone());
        let subscription_handler = BinanceSubscriptionHandler::new(signer, session);

        Self {
            config,
            ws_client,
            subscription_handler,
            state_manager: Arc::new(tokio::sync::Mutex::new(None)),
            event_tx: Arc::new(Mutex::new(None)),
            inbound_capacity,
        }
    }

    pub async fn set_state_manager(&self, state_manager: Arc<ExchangeStateManager>) {
        *self.state_manager.lock().await = Some(state_manager);
    }

    pub async fn get_event_tx(&self) -> Option<mpsc::Sender<ParsedBinanceMessage>> {
        self.event_tx.lock().await.clone()
    }

    /// Start the streaming connection loop
    pub async fn start_streaming_loop(&self) -> Result<(), EngineError> {
        let mut backoff_iter = 0u32;
        let mut connection_id = 0u64;

        loop {
            connection_id += 1;
            info!(
                "[STREAMING {}] Starting streaming connection (backoff_iter={})",
                connection_id, backoff_iter
            );

            match self.connect_and_run_stream().await {
                Ok(_) => {
                    warn!(
                        "[STREAMING {}] Streaming connection ended unexpectedly, reconnecting...",
                        connection_id
                    );
                }
                Err(e) => {
                    error!(
                        "[STREAMING {}] Streaming connection error: {:?}, reconnecting...",
                        connection_id, e
                    );
                }
            }

            // Reset event_tx on disconnect
            {
                let mut guard = self.event_tx.lock().await;
                *guard = None;
            }

            // Exponential backoff
            self.exponential_backoff_wait(backoff_iter).await;
            backoff_iter = backoff_iter.saturating_add(1);
        }
    }

    async fn connect_and_run_stream(&self) -> Result<(), EngineError> {
        // Create event channel
        let (event_tx, event_rx) = mpsc::channel::<ParsedBinanceMessage>(self.inbound_capacity);

        // Store event_tx
        {
            let mut guard = self.event_tx.lock().await;
            *guard = Some(event_tx.clone());
        }

        info!("[STREAMING] Connecting and authenticating...");
        let mut ws = self.ws_client.connect_and_authenticate().await?;

        info!("[STREAMING] Subscribing to user stream...");
        self.ws_client.subscribe_to_user_stream(&mut ws).await?;

        info!("[STREAMING] Starting event processing loop...");

        // Spawn event handler task
        let state_manager_clone = self.state_manager.clone();
        let mut event_handler_handle = tokio::spawn(async move {
            Self::run_event_handler(event_rx, state_manager_clone).await;
        });

        // Main read loop - only reads, never writes
        loop {
            tokio::select! {
                msg_result = ws.next() => {
                    match msg_result {
                        Some(Ok(Message::Text(text))) => {
                            // Parse and route to event handler
                            if let Ok(parsed) = serde_json::from_str::<ParsedBinanceMessage>(&text) {
                                if event_tx.send(parsed).await.is_err() {
                                    error!("[STREAMING] Event channel closed, exiting read loop");
                                    break;
                                }
                            } else {
                                debug!("[STREAMING] Received non-event message: {}", text);
                            }
                        }
                        Some(Ok(Message::Ping(ping))) => {
                            // Respond to ping
                            if let Err(e) = ws.send(Message::Pong(ping)).await {
                                error!("[STREAMING] Failed to send Pong: {:?}", e);
                                break;
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("[STREAMING] Received Pong");
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("[STREAMING] Received Close frame, connection closed");
                            break;
                        }
                        Some(Ok(Message::Binary(_))) => {
                            debug!("[STREAMING] Received binary message");
                        }
                        Some(Err(e)) => {
                            error!("[STREAMING] WebSocket error: {:?}", e);
                            break;
                        }
                        None => {
                            info!("[STREAMING] WebSocket stream ended");
                            break;
                        }
                    }
                }
                result = &mut event_handler_handle => {
                    error!("[STREAMING] Event handler task exited: {:?}, exiting read loop", result);
                    break;
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn run_event_handler(
        mut event_rx: mpsc::Receiver<ParsedBinanceMessage>,
        state_manager: Arc<tokio::sync::Mutex<Option<Arc<ExchangeStateManager>>>>,
    ) {
        while let Some(event) = event_rx.recv().await {
            match event {
                ParsedBinanceMessage::OutboundAccountPosition(position) => {
                    debug!("[STREAMING] OutboundAccountPosition: {:?}", position);

                    // Update state manager if available
                    if let Some(state_mgr) = state_manager.lock().await.as_ref() {
                        let balances: Vec<Balance> = position.into();
                        if let Err(e) = state_mgr.update_balances(balances).await {
                            warn!(
                                "[STREAMING] Failed to update balances in state manager: {}",
                                e
                            );
                        }
                    }
                }
                ParsedBinanceMessage::BalanceUpdate(update) => {
                    debug!(
                        "[STREAMING] BalanceUpdate (delta): asset={}, delta={}",
                        update.asset, update.delta
                    );

                    // Apply delta to current balance
                    if let Some(state_mgr) = state_manager.lock().await.as_ref() {
                        let state_arc = state_mgr.get_state();
                        let state = state_arc.read().await;

                        // Get current balance or create new one
                        let current_balance = state
                            .get_balance(update.asset.as_str())
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
                            warn!("[STREAMING] Failed to update balance from delta: {}", e);
                        }
                    }
                }
                ParsedBinanceMessage::ExecutionReport(report) => {
                    debug!("[STREAMING] Exec rpt: {:?}", report);

                    // Clone report for trade creation (before converting to Order which takes ownership)
                    let report_for_trade = report.clone();
                    let order_status: OrderStatus = report.order_status.clone().into();

                    // Convert to Order and update state manager if available
                    let order: Order = report.clone().into();

                    if let Some(state_mgr) = state_manager.lock().await.as_ref() {
                        // Update order in state
                        if let Err(e) = state_mgr.update_order(order.clone()).await {
                            warn!("[STREAMING] Failed to update order in state manager: {}", e);
                        }

                        // Create Trade when order is filled (Filled or PartiallyFilled)
                        if matches!(
                            order_status,
                            OrderStatus::Filled | OrderStatus::PartiallyFilled
                        ) {
                            // Get the cumulative filled quantity (total filled so far)
                            let cumulative_filled = report_for_trade
                                .cumulative_filled_quantity
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
                                state
                                    .get_order_by_id(&order_id)
                                    .map(|o| o.filled_amount)
                                    .unwrap_or(rust_decimal::Decimal::ZERO)
                            };

                            // Delta is the new fill amount in this ExecutionReport
                            let fill_delta = cumulative_filled - previous_filled;

                            debug!(
                                "[STREAMING] Trade creation: order_id={}, cumulative_filled={}, previous_filled={}, fill_delta={}, status={:?}",
                                report_for_trade.order_id,
                                cumulative_filled,
                                previous_filled,
                                fill_delta,
                                order_status
                            );

                            // Only create trade if there's actually a new fill (delta > 0)
                            if fill_delta > rust_decimal::Decimal::ZERO {
                                // Get pair from symbol registry
                                let state_arc = state_mgr.get_state();
                                let state = state_arc.read().await;
                                let pair = state
                                    .symbol_registry()
                                    .get_pair(&report_for_trade.symbol.clone().into())
                                    .cloned();
                                drop(state);

                                if let Some(pair) = pair {
                                    // Create trade from execution report
                                    let trade = Trade {
                                        trade_id: format!(
                                            "{}_{}",
                                            report_for_trade.order_id,
                                            report_for_trade.transaction_time
                                        )
                                        .into(),
                                        order_id: report_for_trade.order_id.into(),
                                        pair,
                                        price: report_for_trade.price,
                                        quantity: fill_delta, // Use delta, not cumulative
                                        side: report_for_trade.side.clone().into(),
                                        timestamp: report_for_trade.transaction_time,
                                        fee: rust_decimal::Decimal::ZERO, // Fee not available in ExecutionReport
                                        fee_currency: Symbol::from("USDT"), // Default, will be updated from REST if needed
                                    };

                                    if let Err(e) = state_mgr.add_trade(trade.clone()).await {
                                        warn!("[STREAMING] Failed to add trade from execution report: {}", e);
                                    } else {
                                        info!(
                                            "[STREAMING] ✅ Trade created: order_id={}, trade_id={}, pair={}, side={:?}, price={}, quantity={}",
                                            report_for_trade.order_id,
                                            trade.trade_id,
                                            trade.pair,
                                            trade.side,
                                            trade.price,
                                            trade.quantity
                                        );
                                    }
                                } else {
                                    warn!("[STREAMING] Could not find pair for symbol {} when creating trade", report_for_trade.symbol);
                                }
                            }
                        }
                    }
                }
                ParsedBinanceMessage::Raw(r) => {
                    debug!("[STREAMING] Raw message: {}", r);
                }
            }
        }
        error!("[STREAMING] Event handler channel closed");
    }

    async fn exponential_backoff_wait(&self, iteration: u32) {
        let base_delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);
        let delay = std::cmp::min(base_delay * 2_u32.pow(iteration.min(6)), max_delay);
        warn!("[STREAMING] Waiting {:?} before reconnecting...", delay);
        tokio::time::sleep(delay).await;
    }
}
