use crate::state::Strategy;
use asset_engine::AssetService;
use trading_core::{
    channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver},
    circuit_breaker::{order_circuit_breaker, CircuitBreaker},
    error::EngineError,
    retry::{order_retry_config, retry_with_backoff},
    traits::TradingClient,
    types::{common::Exchange, exchange::Order, Pair},
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
    symbol: Pair,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    trading_state_manager: Arc<crate::state::manager::TradingStateManager>,
    order_timeout_secs: u64,
    reconciliation_interval_secs: u64,
    asset_service: Option<Arc<AssetService>>,
    exchange: Exchange,
    /// Config fallback values
    config_min_order_size: Decimal,
    config_step_size: Decimal,
    config_tick_size: Decimal,
    config_min_notional: Decimal,
    /// Trading control - checks if trading is enabled (optional, for TUI control)
    trading_enabled: Option<Arc<tokio::sync::RwLock<bool>>>,
}

impl<S: Strategy> StrategyExecutor<S> {
    pub fn new(
        strategy: Arc<RwLock<S>>,
        trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
        market_data_rx: MarketDataUpdateReceiver,
        symbol: Pair,
        trading_state_manager: Arc<crate::state::manager::TradingStateManager>,
        reconciliation_interval_secs: u64,
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        config_min_order_size: Decimal,
        config_step_size: Decimal,
        config_tick_size: Decimal,
        config_min_notional: Decimal,
    ) -> Self {
        Self {
            strategy,
            trading_client,
            market_data_rx,
            symbol,
            circuit_breaker: Arc::new(Mutex::new(order_circuit_breaker())),
            trading_state_manager,
            order_timeout_secs: 300, // 5 minutes default timeout for limit orders
            reconciliation_interval_secs,
            asset_service,
            exchange,
            config_min_order_size,
            config_step_size,
            config_tick_size,
            config_min_notional,
            trading_enabled: None,
        }
    }

    /// Set trading enabled flag (for TUI control)
    pub fn with_trading_control(mut self, trading_enabled: Arc<tokio::sync::RwLock<bool>>) -> Self {
        self.trading_enabled = Some(trading_enabled);
        self
    }

    pub fn with_order_timeout(mut self, timeout_secs: u64) -> Self {
        self.order_timeout_secs = timeout_secs;
        self
    }

    /// Main execution loop - consumes orderbooks from channels and executes strategy
    pub async fn run(&self) -> Result<(), EngineError> {
        debug!(
            "Starting strategy executor for {symbol:?}",
            symbol = self.symbol
        );

        let strategy = Arc::clone(&self.strategy);
        let trading_client = Arc::clone(&self.trading_client);
        let symbol = self.symbol.clone();
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let mut market_data_rx = self.market_data_rx.resubscribe();

        // Start periodic reconciliation task
        // Start periodic reconciliation using TradingStateManager
        let trading_state_manager_for_recon = Arc::clone(&self.trading_state_manager);
        let symbol_for_recon = self.symbol.clone();
        let strategy_for_price = Arc::clone(&strategy);
        let reconciliation_interval_secs = self.reconciliation_interval_secs;
        tokio::spawn(async move {
            info!(
                "Starting periodic trading-level reconciliation (interval: {} seconds)",
                reconciliation_interval_secs
            );
            let mut interval = interval(Duration::from_secs(reconciliation_interval_secs));
            loop {
                interval.tick().await;

                info!(
                    "Running periodic trading-level reconciliation for {}",
                    symbol_for_recon
                );

                // Get current price from strategy if available
                let current_price = {
                    let strat = strategy_for_price.read().await;
                    strat.get_last_price()
                };

                // Reconcile using TradingStateManager
                match trading_state_manager_for_recon
                    .reconcile(&symbol_for_recon, current_price)
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
                        error!("Trading-level reconciliation error: {e}");
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
                let stale_orders = {
                    let state_guard = trading_state.read().await;
                    state_guard.get_stale_orders(order_timeout)
                };

                for order_id in stale_orders {
                    warn!(
                        "Cancelling stale order: {order_id}",
                        order_id = order_id.as_str()
                    );
                    if let Err(e) = trading_client_for_cleanup
                        .write()
                        .await
                        .cancel_order_by_id(order_id.clone())
                        .await
                    {
                        error!(
                            "Failed to cancel stale order {order_id}: {e}",
                            order_id = order_id.as_str()
                        );
                    } else {
                        // Remove order from TradingStateManager
                        trading_state_manager_for_cleanup
                            .remove_order(&order_id)
                            .await;
                    }
                }
            }
        });

        // Consume market data updates from channel
        loop {
            match market_data_rx.recv().await {
                Ok(MarketDataUpdate::Orderbook { orderbook, .. }) => {
                    // Get strategy decision using TradingStateManager's state
                    let order_request = {
                        let trading_state = self.trading_state_manager.get_state();
                        let state = {
                            let state_guard = trading_state.read().await;
                            state_guard.clone()
                        };
                        let mut strat = strategy.write().await;
                        match strat.on_orderbook(&orderbook, &state).await {
                            Ok(req) => req,
                            Err(e) => {
                                error!("Strategy error in on_orderbook: {error}", error = e);
                                continue;
                            }
                        }
                    };

                    // If strategy wants to place an order, do it
                    if let Some(mut req) = order_request {
                        debug!(
                            "Strategy requested order: side={side}, symbol={symbol}, price={price}, amount={amount}",
                            side = req.side,
                            symbol = req.symbol,
                            price = req.price,
                            amount = req.amount
                        );

                        // Check circuit breaker
                        let cb = circuit_breaker.lock().await;
                        if !cb.can_proceed() {
                            debug!(
                                "Circuit breaker is open, skipping order placement. State: {state}",
                                state = cb.state()
                            );
                            continue;
                        }
                        drop(cb);

                        // Check active orders limit from TradingStateManager
                        let trading_state = self.trading_state_manager.get_state();
                        let active_count = {
                            let state = trading_state.read().await;
                            state.get_active_order_count(&symbol)
                        };

                        if active_count >= 5 {
                            debug!(
                                "Too many active orders ({active_count}), skipping order placement"
                            );
                            continue;
                        }

                        debug!(
                            "Attempting to place order (active_orders={active_count})...",
                            active_count = active_count
                        );

                        // Check balance before placing order
                        match trading_client.read().await.fetch_balance().await {
                            Ok(balances) => {
                                match req.side {
                                    trading_core::types::common::Side::Buy => {
                                        // Find quote currency balance (USDC for BTC/USDC)
                                        let quote_balance = balances
                                            .iter()
                                            .find(|b| {
                                                let balance_symbol = b.symbol.as_ref();
                                                balance_symbol == symbol.quote.as_str()
                                                    || balance_symbol
                                                        .ends_with(symbol.quote.as_str())
                                            })
                                            .map(|b| b.free)
                                            .unwrap_or(Decimal::ZERO);

                                        let required_balance = req.price * req.amount;
                                        if quote_balance < required_balance {
                                            warn!(
                                                "Insufficient balance: need {required} {quote}, have {available} {quote}. Skipping order.",
                                                required = required_balance,
                                                quote = symbol.quote,
                                                available = quote_balance
                                            );
                                            continue;
                                        }
                                        debug!(
                                            "Balance check passed: {available} {quote} >= {required} {quote}",
                                            available = quote_balance,
                                            quote = symbol.quote,
                                            required = required_balance
                                        );
                                    }
                                    trading_core::types::common::Side::Sell => {
                                        // Find base currency balance (BTC for BTC/USDC)
                                        let base_balance = balances
                                            .iter()
                                            .find(|b| {
                                                let balance_symbol = b.symbol.as_ref();
                                                balance_symbol == symbol.base.as_str()
                                                    || balance_symbol
                                                        .starts_with(symbol.base.as_str())
                                            })
                                            .map(|b| b.free)
                                            .unwrap_or(Decimal::ZERO);

                                        if base_balance < req.amount {
                                            warn!(
                                                "Insufficient balance: need {required} {base}, have {available} {base}. Skipping order.",
                                                required = req.amount,
                                                base = symbol.base,
                                                available = base_balance
                                            );
                                            continue;
                                        }
                                        debug!(
                                            "Balance check passed: {available} {base} >= {required} {base}",
                                            available = base_balance,
                                            base = symbol.base,
                                            required = req.amount
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to fetch balance for validation: {error}. Proceeding with order placement.",
                                    error = e
                                );
                            }
                        }

                        // Get trading rules from Asset Engine (with config fallback)
                        use crate::levels::order_validation::{
                            get_trading_rules, round_price, round_quantity, validate_order,
                        };
                        let rules = get_trading_rules(
                            self.asset_service.as_ref(),
                            self.exchange.clone(),
                            &self.symbol,
                            self.config_min_order_size,
                            self.config_step_size,
                            self.config_tick_size,
                            self.config_min_notional,
                        )
                        .await;

                        // Round quantity to step size (Asset Engine or config fallback)
                        let rounded_amount = round_quantity(req.amount, rules.step_size);
                        if rounded_amount != req.amount {
                            debug!(
                                "Order amount {amount} rounded to step size {step}: {rounded}",
                                amount = req.amount,
                                step = rules.step_size,
                                rounded = rounded_amount
                            );
                            req.amount = rounded_amount;
                        }

                        // Enforce minimum order size (Asset Engine or config fallback)
                        if req.amount < rules.min_order_size {
                            debug!(
                                "Order amount {amount} is below minimum {min}. Skipping order.",
                                amount = req.amount,
                                min = rules.min_order_size
                            );
                            continue;
                        }

                        // Round price to tick size (Asset Engine or config fallback)
                        let rounded_price = round_price(req.price, rules.tick_size);
                        if rounded_price != req.price {
                            debug!(
                                "Order price {price} rounded to tick size {tick}: {rounded}",
                                price = req.price,
                                tick = rules.tick_size,
                                rounded = rounded_price
                            );
                            req.price = rounded_price;
                        }

                        // Validate order against trading rules (includes min_notional check)
                        if let Err(e) = validate_order(req.price, req.amount, &rules) {
                            debug!(
                                "Order validation failed: {error}. Skipping order. (price={price}, amount={amount})",
                                error = e,
                                price = req.price,
                                amount = req.amount
                            );
                            continue;
                        }

                        // Check if trading is enabled
                        if let Some(ref trading_enabled) = self.trading_enabled {
                            if !*trading_enabled.read().await {
                                tracing::warn!(
                                    "Trading is disabled - blocking order placement from strategy"
                                );
                                continue;
                            }
                        }

                        // Place the order with retry logic
                        let retry_config = order_retry_config();
                        let client_clone = Arc::clone(&trading_client);
                        // Clone req once for retry closure
                        let req_for_retry = req.clone();

                        debug!(
                            "Calling open_order with retry (max_attempts={max_attempts})...",
                            max_attempts = retry_config.max_attempts
                        );

                        // Helper to check if error is insufficient balance (shouldn't retry)
                        let is_insufficient_balance = |e: &EngineError| -> bool {
                            let error_str = format!("{e}");
                            error_str.contains("-2010")
                                || error_str.contains("insufficient balance")
                        };

                        let result = retry_with_backoff(&retry_config, || {
                            let client = Arc::clone(&client_clone);
                            let req = req_for_retry.clone();
                            Box::pin(async move {
                                debug!("Retry attempt: calling open_order...");
                                let result = client.write().await.open_order(req).await;
                                match &result {
                                    Ok(_) => debug!("open_order succeeded"),
                                    Err(e) => {
                                        if is_insufficient_balance(e) {
                                            debug!("open_order failed: insufficient balance (will not retry): {error}", error = e);
                                        } else {
                                            debug!("open_order failed: {error}", error = e);
                                        }
                                    }
                                }
                                result
                            })
                        })
                        .await;

                        // If it's an insufficient balance error, don't retry and skip circuit breaker failure
                        if let Err(e) = &result {
                            if is_insufficient_balance(e) {
                                debug!(
                                    "Order failed due to insufficient balance (skipping retries): {error}. Order request: side={side}, symbol={symbol}, price={price}, amount={amount}",
                                    error = e,
                                    side = req_for_retry.side,
                                    symbol = req_for_retry.symbol,
                                    price = req_for_retry.price,
                                    amount = req_for_retry.amount
                                );
                                continue; // Skip to next message, don't record circuit breaker failure
                            }
                        }

                        match result {
                            Ok(mut order) => {
                                // Ensure order has New status
                                order.status = trading_core::types::common::OrderStatus::New;
                                order.filled_amount = rust_decimal::Decimal::ZERO;

                                info!(
                                    "Order placed successfully: id={order_id}, symbol={symbol}, side={side}, price={price}, amount={amount}",
                                    order_id = order.order_id.as_str(),
                                    symbol = order.symbol,
                                    side = order.side,
                                    price = order.price,
                                    amount = order.amount
                                );

                                // Record success in circuit breaker
                                circuit_breaker.lock().await.record_success();

                                // Add order to TradingStateManager (single source of truth)
                                self.trading_state_manager.add_order(order).await;
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to place order after {max_retries} retries: {error}. Order request: side={side}, symbol={symbol}, price={price}, amount={amount}",
                                    max_retries = retry_config.max_attempts,
                                    error = e,
                                    side = req_for_retry.side,
                                    symbol = req_for_retry.symbol,
                                    price = req_for_retry.price,
                                    amount = req_for_retry.amount
                                );

                                // Record failure in circuit breaker
                                circuit_breaker.lock().await.record_failure();
                            }
                        }
                    }
                }
                Ok(MarketDataUpdate::Trade { .. }) => {
                    // Ignore trades for now
                    debug!("Received trade update (ignored)");
                }
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
        strategy_guard.on_execution_report(&order).await?;

        Ok(())
    }
}
