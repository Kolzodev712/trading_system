//! Level-based event loop - orchestrates level execution with risk checks and cancellation logic
use super::cancellation::CancellationManager;
use super::{IndicatorSubscriber, LevelExecutor, LevelManager};
use crate::risk::RiskManager;
use indicator_engine::data::IndicatorValue;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use trading_core::channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver};
use trading_core::error::EngineError;
use trading_core::traits::TradingClient;
use trading_core::types::common::Pair;
use trading_core::types::exchange::{Order, OrderRequest};
use trading_core::types::market_data::LegacyOrderbook as Orderbook;

/// Level-based event loop
pub struct LevelBasedEventLoop {
    level_manager: Arc<RwLock<LevelManager>>,
    level_executor: Arc<LevelExecutor>,
    risk_manager: Arc<RiskManager>,
    cancellation_manager: Arc<CancellationManager>,
    indicator_subscriber: Arc<RwLock<IndicatorSubscriber>>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    trading_state_manager: Arc<crate::state::manager::TradingStateManager>,
    market_data_rx: MarketDataUpdateReceiver,
    symbol: Pair,
    reconciliation_interval_secs: u64,
    /// Last seen price for periodic risk checks
    last_price: Arc<RwLock<Option<Decimal>>>,
}

impl LevelBasedEventLoop {
    /// Create a new level-based event loop
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        level_executor: Arc<LevelExecutor>,
        risk_manager: Arc<RiskManager>,
        cancellation_manager: Arc<CancellationManager>,
        indicator_subscriber: Arc<RwLock<IndicatorSubscriber>>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        market_data_rx: MarketDataUpdateReceiver,
        symbol: Pair,
        trading_state_manager: Arc<crate::state::manager::TradingStateManager>,
        reconciliation_interval_secs: u64,
    ) -> Self {
        Self {
            level_manager,
            level_executor,
            risk_manager,
            cancellation_manager,
            indicator_subscriber,
            trading_client,
            trading_state_manager,
            market_data_rx,
            symbol,
            reconciliation_interval_secs,
            last_price: Arc::new(RwLock::new(None)),
        }
    }

    /// Main event loop
    pub async fn run(&self) -> Result<(), EngineError> {
        // Start periodic reconciliation using TradingStateManager
        let trading_state_manager_for_recon = Arc::clone(&self.trading_state_manager);
        let symbol_for_recon = self.symbol.clone();
        let reconciliation_interval_secs = self.reconciliation_interval_secs;
        tokio::spawn(async move {
            tracing::info!(
                "Starting periodic trading-level reconciliation (interval: {} seconds)",
                reconciliation_interval_secs
            );
            let mut interval = interval(Duration::from_secs(reconciliation_interval_secs));
            loop {
                interval.tick().await;

                tracing::info!(
                    "Running periodic trading-level reconciliation for {}",
                    symbol_for_recon
                );

                // Reconcile using TradingStateManager
                match trading_state_manager_for_recon
                    .reconcile(&symbol_for_recon, None)
                    .await
                {
                    Ok(result) => {
                        if result.reconciled {
                            tracing::info!(
                                "Trading-level reconciliation completed for {symbol}: internal={internal_qty}, exchange={exchange_qty}, discrepancy={discrepancy}",
                                symbol = result.symbol,
                                internal_qty = result.internal_quantity,
                                exchange_qty = result.exchange_quantity,
                                discrepancy = result.discrepancy
                            );
                        } else {
                            tracing::info!(
                                "Trading-level reconciliation for {symbol}: no discrepancies found",
                                symbol = result.symbol
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Trading-level reconciliation error: {e}");
                    }
                }
            }
        });

        // Start indicator subscriber in background
        let subscriber_cache = {
            let subscriber = self.indicator_subscriber.read().await;
            subscriber.cache()
        };

        // Consume market data updates from channel
        let mut market_data_rx = self.market_data_rx.resubscribe();
        let mut periodic_timer = interval(Duration::from_secs(1)); // Check every second

        loop {
            tokio::select! {
                // Handle market data updates from channel
                update = market_data_rx.recv() => {
                    match update {
                        Ok(MarketDataUpdate::Orderbook { orderbook, .. }) => {
                            // Update last seen price for periodic risk checks
                            if !orderbook.bids.is_empty() && !orderbook.asks.is_empty() {
                                let price = (orderbook.bids[0] + orderbook.asks[0]) / Decimal::from(2);
                                *self.last_price.write().await = Some(price);
                            }

                            if let Err(e) = self.handle_orderbook(&orderbook, &subscriber_cache).await {
                                tracing::error!("Error handling orderbook: {}", e);
                            }
                        }
                        Ok(MarketDataUpdate::Trade { .. }) => {
                            // Ignore trades for now
                            tracing::debug!("Received trade update (ignored)");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                "Market data receiver lagged, skipped {skipped} messages. This may indicate slow processing."
                            );
                            // Continue receiving - broadcast channels automatically skip lagged messages
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("Market data channel closed - shutting down level-based event loop");
                            return Ok(());
                        }
                    }
                }

                // Periodic tasks
                _ = periodic_timer.tick() => {
                    // Handle reposting and cancellation checks
                    if let Err(e) = self.handle_periodic_tasks(&subscriber_cache).await {
                        tracing::error!("Error in periodic tasks: {}", e);
                    }
                }
            }
        }
    }

    async fn handle_orderbook(
        &self,
        orderbook: &Orderbook,
        indicator_cache: &Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
    ) -> Result<(), EngineError> {
        // Get indicators from cache
        let indicators: HashMap<SmolStr, IndicatorValue> = {
            let cache = indicator_cache.read().await;
            cache.clone()
        };

        // Get levels ready to post (clone IDs to avoid lifetime issues)
        let level_ids: Vec<SmolStr> = {
            let manager = self.level_manager.read().await;
            manager
                .get_levels_ready_to_post()
                .iter()
                .map(|level| level.config.id.clone())
                .collect()
        };

        // Get balances
        let balances = {
            let client = self.trading_client.read().await;
            client.fetch_balance().await?
        };

        // Process each level
        for level_id in level_ids {
            // Get the level again (need to re-acquire lock)
            let level = {
                let manager = self.level_manager.read().await;
                manager.get_level(&level_id).cloned()
            };

            let level = match level {
                Some(l) => l,
                None => continue,
            };
            // Get trading state
            let trading_state = self.trading_state_manager.get_state();
            let state = {
                let state_guard = trading_state.read().await;
                state_guard.clone()
            };

            // Resolve level
            let resolved = self
                .level_executor
                .resolve_level(&level, &indicators, Some(orderbook), &balances)
                .await?;

            // Perform risk checks
            // Get current price from orderbook (LegacyOrderbook has bids/asks as Vec<Decimal>)
            let current_price = if !orderbook.bids.is_empty() && !orderbook.asks.is_empty() {
                Some((orderbook.bids[0] + orderbook.asks[0]) / Decimal::from(2))
            } else {
                None
            };
            let current_prices = current_price.map(|price| {
                let mut map = HashMap::new();
                map.insert(self.symbol.clone(), price);
                map
            });
            let risk_result = self
                .risk_manager
                .check_level(&resolved, &state, &balances, current_prices.as_ref())
                .await;

            if !risk_result.allowed {
                tracing::debug!(
                    "Level {} blocked by risk check: {}",
                    level.config.id,
                    risk_result
                        .reason
                        .as_ref()
                        .map(|r| r.as_str())
                        .unwrap_or("unknown")
                );
                continue;
            }

            // Post level
            match self
                .level_executor
                .post_level(&level, &indicators, Some(orderbook), &balances)
                .await
            {
                Ok(order) => {
                    tracing::info!(
                        "Level {} posted order: {}",
                        level.config.id,
                        order.order_id.as_str()
                    );

                    // Update level state
                    {
                        let mut manager = self.level_manager.write().await;
                        if let Some(level_mut) = manager.get_level_mut(&level.config.id) {
                            level_mut.state = super::LevelState::Posted;
                            level_mut.active_orders.push(order.order_id.clone());
                            level_mut.last_posted_at =
                                Some(trading_core::utils::current_timestamp());
                        }
                    }

                    // Add order to TradingStateManager (tracks orders we placed)
                    // Add order to TradingStateManager (single source of truth)
                    self.trading_state_manager.add_order(order.clone()).await;

                    // Post-trade risk check
                    let current_price = if !orderbook.bids.is_empty() && !orderbook.asks.is_empty()
                    {
                        Some((orderbook.bids[0] + orderbook.asks[0]) / Decimal::from(2))
                    } else {
                        None
                    };
                    if let Some(price) = current_price {
                        let mut current_prices = HashMap::new();
                        current_prices.insert(self.symbol.clone(), price);

                        // Get fresh trading state after order was added
                        let trading_state = self.trading_state_manager.get_state();
                        let state_after_trade = {
                            let state_guard = trading_state.read().await;
                            state_guard.clone()
                        };

                        let post_trade_result = self
                            .risk_manager
                            .check_post_trade(&state_after_trade, &current_prices)
                            .await;

                        if !post_trade_result.allowed {
                            tracing::warn!(
                                "Post-trade risk check failed for order {}: {}",
                                order.order_id.as_str(),
                                post_trade_result
                                    .reason
                                    .as_ref()
                                    .map(|r| r.as_str())
                                    .unwrap_or("unknown")
                            );
                            // Note: Order is already placed, but we log the warning
                            // In the future, we could cancel the order or trigger circuit breaker
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to post level {}: {}", level.config.id, e);
                }
            }
        }

        Ok(())
    }

    async fn handle_periodic_tasks(
        &self,
        _indicator_cache: &Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
    ) -> Result<(), EngineError> {
        // Check cancellation conditions
        let balances = {
            let client = self.trading_client.read().await;
            client.fetch_balance().await?
        };

        let trading_state = self.trading_state_manager.get_state();
        let state = {
            let state_guard = trading_state.read().await;
            state_guard.clone()
        };

        let cancellation_actions = self
            .cancellation_manager
            .check_conditions(
                &state,
                &balances,
                rust_decimal::Decimal::ZERO, // TODO: Calculate daily P&L
            )
            .await;

        if !cancellation_actions.is_empty() {
            let manager = self.level_manager.read().await;
            self.cancellation_manager
                .execute_cancellations(
                    cancellation_actions,
                    Arc::clone(&self.trading_client),
                    &manager,
                )
                .await?;
        }

        // Handle level reposting
        self.handle_reposting().await?;

        // Periodic risk check
        if let Some(price) = *self.last_price.read().await {
            let mut current_prices = HashMap::new();
            current_prices.insert(self.symbol.clone(), price);

            let trading_state = self.trading_state_manager.get_state();
            let state = {
                let state_guard = trading_state.read().await;
                state_guard.clone()
            };

            let periodic_result = self
                .risk_manager
                .check_periodic(&state, &current_prices)
                .await;

            if !periodic_result.allowed {
                tracing::warn!(
                    "Periodic risk check failed: {}",
                    periodic_result
                        .reason
                        .as_ref()
                        .map(|r| r.as_str())
                        .unwrap_or("unknown")
                );
                // Circuit breaker will block future trades
            }
        }

        Ok(())
    }

    async fn handle_reposting(&self) -> Result<(), EngineError> {
        let mut manager = self.level_manager.write().await;

        // Find levels that need reposting
        let levels_to_repost: Vec<SmolStr> = {
            manager
                .get_all_levels()
                .iter()
                .filter_map(|level| match level.state {
                    super::LevelState::Filled | super::LevelState::Cancelled => {
                        if manager.should_repost(&level.config.id).unwrap_or(false) {
                            Some(level.config.id.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect()
        };

        // Reset levels to Created state for reposting
        for level_id in levels_to_repost {
            if let Some(level) = manager.get_level_mut(&level_id) {
                level.state = super::LevelState::Created;
                level.repost_count += 1;

                // Check max reposts
                if let Some(max_reposts) = level.config.lifecycle.max_reposts {
                    if level.repost_count >= max_reposts {
                        level.state = super::LevelState::Paused;
                        tracing::info!("Level {} paused after {} reposts", level_id, max_reposts);
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle execution report
    pub async fn handle_execution_report(&self, order: &Order) -> Result<(), EngineError> {
        // Find level that owns this order
        let level_id = {
            let manager = self.level_manager.read().await;
            manager
                .get_all_levels()
                .iter()
                .find(|level| level.active_orders.contains(&order.order_id))
                .map(|level| level.config.id.clone())
        };

        if let Some(level_id) = level_id {
            let mut manager = self.level_manager.write().await;

            match order.status {
                trading_core::types::common::OrderStatus::Filled => {
                    manager
                        .handle_order_filled(&level_id, &order.order_id)
                        .await?;

                    // TradingStateManager automatically handles order updates via ExchangeStateManager
                    // No manual update needed
                }
                trading_core::types::common::OrderStatus::Canceled
                | trading_core::types::common::OrderStatus::Rejected
                | trading_core::types::common::OrderStatus::Expired => {
                    manager
                        .handle_order_cancelled(&level_id, &order.order_id)
                        .await?;

                    // Remove order from TradingStateManager
                    self.trading_state_manager
                        .remove_order(&order.order_id)
                        .await;
                }
                _ => {
                    // TradingStateManager automatically handles order updates via ExchangeStateManager
                    // No manual update needed
                }
            }
        }

        Ok(())
    }
}
