use crate::state::Strategy;
use crate::strategies::strategy_manager::StrategyManager;
use asset_engine::AssetService;
use trading_core::types::exchange::{
    BulkAmendItem, BulkAmendRequest, BulkCancelRequest, BulkOrderRequest,
};
use trading_core::{
    channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver},
    circuit_breaker::{order_circuit_breaker, CircuitBreaker},
    error::EngineError,
    types::{
        common::{Exchange, Instrument, InstrumentType, OrderId},
        exchange::{Order, OrderRequest},
        Pair,
    },
    venue::cex::traits::TradingClient,
};
// ExecutionReport removed - use Order directly from channels
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Executes a trading strategy by:
/// 1. Consuming market data from channels
/// 2. Calling strategy.on_orderbook() for each update
/// 3. Placing orders through the trading client
/// 4. Handling execution reports
/// 5. Periodic position reconciliation
/// 6. Circuit breaker for error handling
pub struct StrategyExecutor<S: Strategy + 'static> {
    strategy: Arc<RwLock<S>>,
    trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
    market_data_rx: MarketDataUpdateReceiver,
    pairs: Vec<Pair>, // All pairs this executor handles
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    trading_state_manager: Arc<crate::state::manager::TradingStateManager>,
    order_timeout_secs: u64,
    reconciliation_interval_secs: u64,
    asset_service: Option<Arc<AssetService>>,
    exchange: Exchange,
    /// Instrument type for orders (Spot or Futures)
    instrument_type: InstrumentType,
    /// Config fallback values
    config_min_order_size: Decimal,
    config_step_size: Decimal,
    config_tick_size: Decimal,
    config_min_notional: Decimal,
    /// Trading control - checks if trading is enabled (optional, for TUI control)
    trading_enabled: Option<Arc<tokio::sync::RwLock<bool>>>,
    /// Optional StrategyManager for batching (only set when using StrategyManagerWrapper)
    strategy_manager: Option<Arc<RwLock<crate::strategies::strategy_manager::StrategyManager>>>,
}

impl<S: Strategy> StrategyExecutor<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        strategy: Arc<RwLock<S>>,
        trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
        market_data_rx: MarketDataUpdateReceiver,
        pairs: Vec<Pair>,
        trading_state_manager: Arc<crate::state::manager::TradingStateManager>,
        reconciliation_interval_secs: u64,
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        instrument_type: InstrumentType,
        config_min_order_size: Decimal,
        config_step_size: Decimal,
        config_tick_size: Decimal,
        config_min_notional: Decimal,
    ) -> Self {
        if pairs.is_empty() {
            panic!("StrategyExecutor requires at least one pair");
        }
        Self {
            strategy,
            trading_client,
            market_data_rx,
            pairs,
            circuit_breaker: Arc::new(Mutex::new(order_circuit_breaker())),
            trading_state_manager,
            order_timeout_secs: 300, // 5 minutes default timeout for limit orders
            reconciliation_interval_secs,
            asset_service,
            exchange,
            instrument_type,
            config_min_order_size,
            config_step_size,
            config_tick_size,
            config_min_notional,
            trading_enabled: None,
            strategy_manager: None,
        }
    }

    /// Set trading enabled flag (for TUI control)
    pub fn with_trading_control(mut self, trading_enabled: Arc<tokio::sync::RwLock<bool>>) -> Self {
        self.trading_enabled = Some(trading_enabled);
        self
    }

    /// Set StrategyManager for batching (only for StrategyManagerWrapper)
    pub fn with_strategy_manager(mut self, strategy_manager: Arc<RwLock<StrategyManager>>) -> Self {
        self.strategy_manager = Some(strategy_manager);
        self
    }

    pub fn with_order_timeout(mut self, timeout_secs: u64) -> Self {
        self.order_timeout_secs = timeout_secs;
        self
    }

    /// Main execution loop - consumes orderbooks from channels and executes strategy
    pub async fn run(&self) -> Result<(), EngineError> {
        debug!(
            "Starting strategy executor for {} pair(s): {:?}",
            self.pairs.len(),
            self.pairs
        );

        let strategy = Arc::clone(&self.strategy);
        let trading_client = Arc::clone(&self.trading_client);
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let mut market_data_rx = self.market_data_rx.resubscribe();

        // Start periodic reconciliation task for all pairs
        // Start periodic reconciliation using TradingStateManager
        let trading_state_manager_for_recon = Arc::clone(&self.trading_state_manager);
        let pairs_for_recon = self.pairs.clone();
        let strategy_for_price = Arc::clone(&strategy);
        let reconciliation_interval_secs = self.reconciliation_interval_secs;
        tokio::spawn(async move {
            info!(
                "Starting periodic trading-level reconciliation (interval: {} seconds) for {} pair(s)",
                reconciliation_interval_secs,
                pairs_for_recon.len()
            );
            let mut interval = interval(Duration::from_secs(reconciliation_interval_secs));
            loop {
                interval.tick().await;

                // Reconcile each pair
                for pair in &pairs_for_recon {
                    info!("Running periodic trading-level reconciliation for {}", pair);

                    // Get current price from strategy if available
                    let current_price = {
                        let strat = strategy_for_price.read().await;
                        strat.get_last_price()
                    };

                    // Reconcile using TradingStateManager
                    match trading_state_manager_for_recon
                        .reconcile(pair, current_price)
                        .await
                    {
                        Ok(result) => {
                            if result.reconciled {
                                info!(
                                    "Trading-level reconciliation completed for {symbol}: internal={internal_qty}, exchange={exchange_qty}, discrepancy={discrepancy}",
                                    symbol = result.symbol,
                                    internal_qty = result.internal_quantity,
                                    exchange_qty = result.exchange_quantity,
                                    discrepancy = result.discrepancy
                                );
                            } else {
                                info!(
                                    "Trading-level reconciliation for {symbol}: no discrepancies found",
                                    symbol = result.symbol
                                );
                            }
                        }
                        Err(e) => {
                            error!("Trading-level reconciliation error for {}: {e}", pair);
                        }
                    }
                }
            }
        });

        // Start stale order cleanup task
        // Uses TradingStateManager to get stale orders
        let trading_state_manager_for_cleanup = Arc::clone(&self.trading_state_manager);
        let trading_client_for_cleanup = Arc::clone(&trading_client);
        let order_timeout = self.order_timeout_secs;
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60)); // Every minute
            loop {
                interval.tick().await;

                // Get stale orders from TradingStateManager
                let trading_state = trading_state_manager_for_cleanup.get_state();
                let stale_orders: Vec<_> = {
                    let state_guard = trading_state.read().await;
                    state_guard.get_stale_orders(order_timeout)
                };

                if !stale_orders.is_empty() {
                    warn!("Cancelling {} stale order(s)", stale_orders.len());

                    let bulk_request = BulkCancelRequest {
                        order_ids: stale_orders.clone(),
                        pair: None,
                    };
                    let trading_client_guard = trading_client_for_cleanup.write().await;
                    let bulk_response = trading_client_guard.bulk_cancel_orders(bulk_request).await;

                    match bulk_response {
                        Ok(response) => {
                            for (order_id, result) in
                                stale_orders.iter().zip(response.results.iter())
                            {
                                if result.success {
                                    // Remove order from TradingStateManager
                                    trading_state_manager_for_cleanup
                                        .remove_order(order_id)
                                        .await;
                                } else {
                                    error!(
                                        "Failed to cancel stale order {order_id}: {e}",
                                        order_id = order_id.as_str(),
                                        e = result
                                            .error
                                            .as_ref()
                                            .unwrap_or(&"Unknown error".to_string())
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!("Bulk cancel stale orders failed: {}", e);
                        }
                    }
                }
            }
        });

        // Consume market data updates from channel
        let pairs_set: std::collections::HashSet<_> = self.pairs.iter().collect();
        loop {
            match market_data_rx.recv().await {
                Ok(MarketDataUpdate::Orderbook { orderbook, .. }) => {
                    // Only process orderbooks for pairs we're handling
                    if !pairs_set.contains(&orderbook.pair) {
                        continue;
                    }

                    // Check global trading control first
                    let global_enabled = if let Some(ref trading_enabled) = self.trading_enabled {
                        *trading_enabled.read().await
                    } else {
                        true // Default to enabled if no control set
                    };

                    if !global_enabled {
                        static SKIP_COUNT: std::sync::atomic::AtomicU64 =
                            std::sync::atomic::AtomicU64::new(0);
                        let skip_count =
                            SKIP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if skip_count.is_multiple_of(100) {
                            info!("[StrategyExecutor] Global trading disabled, skipping orderbook (skipped {} so far)", skip_count);
                        }
                        continue; // Global trading disabled, skip this orderbook
                    }

                    // Log that we're processing orderbooks (periodically)
                    static OB_COUNT: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let ob_count = OB_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if ob_count.is_multiple_of(20) {
                        info!(
                            "[StrategyExecutor] Processing orderbook #{} for {}",
                            ob_count, orderbook.pair
                        );
                    }

                    // Get all order requests from strategy
                    // For StrategyManagerWrapper, get all requests for batching
                    // For other strategies, get single request
                    let orderbook_pair = orderbook.pair.clone();
                    let trading_state = self.trading_state_manager.get_state();
                    let state = {
                        let state_guard = trading_state.read().await;
                        state_guard.clone()
                    };

                    // Get all order requests and cancellation requests from strategy
                    // If StrategyManager is available, get all requests for batching
                    // Otherwise, get single request
                    let all_requests: Vec<OrderRequest> = {
                        if let Some(ref strategy_manager) = self.strategy_manager {
                            // Get all requests from StrategyManager for batching
                            let manager = strategy_manager.read().await;
                            match manager.on_orderbook(&orderbook).await {
                                Ok(requests) => requests,
                                Err(e) => {
                                    error!("StrategyManager error: {error}", error = e);
                                    continue;
                                }
                            }
                        } else {
                            // Single strategy - get one request
                            let mut strat = strategy.write().await;
                            match strat.on_orderbook(&orderbook, &state).await {
                                Ok(Some(req)) => vec![req],
                                Ok(None) => vec![],
                                Err(e) => {
                                    error!("Strategy error in on_orderbook: {error}", error = e);
                                    continue;
                                }
                            }
                        }
                    };

                    // Get cancellation requests from strategies
                    let cancel_ids: Vec<OrderId> = {
                        if let Some(ref strategy_manager) = self.strategy_manager {
                            // Get all cancellation requests from StrategyManager
                            let manager = strategy_manager.read().await;
                            match manager.on_orderbook_cancels(&orderbook).await {
                                Ok(ids) => ids,
                                Err(e) => {
                                    error!(
                                        "StrategyManager error getting cancels: {error}",
                                        error = e
                                    );
                                    vec![]
                                }
                            }
                        } else {
                            // Single strategy - get cancellation requests
                            let mut strat = strategy.write().await;
                            match strat.on_orderbook_cancels(&orderbook, &state).await {
                                Ok(ids) => ids,
                                Err(e) => {
                                    error!(
                                        "Strategy error in on_orderbook_cancels: {error}",
                                        error = e
                                    );
                                    vec![]
                                }
                            }
                        }
                    };

                    // Batch process cancellation requests first
                    if !cancel_ids.is_empty() {
                        info!(
                            "[StrategyExecutor] Batch cancelling {} order(s) requested by strategies",
                            cancel_ids.len()
                        );
                        let bulk_request = BulkCancelRequest {
                            order_ids: cancel_ids.clone(),
                            pair: None,
                        };
                        let trading_client_guard = trading_client.write().await;
                        let bulk_response =
                            trading_client_guard.bulk_cancel_orders(bulk_request).await;

                        match bulk_response {
                            Ok(response) => {
                                for (order_id, result) in
                                    cancel_ids.iter().zip(response.results.iter())
                                {
                                    if result.success {
                                        info!(
                                            "[StrategyExecutor] ✅ Cancelled order {} requested by strategy",
                                            order_id.as_str()
                                        );
                                        // Remove order from TradingStateManager
                                        self.trading_state_manager.remove_order(order_id).await;
                                    } else {
                                        warn!(
                                            "Failed to cancel order {} requested by strategy: {}",
                                            order_id.as_str(),
                                            result
                                                .error
                                                .as_ref()
                                                .unwrap_or(&"Unknown error".to_string())
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Bulk cancel orders failed: {}", e);
                            }
                        }
                    }

                    if all_requests.is_empty() {
                        continue;
                    }

                    // Override instrument type for all requests
                    let all_requests: Vec<OrderRequest> = all_requests
                        .into_iter()
                        .map(|mut req| {
                            req.instrument = Instrument::new(
                                self.instrument_type.clone(),
                                req.instrument.pair.clone(),
                            );
                            req
                        })
                        .collect();

                    // Check circuit breaker and trading control
                    let cb = circuit_breaker.lock().await;
                    if !cb.can_proceed() {
                        debug!(
                            "Circuit breaker is open, skipping order placement. State: {state}",
                            state = cb.state()
                        );
                        continue;
                    }
                    drop(cb);

                    // Check if trading is enabled
                    if let Some(ref trading_enabled) = self.trading_enabled {
                        if !*trading_enabled.read().await {
                            tracing::warn!(
                                "Trading is disabled - blocking order placement from strategy"
                            );
                            continue;
                        }
                    }

                    // Get trading rules once for all requests
                    use crate::order_validation::{
                        get_trading_rules, round_price, round_quantity, validate_order,
                    };
                    let rules = get_trading_rules(
                        self.asset_service.as_ref(),
                        self.exchange.clone(),
                        &orderbook_pair,
                        self.config_min_order_size,
                        self.config_step_size,
                        self.config_tick_size,
                        self.config_min_notional,
                    )
                    .await;

                    // Get balances once
                    let balances = trading_client.read().await.fetch_balance().await.ok();

                    // Get active orders count
                    let trading_state = self.trading_state_manager.get_state();
                    let active_count = {
                        let state = trading_state.read().await;
                        state.get_active_order_count(&orderbook_pair)
                    };

                    // Collect requests into batches: to_post and to_amend
                    let mut to_post: Vec<OrderRequest> = Vec::new();
                    let mut to_amend: Vec<(OrderId, OrderRequest)> = Vec::new();

                    // First pass: validate and check each request
                    for mut req in all_requests {
                        info!(
                            "[StrategyExecutor] Strategy requested order: side={side}, symbol={symbol}, price={price}, amount={amount}",
                            side = req.side,
                            symbol = req.instrument.symbol(),
                            price = req.price,
                            amount = req.amount
                        );

                        // Check active orders limit
                        if active_count >= 5 {
                            debug!(
                                "Too many active orders ({active_count}), skipping order placement"
                            );
                            continue;
                        }

                        // Check if there's an existing order for this symbol/side that should be amended
                        let existing_order_opt = {
                            let state = trading_state.read().await;
                            state
                                .active_orders
                                .values()
                                .find(|o| {
                                    o.instrument.pair == req.instrument.pair && o.side == req.side
                                        && matches!(
                                            o.status,
                                            trading_core::types::common::OrderStatus::New
                                                | trading_core::types::common::OrderStatus::PartiallyFilled
                                        )
                                })
                                .cloned()
                        };

                        // If we found an existing order, check if it needs amendment
                        if let Some(existing_order) = existing_order_opt {
                            use rust_decimal::Decimal;
                            let price_changed =
                                (existing_order.price - req.price).abs() > Decimal::new(1, 10);
                            let quantity_changed =
                                (existing_order.amount - req.amount).abs() > Decimal::new(1, 10);

                            if price_changed || quantity_changed {
                                // Collect for batch amendment
                                to_amend.push((existing_order.order_id, req));
                                continue;
                            } else {
                                // No amendment needed
                                continue;
                            }
                        }

                        // Validate balance
                        if let Some(ref balances) = balances {
                            let balance_ok = match req.side {
                                trading_core::types::common::Side::Buy => {
                                    let quote_balance = balances
                                        .iter()
                                        .find(|b| {
                                            let balance_symbol = b.symbol.as_ref();
                                            balance_symbol == orderbook_pair.quote.as_str()
                                                || balance_symbol
                                                    .ends_with(orderbook_pair.quote.as_str())
                                        })
                                        .map(|b| b.free)
                                        .unwrap_or(Decimal::ZERO);
                                    quote_balance >= req.price * req.amount
                                }
                                trading_core::types::common::Side::Sell => {
                                    let base_balance = balances
                                        .iter()
                                        .find(|b| {
                                            let balance_symbol = b.symbol.as_ref();
                                            balance_symbol == orderbook_pair.base.as_str()
                                                || balance_symbol
                                                    .starts_with(orderbook_pair.base.as_str())
                                        })
                                        .map(|b| b.free)
                                        .unwrap_or(Decimal::ZERO);
                                    base_balance >= req.amount
                                }
                            };
                            if !balance_ok {
                                warn!("Insufficient balance, skipping order");
                                continue;
                            }
                        }

                        // Round and validate
                        req.amount = round_quantity(req.amount, rules.step_size);
                        if req.amount < rules.min_order_size {
                            continue;
                        }
                        req.price = round_price(req.price, rules.tick_size);
                        if validate_order(req.price, req.amount, &rules).is_err() {
                            continue;
                        }

                        // Collect for batch posting
                        to_post.push(req);
                    }

                    // Second pass: Batch process posts and amendments
                    let trading_client_guard = trading_client.write().await;

                    // Batch process posts
                    if !to_post.is_empty() {
                        info!(
                            "[StrategyExecutor] Batch posting {} order(s) for {}",
                            to_post.len(),
                            orderbook_pair
                        );
                        let bulk_request = BulkOrderRequest {
                            requests: to_post.clone(),
                        };
                        let bulk_response =
                            trading_client_guard.bulk_open_orders(bulk_request).await;

                        match bulk_response {
                            Ok(response) => {
                                let results = response.results;
                                for (idx, _req) in to_post.iter().enumerate() {
                                    if idx >= results.len() {
                                        warn!("Bulk response missing result for index {}", idx);
                                        continue;
                                    }
                                    let result = &results[idx];
                                    let order_result = if result.success {
                                        result.data.clone().ok_or_else(|| {
                                            EngineError::CustomError(anyhow::anyhow!(
                                                "Bulk operation succeeded but no data returned"
                                            ))
                                        })
                                    } else {
                                        let error_msg = result
                                            .error
                                            .clone()
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        Err(EngineError::CustomError(anyhow::anyhow!(error_msg)))
                                    };

                                    match order_result {
                                        Ok(mut order) => {
                                            order.status =
                                                trading_core::types::common::OrderStatus::New;
                                            order.filled_amount = Decimal::ZERO;
                                            info!(
                                                "[StrategyExecutor] ✅ Order placed: {} for {}",
                                                order.order_id.as_str(),
                                                orderbook_pair
                                            );
                                            circuit_breaker.lock().await.record_success();
                                            self.trading_state_manager.add_order(order).await;
                                        }
                                        Err(e) => {
                                            warn!("Failed to place order in batch: {}", e);
                                            circuit_breaker.lock().await.record_failure();
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Bulk open orders failed: {}", e);
                            }
                        }
                    }

                    // Batch process amendments
                    if !to_amend.is_empty() {
                        info!(
                            "[StrategyExecutor] Batch amending {} order(s) for {}",
                            to_amend.len(),
                            orderbook_pair
                        );
                        let amendments: Vec<BulkAmendItem> = to_amend
                            .iter()
                            .map(|(order_id, req)| BulkAmendItem {
                                order_id: order_id.clone(),
                                new_request: req.clone(),
                            })
                            .collect();
                        let bulk_request = BulkAmendRequest { amendments };
                        let bulk_response =
                            trading_client_guard.bulk_amend_orders(bulk_request).await;

                        match bulk_response {
                            Ok(response) => {
                                let results = response.results;
                                for (idx, (order_id, _)) in to_amend.iter().enumerate() {
                                    if idx >= results.len() {
                                        warn!("Bulk response missing result for index {}", idx);
                                        continue;
                                    }
                                    let result = &results[idx];
                                    let amended_order_result = if result.success {
                                        result.data.clone().ok_or_else(|| {
                                            EngineError::CustomError(anyhow::anyhow!(
                                                "Bulk operation succeeded but no data returned"
                                            ))
                                        })
                                    } else {
                                        let error_msg = result
                                            .error
                                            .clone()
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        Err(EngineError::CustomError(anyhow::anyhow!(error_msg)))
                                    };

                                    match amended_order_result {
                                        Ok(amended_order) => {
                                            info!(
                                                "[StrategyExecutor] ✅ Order amended: {} -> {} for {}",
                                                order_id.as_str(),
                                                amended_order.order_id.as_str(),
                                                orderbook_pair
                                            );
                                            self.trading_state_manager
                                                .add_order(amended_order)
                                                .await;
                                        }
                                        Err(e) => {
                                            warn!("Failed to amend order in batch: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Bulk amend orders failed: {}", e);
                            }
                        }
                    }
                }
                Ok(MarketDataUpdate::Trade { .. }) => {
                    // Ignore trades for now
                    debug!("Received trade update (ignored)");
                }
                Ok(MarketDataUpdate::DexAccountHealth { .. }) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        "Market data receiver lagged, skipped {skipped} messages. This may indicate slow processing."
                    );
                    // Continue receiving - broadcast channels automatically skip lagged messages
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    warn!("Market data channel closed - shutting down strategy executor");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle execution report from exchange
    pub async fn handle_execution_report(&self, order: &Order) -> Result<(), EngineError> {
        let mut strategy_guard = self.strategy.write().await;
        strategy_guard.on_execution_report(order).await?;

        Ok(())
    }
}
