use crate::cex::intent::{
    apply_abs_qty_cap, approve_target_qty, intent_to_order_request, StrategyContext, StrategyId,
    StrategyIntent,
};
use crate::cex::state::Strategy;
use crate::cex::strategies::strategy_manager::StrategyManager;
use crate::risk::RiskManager;
use asset_engine::AssetService;
use trading_core::types::exchange::{
    BulkAmendItem, BulkAmendRequest, BulkCancelRequest, BulkOrderRequest,
};
use trading_core::{
    channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver},
    circuit_breaker::{order_circuit_breaker, CircuitBreaker},
    error::EngineError,
    types::{
        common::{Exchange, Instrument, InstrumentType, OrderId, OrderType, Side},
        exchange::{Order, OrderRequest},
        ExecutionMode, Pair,
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
/// 2. Calling `Strategy::evaluate` for each update
/// 3. Turning approved intent into orders through the trading client
/// 4. Handling execution reports
/// 5. Periodic position reconciliation
/// 6. Circuit breaker for error handling
pub struct StrategyExecutor<S: Strategy + 'static> {
    strategy: Arc<RwLock<S>>,
    trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
    market_data_rx: MarketDataUpdateReceiver,
    pairs: Vec<Pair>, // All pairs this executor handles
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    trading_state_manager: Arc<crate::cex::state::manager::TradingStateManager>,
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
    entries_paused: Option<Arc<tokio::sync::RwLock<bool>>>,
    flatten_latched: Option<Arc<tokio::sync::RwLock<bool>>>,
    execution_mode: ExecutionMode,
    /// Optional StrategyManager for batching (only set when using StrategyManagerWrapper)
    strategy_manager:
        Option<Arc<RwLock<crate::cex::strategies::strategy_manager::StrategyManager>>>,
    risk_manager: Option<Arc<RiskManager>>,
    /// `max_position` per strategy id. Shared so gRPC can update without restart.
    strategy_caps: Arc<RwLock<std::collections::HashMap<StrategyId, Decimal>>>,
    max_active_orders: usize,
    /// Futures leverage used only to scale quote margin checks. `None` requires full notional.
    leverage: Option<u32>,
    /// Required for Exchange+Futures. None means freshness is not enforced (Spot / Shadow).
    max_market_data_age_ms: Option<u64>,
    submission_journal: Option<Arc<crate::cex::state::SubmissionJournal>>,
    exchange_state_manager: Option<Arc<exchange_engine::state::manager::ExchangeStateManager>>,
    immediate_recon: Arc<exchange_engine::state::ImmediateTrigger>,
}

impl<S: Strategy> StrategyExecutor<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        strategy: Arc<RwLock<S>>,
        trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
        market_data_rx: MarketDataUpdateReceiver,
        pairs: Vec<Pair>,
        trading_state_manager: Arc<crate::cex::state::manager::TradingStateManager>,
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
            entries_paused: None,
            flatten_latched: None,
            execution_mode: ExecutionMode::Exchange,
            strategy_manager: None,
            risk_manager: None,
            strategy_caps: Arc::new(RwLock::new(std::collections::HashMap::new())),
            max_active_orders: 5,
            leverage: None,
            max_market_data_age_ms: None,
            submission_journal: None,
            exchange_state_manager: None,
            immediate_recon: Arc::new(exchange_engine::state::ImmediateTrigger::new()),
        }
    }

    pub fn with_shared_strategy_caps(
        mut self,
        strategy_caps: Arc<RwLock<std::collections::HashMap<StrategyId, Decimal>>>,
    ) -> Self {
        self.strategy_caps = strategy_caps;
        self
    }

    pub fn with_max_active_orders(mut self, max_active_orders: usize) -> Self {
        self.max_active_orders = max_active_orders;
        self
    }

    pub fn with_leverage(mut self, leverage: Option<u32>) -> Self {
        self.leverage = leverage;
        self
    }

    pub fn with_max_market_data_age_ms(mut self, max_market_data_age_ms: Option<u64>) -> Self {
        self.max_market_data_age_ms = max_market_data_age_ms;
        self
    }

    pub fn with_risk_manager(mut self, risk_manager: Arc<RiskManager>) -> Self {
        self.risk_manager = Some(risk_manager);
        self
    }

    /// Set trading enabled flag (for TUI control)
    pub fn with_trading_control(mut self, trading_enabled: Arc<tokio::sync::RwLock<bool>>) -> Self {
        self.trading_enabled = Some(trading_enabled);
        self
    }

    pub fn with_operator_flags(
        mut self,
        entries_paused: Arc<tokio::sync::RwLock<bool>>,
        flatten_latched: Arc<tokio::sync::RwLock<bool>>,
    ) -> Self {
        self.entries_paused = Some(entries_paused);
        self.flatten_latched = Some(flatten_latched);
        self
    }

    pub fn with_execution_mode(mut self, execution_mode: ExecutionMode) -> Self {
        self.execution_mode = execution_mode;
        self
    }

    /// Set StrategyManager for batching (only for StrategyManagerWrapper)
    pub fn with_strategy_manager(mut self, strategy_manager: Arc<RwLock<StrategyManager>>) -> Self {
        self.strategy_manager = Some(strategy_manager);
        self
    }

    pub fn with_submission_journal(
        mut self,
        journal: Arc<crate::cex::state::SubmissionJournal>,
    ) -> Self {
        self.submission_journal = Some(journal);
        self
    }

    pub fn with_exchange_state_manager(
        mut self,
        exchange_state_manager: Arc<exchange_engine::state::manager::ExchangeStateManager>,
    ) -> Self {
        self.exchange_state_manager = Some(exchange_state_manager);
        self
    }

    async fn persist_submission(
        &self,
        pending: &crate::cex::state::PendingSubmission,
        state: crate::cex::state::SubmissionState,
    ) -> Result<(), EngineError> {
        let journal = self.submission_journal.as_ref().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("submission journal is required"))
        })?;
        journal
            .append(&crate::cex::state::SubmissionRecord::from_pending(
                pending, state,
            ))
            .map_err(|err| {
                EngineError::CustomError(anyhow::anyhow!("submission journal append failed: {err}"))
            })
    }

    async fn halt_new_exposure(&self, reason: impl std::fmt::Display) {
        error!("halting new exposure: {reason}");
        self.trading_state_manager
            .get_state()
            .write()
            .await
            .halt_new_exposure(reason.to_string());
    }

    async fn persist_or_halt(
        &self,
        pending: &crate::cex::state::PendingSubmission,
        state: crate::cex::state::SubmissionState,
    ) -> bool {
        match self.persist_submission(pending, state).await {
            Ok(()) => true,
            Err(err) => {
                self.halt_new_exposure(err).await;
                false
            }
        }
    }

    async fn recover_submissions(&self) {
        let Some(journal) = &self.submission_journal else {
            self.halt_new_exposure("submission journal is required")
                .await;
            return;
        };
        let client = self.trading_client.read().await;
        let state_lock = self.trading_state_manager.get_state();
        let mut state = state_lock.write().await;
        match crate::cex::state::recover_unknown_submissions(journal.as_ref(), &*client, &mut state)
            .await
        {
            Ok(report) => {
                info!(
                    imported_working = report.imported_working,
                    resolved_terminal = report.resolved_terminal,
                    still_ambiguous = report.still_ambiguous,
                    "submission recovery finished"
                );
            }
            Err(err) => {
                error!("submission recovery failed: {err}");
                state.halt_new_exposure(format!("submission recovery failed: {err}"));
            }
        }
        drop(state);
        if let Some(ledger) = self.trading_state_manager.execution_ledger() {
            match client.fetch_trades(None).await {
                Ok(trades) => {
                    for trade in trades {
                        if let Err(err) = ledger.record_trade(&trade) {
                            error!("execution ledger replay failed: {err}");
                            self.halt_new_exposure(format!(
                                "execution ledger replay failed: {err}"
                            ))
                            .await;
                            break;
                        }
                    }
                }
                Err(err) => {
                    error!("execution ledger trade fetch failed: {err}");
                    self.halt_new_exposure(format!("execution ledger trade fetch failed: {err}"))
                        .await;
                }
            }
        } else {
            self.halt_new_exposure("execution ledger is required").await;
        }
        drop(client);
        self.request_immediate_reconciliation();
    }

    fn request_immediate_reconciliation(&self) {
        if let Some(esm) = &self.exchange_state_manager {
            esm.request_immediate_reconciliation();
        }
        self.immediate_recon.request();
    }

    /// Main execution loop - consumes orderbooks from channels and executes strategy
    pub async fn run(&self) -> Result<(), EngineError> {
        debug!(
            "Starting strategy executor for {} pair(s): {:?}",
            self.pairs.len(),
            self.pairs
        );

        self.recover_submissions().await;

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
        let immediate_recon = Arc::clone(&self.immediate_recon);
        tokio::spawn(async move {
            info!(
                "Starting periodic trading-level reconciliation (interval: {} seconds) for {} pair(s)",
                reconciliation_interval_secs,
                pairs_for_recon.len()
            );
            let mut interval = interval(Duration::from_secs(reconciliation_interval_secs));
            loop {
                if immediate_recon.wait_tick_or_request(&mut interval).await {
                    info!("Immediate trading-level reconciliation requested");
                }

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
        let skip_exchange_orders = self.execution_mode == ExecutionMode::Shadow;
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60)); // Every minute
            loop {
                interval.tick().await;
                if skip_exchange_orders {
                    continue;
                }

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

                    // Evaluate even when exchange execution is forbidden (Shadow / paused).
                    let trading_enabled = if let Some(ref trading_enabled) = self.trading_enabled {
                        *trading_enabled.read().await
                    } else {
                        true
                    };
                    let entries_paused = if let Some(ref flag) = self.entries_paused {
                        *flag.read().await
                    } else {
                        false
                    };
                    let flatten_latched = if let Some(ref flag) = self.flatten_latched {
                        *flag.read().await
                    } else {
                        false
                    };
                    let exchange_orders_allowed = self.execution_mode.permits_exchange_orders(true);

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

                    // StrategyManager evaluates every matching strategy; otherwise one evaluate.
                    let orderbook_pair = orderbook.pair.clone();
                    let trading_state = self.trading_state_manager.get_state();
                    let state = {
                        let state_guard = trading_state.read().await;
                        state_guard.clone()
                    };

                    let intents: Vec<StrategyIntent> = {
                        if let Some(ref strategy_manager) = self.strategy_manager {
                            let manager = strategy_manager.read().await;
                            match manager.on_orderbook(&orderbook, &state).await {
                                Ok(intents) => intents,
                                Err(e) => {
                                    error!("StrategyManager error: {error}", error = e);
                                    continue;
                                }
                            }
                        } else {
                            let mut strat = strategy.write().await;
                            let ctx = StrategyContext {
                                orderbook: &orderbook,
                                account: &state,
                                instrument_type: self.instrument_type.clone(),
                            };
                            match strat.evaluate(&ctx).await {
                                Ok(intent) => vec![intent],
                                Err(e) => {
                                    error!("Strategy error in evaluate: {error}", error = e);
                                    continue;
                                }
                            }
                        }
                    };

                    let intents = if flatten_latched {
                        intents
                            .into_iter()
                            .map(|intent| match intent {
                                StrategyIntent::TargetPosition {
                                    strategy_id,
                                    instrument,
                                    ..
                                } => StrategyIntent::TargetPosition {
                                    strategy_id,
                                    instrument,
                                    qty: Decimal::ZERO,
                                },
                                other => other,
                            })
                            .collect()
                    } else if std::env::var("TRADING_TEST_HOLD_INTENTS").ok().as_deref()
                        == Some("1")
                    {
                        intents
                            .into_iter()
                            .map(|intent| match intent {
                                StrategyIntent::TargetPosition { strategy_id, .. } => {
                                    StrategyIntent::Hold { strategy_id }
                                }
                                other => other,
                            })
                            .collect()
                    } else {
                        intents
                    };

                    let current_qty = state.effective_position_qty(&orderbook_pair);
                    let max_abs = self
                        .risk_manager
                        .as_ref()
                        .and_then(|risk| risk.get_limits().total_max_position);
                    let caps = self.strategy_caps.read().await;

                    let mut all_requests: Vec<OrderRequest> = intents
                        .into_iter()
                        .filter_map(|intent| {
                            let approved = match &intent {
                                StrategyIntent::Hold { .. } => return None,
                                StrategyIntent::TargetPosition {
                                    strategy_id, qty, ..
                                } => {
                                    let per_strategy = approve_target_qty(strategy_id, *qty, &caps);
                                    apply_abs_qty_cap(per_strategy, max_abs)
                                }
                            };
                            let approved_intent = match intent {
                                StrategyIntent::TargetPosition {
                                    strategy_id,
                                    instrument,
                                    ..
                                } => StrategyIntent::TargetPosition {
                                    strategy_id,
                                    instrument,
                                    qty: approved,
                                },
                                other => other,
                            };
                            intent_to_order_request(&approved_intent, current_qty, &orderbook)
                        })
                        .collect();
                    drop(caps);

                    if !exchange_orders_allowed {
                        if self.execution_mode == ExecutionMode::Shadow && !all_requests.is_empty()
                        {
                            info!(
                                "[StrategyExecutor] Shadow mode: {} would-be order(s) for {} not sent",
                                all_requests.len(),
                                orderbook_pair
                            );
                        }
                        continue;
                    }

                    all_requests.retain(|req| {
                        let increasing =
                            crate::risk::order_increases_exposure(req.side.clone(), current_qty);
                        trading_core::grpc::operator_allows_submit(
                            increasing,
                            trading_enabled,
                            entries_paused,
                            flatten_latched,
                        )
                    });

                    if all_requests.is_empty() {
                        if flatten_latched || entries_paused {
                            continue;
                        }
                        if let Some(req) =
                            lab_force_place_request(&orderbook, self.instrument_type.clone())
                        {
                            info!("[StrategyExecutor] lab force-place for {}", orderbook_pair);
                            all_requests.push(req);
                        } else {
                            continue;
                        }
                    }

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
                        if active_count >= self.max_active_orders {
                            debug!(
                                "Too many active orders ({active_count}), skipping order placement"
                            );
                            continue;
                        }

                        let position_qty = state.position_qty(&orderbook_pair);
                        let futures = matches!(
                            req.instrument.instrument_type(),
                            trading_core::types::common::InstrumentType::Futures
                                | trading_core::types::common::InstrumentType::Derivitives
                        );
                        if futures
                            && !crate::risk::order_increases_exposure(
                                req.side.clone(),
                                position_qty,
                            )
                        {
                            req.exchange_params.insert(
                                smol_str::SmolStr::new("reduceOnly"),
                                smol_str::SmolStr::new("true"),
                            );
                        }

                        let mut prices = std::collections::HashMap::new();
                        if let Some(mid) = orderbook.mid_price() {
                            prices.insert(orderbook_pair.clone(), mid);
                        }
                        let balance_ok = crate::risk::check_order_balance(
                            &req,
                            balances.as_deref().unwrap_or(&[]),
                            position_qty,
                            self.leverage,
                        );
                        if !balance_ok.allowed {
                            warn!(
                                "Balance check blocked order: {}",
                                balance_ok.reason.unwrap_or_else(|| "unknown".into())
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
                                if let Some(ref risk) = self.risk_manager {
                                    let check = risk
                                        .check_pre_trade(
                                            &req,
                                            &state,
                                            balances.as_deref().unwrap_or(&[]),
                                            Some(&prices),
                                            position_qty,
                                            self.leverage,
                                        )
                                        .await;
                                    if !check.allowed {
                                        warn!(
                                            "Risk blocked amendment: {}",
                                            check.reason.unwrap_or_else(|| "unknown".into())
                                        );
                                        continue;
                                    }
                                }
                                to_amend.push((existing_order.order_id, req));
                                continue;
                            } else {
                                // No amendment needed
                                continue;
                            }
                        }

                        // Round and validate
                        req.amount = round_quantity(req.amount, rules.step_size);
                        if req.amount < rules.min_order_size {
                            continue;
                        }
                        req.price = round_price(req.price, rules.tick_size);
                        if let Err(err) = validate_order(req.price, req.amount, &rules) {
                            warn!("Order failed validation: {err}");
                            continue;
                        }

                        if let Some(ref risk) = self.risk_manager {
                            let check = risk
                                .check_pre_trade(
                                    &req,
                                    &state,
                                    balances.as_deref().unwrap_or(&[]),
                                    Some(&prices),
                                    position_qty,
                                    self.leverage,
                                )
                                .await;
                            if !check.allowed {
                                warn!(
                                    "Risk blocked order: {}",
                                    check.reason.unwrap_or_else(|| "unknown".into())
                                );
                                continue;
                            }
                        }

                        // Collect for batch posting
                        apply_lab_order_overrides(&mut req);
                        to_post.push(req);
                    }

                    // Second pass: record intent, then submit. Halted or unresolved
                    // submissions do not send another order.
                    {
                        let live = trading_state.read().await;
                        if live.blocks_new_exposure(&orderbook_pair) {
                            warn!(
                                "New exposure blocked for {} (halt={:?} unresolved={} disagreement={})",
                                orderbook_pair,
                                live.exposure_halt_reason(),
                                live.has_unresolved_submission(&orderbook_pair),
                                live.position_disagreement(&orderbook_pair)
                            );
                            continue;
                        }
                    }

                    let stale_md =
                        market_data_is_stale(orderbook.timestamp, self.max_market_data_age_ms);
                    let position_qty = {
                        let live = trading_state.read().await;
                        live.position_qty(&orderbook_pair)
                    };

                    let mut ready_to_post: Vec<OrderRequest> = Vec::new();
                    for req in &mut to_post {
                        let increasing = is_exposure_increase(req, position_qty);
                        if increasing {
                            let live = trading_state.read().await;
                            if live.blocks_exposure_increase(&orderbook_pair) {
                                warn!(
                                    "Exposure increase blocked for {} (sync_pending={})",
                                    orderbook_pair,
                                    live.position_sync_pending(&orderbook_pair)
                                );
                                continue;
                            }
                            if stale_md {
                                warn!(
                                    "Stale market data blocks exposure increase for {}",
                                    orderbook_pair
                                );
                                continue;
                            }
                        }
                        let client_order_id = lab_client_order_id(req);
                        req.exchange_params.insert(
                            smol_str::SmolStr::new("orderLinkId"),
                            client_order_id.clone(),
                        );
                        let pending = crate::cex::state::PendingSubmission {
                            client_order_id,
                            symbol: orderbook_pair.clone(),
                            side: req.side.clone(),
                            amount: req.amount,
                            price: req.price,
                            strategy_id: None,
                        };
                        if !self
                            .persist_or_halt(&pending, crate::cex::state::SubmissionState::Prepared)
                            .await
                        {
                            continue;
                        }
                        trading_state
                            .write()
                            .await
                            .begin_submission(pending.clone());
                        if !self
                            .persist_or_halt(
                                &pending,
                                crate::cex::state::SubmissionState::Submitted,
                            )
                            .await
                        {
                            continue;
                        }
                        ready_to_post.push(req.clone());
                    }
                    to_post = ready_to_post;

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

                        let mut need_recovery = false;
                        match bulk_response {
                            Ok(response) => {
                                let results = response.results;
                                for (idx, req) in to_post.iter().enumerate() {
                                    let client_id = req
                                        .exchange_params
                                        .get("orderLinkId")
                                        .map(|id| id.to_string());
                                    let pending = client_id.as_ref().map(|id| {
                                        crate::cex::state::PendingSubmission {
                                            client_order_id: smol_str::SmolStr::new(id),
                                            symbol: orderbook_pair.clone(),
                                            side: req.side.clone(),
                                            amount: req.amount,
                                            price: req.price,
                                            strategy_id: None,
                                        }
                                    });
                                    if idx >= results.len() {
                                        warn!("Bulk response missing result for index {}", idx);
                                        if let Some(pending) = pending {
                                            let _ = self
                                                .persist_or_halt(
                                                    &pending,
                                                    crate::cex::state::SubmissionState::Unknown,
                                                )
                                                .await;
                                        }
                                        need_recovery = true;
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
                                            if order.client_order_id.is_none() {
                                                order.client_order_id = client_id
                                                    .as_deref()
                                                    .map(smol_str::SmolStr::new);
                                            }
                                            if let Some(pending) = pending {
                                                let _ = self
                                                    .persist_or_halt(
                                                        &pending,
                                                        crate::cex::state::SubmissionState::Acknowledged,
                                                    )
                                                    .await;
                                            }
                                            order.status =
                                                trading_core::types::common::OrderStatus::New;
                                            order.filled_amount = Decimal::ZERO;
                                            info!(
                                                "[StrategyExecutor] ✅ Order placed: {} for {}",
                                                order.order_id.as_str(),
                                                orderbook_pair
                                            );
                                            circuit_breaker.lock().await.record_success();
                                            trading_state.write().await.import_working_order(order);
                                        }
                                        Err(e) => {
                                            if place_error_is_unknown(&e) {
                                                if let Some(pending) = pending {
                                                    let _ = self
                                                        .persist_or_halt(
                                                            &pending,
                                                            crate::cex::state::SubmissionState::Unknown,
                                                        )
                                                        .await;
                                                }
                                                warn!(
                                                    "Failed to place order in batch (reservation retained): {}",
                                                    e
                                                );
                                                circuit_breaker.lock().await.record_failure();
                                                need_recovery = true;
                                            } else if let Some(pending) = pending {
                                                warn!(
                                                    "Known place rejection (reservation released): {}",
                                                    e
                                                );
                                                trading_state.write().await.clear_submission(
                                                    pending.client_order_id.as_str(),
                                                );
                                                let _ = self
                                                    .persist_or_halt(
                                                        &pending,
                                                        crate::cex::state::SubmissionState::Rejected,
                                                    )
                                                    .await;
                                                circuit_breaker.lock().await.record_failure();
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Bulk open orders failed (reservation retained): {}", e);
                                for req in &to_post {
                                    if let Some(id) = req.exchange_params.get("orderLinkId") {
                                        let pending = crate::cex::state::PendingSubmission {
                                            client_order_id: id.clone(),
                                            symbol: orderbook_pair.clone(),
                                            side: req.side.clone(),
                                            amount: req.amount,
                                            price: req.price,
                                            strategy_id: None,
                                        };
                                        let _ = self
                                            .persist_or_halt(
                                                &pending,
                                                crate::cex::state::SubmissionState::Unknown,
                                            )
                                            .await;
                                    }
                                }
                                need_recovery = true;
                            }
                        }
                        drop(trading_client_guard);
                        if need_recovery {
                            self.recover_submissions().await;
                        }
                    } else {
                        drop(trading_client_guard);
                    }

                    // Batch process amendments
                    if !to_amend.is_empty() {
                        info!(
                            "[StrategyExecutor] Batch amending {} order(s) for {}",
                            to_amend.len(),
                            orderbook_pair
                        );
                        let trading_client_guard = trading_client.write().await;
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

fn lab_client_order_id(req: &OrderRequest) -> smol_str::SmolStr {
    let reduce_only = req
        .exchange_params
        .get("reduceOnly")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if reduce_only {
        return smol_str::SmolStr::new(uuid::Uuid::new_v4().to_string());
    }
    match std::env::var("TRADING_TEST_CLIENT_ORDER_ID") {
        Ok(id) if !id.is_empty() => smol_str::SmolStr::new(id),
        _ => smol_str::SmolStr::new(uuid::Uuid::new_v4().to_string()),
    }
}

fn apply_lab_order_overrides(req: &mut OrderRequest) {
    if let Ok(raw) = std::env::var("TRADING_TEST_LIMIT_PRICE") {
        if let Ok(price) = raw.parse::<Decimal>() {
            if price > Decimal::ZERO {
                req.price = price;
            }
        }
    }
    if let Ok(raw) = std::env::var("TRADING_TEST_ORDER_QTY") {
        if let Ok(qty) = raw.parse::<Decimal>() {
            if qty > Decimal::ZERO {
                req.amount = qty;
            }
        }
    }
}

fn lab_force_place_request(
    orderbook: &trading_core::types::market_data::Orderbook,
    instrument_type: InstrumentType,
) -> Option<OrderRequest> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static USED: AtomicBool = AtomicBool::new(false);
    if std::env::var("TRADING_TEST_FORCE_PLACE").ok().as_deref() != Some("1") {
        return None;
    }
    if USED.swap(true, Ordering::SeqCst) {
        return None;
    }
    let price = std::env::var("TRADING_TEST_LIMIT_PRICE")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .filter(|price: &Decimal| *price > Decimal::ZERO)
        .or_else(|| {
            orderbook
                .best_bid_price()
                .map(|bid| (bid / Decimal::from(2)).max(Decimal::ONE))
        })?;
    let amount = std::env::var("TRADING_TEST_ORDER_QTY")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .filter(|qty: &Decimal| *qty > Decimal::ZERO)
        .unwrap_or(Decimal::new(2, 3));
    Some(OrderRequest {
        instrument: Instrument::new(instrument_type, orderbook.pair.clone()),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price,
        amount,
        timestamp: None,
        exchange_params: Default::default(),
    })
}

fn is_exposure_increase(req: &OrderRequest, position_qty: Decimal) -> bool {
    match req.side {
        Side::Buy => true,
        Side::Sell => position_qty <= Decimal::ZERO,
    }
}

fn market_data_is_stale(orderbook_ts: u64, max_age_ms: Option<u64>) -> bool {
    let Some(max_age_ms) = max_age_ms else {
        return false;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let ts_ms = if orderbook_ts > 10_000_000_000 {
        orderbook_ts
    } else {
        orderbook_ts.saturating_mul(1000)
    };
    now_ms.saturating_sub(ts_ms) > max_age_ms
}

fn place_error_is_known_reject(err: &EngineError) -> bool {
    let text = err.to_string();
    let lower = text.to_lowercase();
    lower.contains("lab_reject")
        || lower.contains("duplicate client_order_id")
        || lower.contains("order rejected")
}

fn place_error_is_unknown(err: &EngineError) -> bool {
    !place_error_is_known_reject(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aged_timestamp_is_stale() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(market_data_is_stale(
            now_ms.saturating_sub(5_000),
            Some(1_000)
        ));
        assert!(!market_data_is_stale(now_ms, Some(60_000)));
    }

    #[test]
    fn timeout_place_error_stays_unknown() {
        let timeout =
            EngineError::CustomError(anyhow::anyhow!("k-paper: open_order timeout (15s)"));
        assert!(place_error_is_unknown(&timeout));
        let reject = EngineError::CustomError(anyhow::anyhow!("lab_reject"));
        assert!(!place_error_is_unknown(&reject));
        let dup = EngineError::CustomError(anyhow::anyhow!("duplicate client_order_id"));
        assert!(!place_error_is_unknown(&dup));
        let missing = EngineError::CustomError(anyhow::anyhow!("Response missing order data"));
        assert!(place_error_is_unknown(&missing));
        let generic = EngineError::CustomError(anyhow::anyhow!("something went wrong"));
        assert!(place_error_is_unknown(&generic));
    }
}
