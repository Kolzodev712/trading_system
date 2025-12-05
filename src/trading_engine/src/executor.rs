use super::{reconciliation::PositionReconciler, strategy::Strategy};
use anyhow;
use core_logic::{
    circuit_breaker::{order_circuit_breaker, CircuitBreaker},
    data::{base_data::Pair, private_data::Order},
    error::EngineError,
    redis::{RedisMessage, ORDERBOOK_CHANNEL},
    retry::{order_retry_config, retry_with_backoff},
    traits::TradingClient,
};
use exchange_engine::exchanges::binance::websocket::types::user_data::execution_report::ExecutionReport;
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
    trading_client: Arc<RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>>,
    redis_url: String,
    symbol: Pair,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    #[allow(dead_code)] // Used in spawned tasks
    reconciler: PositionReconciler,
    order_timeout_secs: u64,
}

impl<S: Strategy> StrategyExecutor<S> {
    pub fn new(
        strategy: Arc<RwLock<S>>,
        trading_client: Arc<
            RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
        >,
        redis_url: String,
        symbol: Pair,
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
        }
    }

    pub fn with_order_timeout(mut self, timeout_secs: u64) -> Self {
        self.order_timeout_secs = timeout_secs;
        self
    }

    /// Main execution loop - consumes orderbooks from Redis and executes strategy
    pub async fn run(&self) -> Result<(), EngineError> {
        info!(
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
                            info!(
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
                            core_logic::data::base_data::OrderStatus::Canceled,
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
        let mut pubsub_conn = redis::Client::open(redis_url)?.get_async_pubsub().await?;
        pubsub_conn.subscribe(ORDERBOOK_CHANNEL).await?;

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
                        strat.on_orderbook(&orderbook, &state).await?
                    };

                    // If strategy wants to place an order, do it
                    if let Some(req) = order_request {
                        // Check circuit breaker
                        let cb = circuit_breaker.lock().await;
                        if !cb.can_proceed() {
                            warn!(
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
                            warn!("Too many active orders, skipping");
                            continue;
                        }

                        // Place the order with retry logic
                        let retry_config = order_retry_config();
                        let client_clone = Arc::clone(&trading_client);
                        let req_clone = req.clone();

                        match retry_with_backoff(&retry_config, || {
                            let client = Arc::clone(&client_clone);
                            let req = req_clone.clone();
                            Box::pin(async move { client.write().await.open_order(req).await })
                        })
                        .await
                        {
                            Ok(mut order) => {
                                // Ensure order has New status
                                order.status = core_logic::data::base_data::OrderStatus::New;
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
                                error!(
                                    "Failed to place order after {max_retries} retries: {error}. Order request: {request:?}",
                                    max_retries = retry_config.max_attempts,
                                    error = e,
                                    request = req_clone
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
                    warn!(
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
    pub async fn handle_execution_report(
        &self,
        report: &ExecutionReport,
    ) -> Result<(), EngineError> {
        let order: Order = report.clone().into();

        let mut strategy_guard = self.strategy.write().await;
        strategy_guard.on_execution_report(&order).await?;

        Ok(())
    }
}
