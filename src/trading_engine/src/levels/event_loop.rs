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
use trading_core::types::market_data::StructuredOrderbook;

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
    pairs: Vec<Pair>,
    reconciliation_interval_secs: u64,
    /// Last seen prices for periodic risk checks (one per pair)
    last_prices: Arc<RwLock<HashMap<Pair, Decimal>>>,
}

impl LevelBasedEventLoop {
    /// Create a new level-based event loop
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        level_executor: Arc<LevelExecutor>,
        risk_manager: Arc<RiskManager>,
        cancellation_manager: Arc<CancellationManager>,
        indicator_subscriber: Arc<RwLock<IndicatorSubscriber>>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        market_data_rx: MarketDataUpdateReceiver,
        pairs: Vec<Pair>,
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
            pairs,
            reconciliation_interval_secs,
            last_prices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Main event loop
    pub async fn run(&self) -> Result<(), EngineError> {
        tracing::debug!("[LEVELS EVENT LOOP] ========== run() method ENTERED ==========");
        // Start periodic reconciliation using TradingStateManager for all pairs
        let trading_state_manager_for_recon = Arc::clone(&self.trading_state_manager);
        let pairs_for_recon = self.pairs.clone();
        let reconciliation_interval_secs = self.reconciliation_interval_secs;
        tokio::spawn(async move {
            tracing::info!(
                "Starting periodic trading-level reconciliation for {} pair(s) (interval: {} seconds)",
                pairs_for_recon.len(),
                reconciliation_interval_secs
            );
            let mut interval = interval(Duration::from_secs(reconciliation_interval_secs));
            loop {
                interval.tick().await;

                // Reconcile all pairs
                for pair in &pairs_for_recon {
                    tracing::debug!("Running periodic trading-level reconciliation for {}", pair);

                    // Reconcile using TradingStateManager
                    match trading_state_manager_for_recon.reconcile(pair, None).await {
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
                            tracing::error!(
                                "Trading-level reconciliation error for {}: {}",
                                pair,
                                e
                            );
                        }
                    }
                }
            }
        });

        // Start indicator subscriber in background
        tracing::info!("[LEVELS EVENT LOOP] Getting subscriber cache...");
        let subscriber_cache = self.indicator_subscriber.read().await.cache();
        tracing::info!("[LEVELS EVENT LOOP] Subscriber cache obtained");

        // Consume market data updates from channel
        tracing::info!("[LEVELS EVENT LOOP] Resubscribing to market_data_rx...");
        let mut market_data_rx = self.market_data_rx.resubscribe();
        tracing::info!("[LEVELS EVENT LOOP] Market data resubscribe complete");
        let mut periodic_timer = interval(Duration::from_secs(1)); // Check every second
        tracing::info!("[LEVELS EVENT LOOP] Periodic timer created");

        tracing::info!(
            "[LEVELS EVENT LOOP] Starting event loop, waiting for market data updates..."
        );
        loop {
            tokio::select! {
                // Handle market data updates from channel
                update = market_data_rx.recv() => {
                    tracing::debug!("[LEVELS EVENT LOOP] Received update from market_data_rx channel");
                    match update {
                        Ok(MarketDataUpdate::Orderbook { orderbook }) => {
                            tracing::debug!(
                                "Received orderbook update for {}/{} (mid_price={:?})",
                                orderbook.pair.base,
                                orderbook.pair.quote,
                                orderbook.mid_price()
                            );
                            // Update last seen price for periodic risk checks
                            if let Some(price) = orderbook.mid_price() {
                                // Store price for the orderbook's pair
                                self.last_prices.write().await.insert(orderbook.pair.clone(), price);
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
        orderbook: &StructuredOrderbook,
        indicator_cache: &Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
    ) -> Result<(), EngineError> {
        // Get indicators from cache (read fresh each time to avoid stale data)
        let _indicators: HashMap<SmolStr, IndicatorValue> = {
            let cache = indicator_cache.read().await;
            let cloned = cache.clone();
            let available: Vec<_> = cloned.keys().map(|k| k.as_str()).collect();
            tracing::debug!(
                "[LEVELS EVENT LOOP] Reading indicator cache: {} indicators available: {:?}",
                cloned.len(),
                available
            );
            drop(cache); // Release lock immediately
            cloned
        };

        // Get levels ready to post for THIS orderbook's pair
        // Filter by pair to only process levels matching the current orderbook
        let level_ids: Vec<SmolStr> = {
            let manager = self.level_manager.read().await;
            let all_levels = manager.get_all_levels();
            tracing::debug!(
                "Total levels: {}, checking which are ready to post for {}/{}...",
                all_levels.len(),
                orderbook.pair.base,
                orderbook.pair.quote
            );

            // Filter levels by pair AND ready to post
            let ready_levels: Vec<_> = all_levels
                .iter()
                .filter(|level| {
                    // Match the orderbook's pair
                    level.pair.base == orderbook.pair.base
                        && level.pair.quote == orderbook.pair.quote
                })
                .filter(|level| level.is_ready_to_post())
                .collect();

            if !ready_levels.is_empty() {
                tracing::debug!(
                    "Found {} level(s) ready to post for {}/{}: {:?}",
                    ready_levels.len(),
                    orderbook.pair.base,
                    orderbook.pair.quote,
                    ready_levels
                        .iter()
                        .map(|l| l.config.id.as_str())
                        .collect::<Vec<_>>()
                );
            } else {
                tracing::info!(
                    "No levels ready to post for {}/{} (checking level states...)",
                    orderbook.pair.base,
                    orderbook.pair.quote
                );
                for level in all_levels.iter() {
                    if level.pair.base == orderbook.pair.base
                        && level.pair.quote == orderbook.pair.quote
                    {
                        tracing::info!(
                            "Level {}: state={:?}, enabled={}, active_orders={}, pair={}/{}",
                            level.config.id,
                            level.state,
                            level.config.enabled,
                            level.active_orders.len(),
                            level.pair.base,
                            level.pair.quote
                        );
                    }
                }
            }

            // Use the actual HashMap key (expanded ID) - we need to get it from the manager
            // Since we don't have the key, we'll need to reconstruct it or store it
            // For now, let's get levels by iterating and matching
            let mut matching_level_ids = Vec::new();
            for level in ready_levels {
                // Reconstruct the expanded ID: "buy_level_1" -> "buy_level_1_ETHUSDC"
                let pair_str = level.pair.convert_to_smolstr();
                let expanded_id = SmolStr::new(&format!("{}_{}", level.config.id, pair_str));
                matching_level_ids.push(expanded_id);
            }
            matching_level_ids
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
                Some(l) => {
                    // Verify the level matches the orderbook's pair
                    if l.pair.base != orderbook.pair.base || l.pair.quote != orderbook.pair.quote {
                        tracing::debug!(
                            "Skipping level {} - pair mismatch: level={}/{}, orderbook={}/{}",
                            level_id,
                            l.pair.base,
                            l.pair.quote,
                            orderbook.pair.base,
                            orderbook.pair.quote
                        );
                        continue;
                    }
                    l
                }
                None => {
                    tracing::warn!(
                        "Level {} not found in manager (this should not happen)",
                        level_id
                    );
                    continue;
                }
            };
            // Get trading state
            let trading_state = self.trading_state_manager.get_state();
            let state = {
                let state_guard = trading_state.read().await;
                state_guard.clone()
            };

            // Resolve level - read fresh indicators from cache right before resolving
            // This ensures we have the latest indicator values
            let fresh_indicators: HashMap<SmolStr, IndicatorValue> = {
                let cache = indicator_cache.read().await;
                cache.clone()
            };

            tracing::debug!(
                "Attempting to resolve level {} for {}/{} (indicators in cache: {})",
                level.config.id,
                level.pair.base,
                level.pair.quote,
                fresh_indicators.len()
            );
            let resolved = match self
                .level_executor
                .resolve_level(&level, &fresh_indicators, Some(orderbook), &balances)
                .await
            {
                Ok(r) => {
                    tracing::info!(
                        "Level {} resolved: price={}, quantity={}, order_type={:?}",
                        level.config.id,
                        r.resolved_price,
                        r.resolved_quantity,
                        r.resolved_order_type
                    );
                    r
                }
                Err(e) => {
                    tracing::debug!(
                        "Failed to resolve level {} for {}/{}: {}",
                        level.config.id,
                        level.pair.base,
                        level.pair.quote,
                        e
                    );
                    continue;
                }
            };

            // Perform risk checks
            // Get current price from orderbook for the level's symbol
            let current_price = orderbook.mid_price();
            let current_prices = current_price.map(|price| {
                let mut map = HashMap::new();
                map.insert(level.pair.clone(), price);
                map
            });

            tracing::debug!(
                "Performing risk check for level {} ({}/{}), current_price={:?}",
                level.config.id,
                level.pair.base,
                level.pair.quote,
                current_price
            );

            let risk_result = self
                .risk_manager
                .check_level(&resolved, &state, &balances, current_prices.as_ref())
                .await;

            if !risk_result.allowed {
                tracing::info!(
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

            tracing::debug!(
                "Risk check passed for level {} ({}/{})",
                level.config.id,
                level.pair.base,
                level.pair.quote
            );

            // Check if level is enabled (individual toggle)
            if !level.config.enabled {
                tracing::info!(
                    "Level {} ({}/{}) is disabled, skipping",
                    level.config.id,
                    level.pair.base,
                    level.pair.quote
                );
                continue; // Skip disabled levels
            }

            // CRITICAL: Check state again right before posting to prevent race condition
            // Another orderbook update might have already posted this level
            // Use expanded ID to look up the level (matching how it's stored in HashMap)
            {
                let expanded_id = {
                    let pair_str = level.pair.convert_to_smolstr();
                    SmolStr::new(&format!("{}_{}", level.config.id, pair_str))
                };
                let manager = self.level_manager.read().await;
                if let Some(current_level) = manager.get_level(&expanded_id) {
                    if !current_level.is_ready_to_post() {
                        tracing::info!(
                            "Level {} ({}/{}) is no longer ready to post (state={:?}, active_orders={}), skipping",
                            level.config.id,
                            level.pair.base,
                            level.pair.quote,
                            current_level.state,
                            current_level.active_orders.len()
                        );
                        continue;
                    }
                } else {
                    tracing::warn!(
                        "Level {} ({}/{}) no longer exists in manager (expanded_id={}), skipping",
                        level.config.id,
                        level.pair.base,
                        level.pair.quote,
                        expanded_id
                    );
                    continue;
                }
            }

            tracing::info!(
                "Attempting to post level {} ({}/{}) - price={}, quantity={}",
                level.config.id,
                level.pair.base,
                level.pair.quote,
                resolved.resolved_price,
                resolved.resolved_quantity
            );

            // Post level (LevelExecutor will also check global trading control)
            // Use fresh indicators for posting as well
            match self
                .level_executor
                .post_level(&level, &fresh_indicators, Some(orderbook), &balances)
                .await
            {
                Ok(order) => {
                    tracing::info!(
                        "[SUCCESS] Level {} posted order: {} for {}/{}",
                        level.config.id,
                        order.order_id.as_str(),
                        level.pair.base,
                        level.pair.quote
                    );

                    // Update level state
                    {
                        // Use expanded ID to look up the level
                        let expanded_id = {
                            let pair_str = level.pair.convert_to_smolstr();
                            SmolStr::new(&format!("{}_{}", level.config.id, pair_str))
                        };
                        let mut manager = self.level_manager.write().await;
                        if let Some(level_mut) = manager.get_level_mut(&expanded_id) {
                            level_mut.state = super::LevelState::Posted;
                            level_mut.active_orders.push(order.order_id.clone());
                            level_mut.last_posted_at =
                                Some(trading_core::utils::current_timestamp());
                        } else {
                            tracing::warn!(
                                "Level {} ({}/{}) not found when updating state (expanded_id={})",
                                level.config.id,
                                level.pair.base,
                                level.pair.quote,
                                expanded_id
                            );
                        }
                    }

                    // Add order to TradingStateManager (tracks orders we placed)
                    // Add order to TradingStateManager (single source of truth)
                    self.trading_state_manager.add_order(order.clone()).await;

                    // Post-trade risk check
                    let current_price = orderbook.mid_price();
                    if let Some(price) = current_price {
                        let mut current_prices = HashMap::new();
                        current_prices.insert(level.pair.clone(), price);

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
                    tracing::warn!(
                        "Failed to post level {} for {}/{}: {}",
                        level.config.id,
                        level.pair.base,
                        level.pair.quote,
                        e
                    );
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

        // Periodic risk check for all pairs
        let last_prices_guard = self.last_prices.read().await;
        let current_prices: HashMap<Pair, Decimal> = last_prices_guard.clone();
        drop(last_prices_guard);

        if !current_prices.is_empty() {
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
                        tracing::debug!("Level {} paused after {} reposts", level_id, max_reposts);
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
