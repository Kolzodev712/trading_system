use crate::{reconciliation::PositionReconciler, state::Strategy};
use anyhow;
use trading_core::{
    circuit_breaker::{order_circuit_breaker, CircuitBreaker},
    error::EngineError,
    redis::{RedisMessage, ORDERBOOK_CHANNEL},
    retry::{order_retry_config, retry_with_backoff},
    traits::TradingClient,
    types::{common::Pair, exchange::Order},
};
// ExecutionReport removed - use Order directly from channels
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

/// Executes a trading strategy by:
/// 1. Consuming market data from Redis
/// 2. Calling strategy.on_orderbook() for each update
/// 3. Placing orders through the trading client
/// 4. Handling execution reports
/// 5. Periodic position reconciliation
/// 6. Circuit breaker for error handling
pub struct StrategyExecutor<S: Strategy + 'static> {
    strategy: Arc<RwLock<S>>,
    trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
    redis_url: String,
    symbol: Pair,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    #[allow(dead_code)] // Used in spawned tasks
    reconciler: PositionReconciler,
    order_timeout_secs: u64,
    min_order_size: Decimal,
    step_size: Decimal,
    tick_size: Decimal,
    min_notional: Decimal,
}

impl<S: Strategy> StrategyExecutor<S> {
    pub fn new(
        strategy: Arc<RwLock<S>>,
        trading_client: Arc<RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>>,
        redis_url: String,
        symbol: Pair,
        min_order_size: Decimal,
        step_size: Decimal,
        tick_size: Decimal,
        min_notional: Decimal,
    ) -> Self {
        let reconciler = PositionReconciler::new(Arc::clone(&trading_client));
        Self {
            strategy,
            trading_client,
            redis_url,
            symbol,
            circuit_breaker: Arc::new(Mutex::new(order_circuit_breaker())),
            reconciler,
            order_timeout_secs: 300, // 5 minutes default timeout for limit orders
            min_order_size,
            step_size,
            tick_size,
            min_notional,
        }
    }

    pub fn with_order_timeout(mut self, timeout_secs: u64) -> Self {
        self.order_timeout_secs = timeout_secs;
        self
    }

    /// Main execution loop - consumes orderbooks from Redis and executes strategy
    pub async fn run(&self) -> Result<(), EngineError> {
        debug!(
            "Starting strategy executor for {symbol:?}",
            symbol = self.symbol
        );

        let strategy = Arc::clone(&self.strategy);
        let trading_client = Arc::clone(&self.trading_client);
        let symbol = self.symbol.clone();
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let order_timeout = self.order_timeout_secs;

        // Start periodic reconciliation task
        let strategy_for_recon = Arc::clone(&strategy);
        let symbol_for_recon = symbol.clone();
        let trading_client_for_recon = Arc::clone(&trading_client);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes
            loop {
                interval.tick().await;
                // Get the last known price before acquiring write lock
                let current_price = {
                    let strat = strategy_for_recon.read().await;
                    strat.get_last_price()
                };

                let mut strat = strategy_for_recon.write().await;
                let state = strat.get_state_mut();

                // Create a new reconciler for this task to avoid lifetime issues
                let task_reconciler =
                    PositionReconciler::new(Arc::clone(&trading_client_for_recon));
                match task_reconciler
                    .reconcile(state, &symbol_for_recon, current_price)
                    .await
                {
                    Ok(result) => {
                        if result.reconciled {
                            debug!(
                                "Position reconciled for {symbol}: internal={internal_qty}, exchange={exchange_qty}, discrepancy={discrepancy}",
                                symbol = result.symbol,
                                internal_qty = result.internal_quantity,
                                exchange_qty = result.exchange_quantity,
                                discrepancy = result.discrepancy
                            );
                        }
                    }
                    Err(e) => {
                        // Check if it's a connection error - these are expected during reconnection
                        let error_msg = format!("{e}");
                        if error_msg.contains("Channel Closed")
                            || error_msg.contains("Connection Closed")
                        {
                            debug!(
                                "Position reconciliation skipped - WebSocket reconnecting. Will retry on next interval."
                            );
                        } else {
                            warn!("Position reconciliation failed: {e}");
                        }
                    }
                }
            }
        });

        // Start stale order cleanup task
        let strategy_for_cleanup = Arc::clone(&strategy);
        let trading_client_for_cleanup = Arc::clone(&trading_client);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60)); // Every minute
            loop {
                interval.tick().await;
                let stale_orders = {
                    let strat = strategy_for_cleanup.read().await;
                    strat.get_state().get_stale_orders(order_timeout)
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
                        // Update order status to Canceled
                        let mut strat = strategy_for_cleanup.write().await;
                        strat.get_state_mut().update_order_status(
                            &order_id,
                            trading_core::types::common::OrderStatus::Canceled,
                            None,
                        );
                    }
                }
            }
        });

        // Subscribe to orderbook channel
        let redis_url: redis::ConnectionInfo = self.redis_url.parse().map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!(
                "Invalid Redis URL '{url}': {error}. Please check your Redis configuration.",
                url = self.redis_url,
                error = e
            ))
        })?;
        let mut pubsub_conn = redis::Client::open(redis_url)
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to open Redis client: {}", e))
            })?
            .get_async_pubsub()
            .await
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!("Failed to get Redis pubsub: {}", e))
            })?;
        pubsub_conn
            .subscribe(ORDERBOOK_CHANNEL)
            .await
            .map_err(|e| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to subscribe to Redis channel: {}",
                    e
                ))
            })?;

        let mut stream = pubsub_conn.on_message();
        while let Some(message) = stream.next().await {
            let payload: String = message
                .get_payload()
                .map_err(|e| EngineError::CustomError(anyhow::anyhow!(
                    "Failed to get Redis message payload: {error}. This may indicate a Redis connection issue.",
                    error = e
                )))?;

            match serde_json::from_str::<RedisMessage>(&payload) {
                Ok(RedisMessage::OrderbookMessage(orderbook)) => {
                    // Get strategy decision
                    let order_request = {
                        let mut strat = strategy.write().await;
                        let state = strat.get_state().clone();
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

                        // Check active orders limit
                        let active_count = {
                            let strat = strategy.read().await;
                            strat.get_state().get_active_order_count(&symbol)
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

                        // Round quantity to step size (required by Binance LOT_SIZE filter)
                        // Round down to avoid exceeding available balance
                        let rounded_amount = (req.amount / self.step_size).floor() * self.step_size;
                        if rounded_amount != req.amount {
                            debug!(
                                "Order amount {amount} rounded to step size {step}: {rounded}",
                                amount = req.amount,
                                step = self.step_size,
                                rounded = rounded_amount
                            );
                            req.amount = rounded_amount;
                        }

                        // Enforce minimum order size from config
                        if req.amount < self.min_order_size {
                            debug!(
                                "Order amount {amount} is below minimum {min}. Skipping order.",
                                amount = req.amount,
                                min = self.min_order_size
                            );
                            continue;
                        }

                        // Round price to tick size (required by Binance PRICE_FILTER)
                        let rounded_price = (req.price / self.tick_size).floor() * self.tick_size;
                        if rounded_price != req.price {
                            debug!(
                                "Order price {price} rounded to tick size {tick}: {rounded}",
                                price = req.price,
                                tick = self.tick_size,
                                rounded = rounded_price
                            );
                            req.price = rounded_price;
                        }

                        // Check minimum notional value (price * quantity)
                        let notional = req.price * req.amount;
                        if notional < self.min_notional {
                            debug!(
                                "Order notional {notional} is below minimum {min}. Skipping order. (price={price}, amount={amount})",
                                notional = notional,
                                min = self.min_notional,
                                price = req.price,
                                amount = req.amount
                            );
                            continue;
                        }

                        // Place the order with retry logic
                        let retry_config = order_retry_config();
                        let client_clone = Arc::clone(&trading_client);
                        let req_clone = req.clone();

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
                            let req = req_clone.clone();
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
                                    side = req_clone.side,
                                    symbol = req_clone.symbol,
                                    price = req_clone.price,
                                    amount = req_clone.amount
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

                                // Add to active orders
                                let mut strat = strategy.write().await;
                                strat.get_state_mut().add_order(order);
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to place order after {max_retries} retries: {error}. Order request: side={side}, symbol={symbol}, price={price}, amount={amount}",
                                    max_retries = retry_config.max_attempts,
                                    error = e,
                                    side = req_clone.side,
                                    symbol = req_clone.symbol,
                                    price = req_clone.price,
                                    amount = req_clone.amount
                                );

                                // Record failure in circuit breaker
                                circuit_breaker.lock().await.record_failure();
                            }
                        }
                    }
                }
                Ok(RedisMessage::TradeMessage(_)) => {
                    // Ignore trades for now
                }
                Err(e) => {
                    debug!(
                        "Failed to parse Redis message: {error}. Payload: {payload:.100}...",
                        error = e,
                        payload = payload
                    );
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
