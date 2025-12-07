//! Level-based event loop - orchestrates level execution with risk checks and cancellation logic
use super::cancellation::CancellationManager;
use super::risk::RiskManager;
use super::{IndicatorSubscriber, LevelExecutor, LevelManager};
use crate::reconciliation::PositionReconciler;
use crate::state::TradingState;
use indicator_engine::data::IndicatorValue;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tokio_stream::StreamExt;
use trading_core::error::EngineError;
use trading_core::redis::{RedisMessage, ORDERBOOK_CHANNEL};
use trading_core::traits::TradingClient;
use trading_core::types::common::Pair;
use trading_core::types::exchange::{Order, OrderRequest};
use trading_core::types::market_data::Orderbook;

/// Level-based event loop
pub struct LevelBasedEventLoop {
    level_manager: Arc<RwLock<LevelManager>>,
    level_executor: Arc<LevelExecutor>,
    risk_manager: Arc<RiskManager>,
    cancellation_manager: Arc<CancellationManager>,
    indicator_subscriber: Arc<RwLock<IndicatorSubscriber>>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    trading_state: Arc<RwLock<TradingState>>,
    redis_url: String,
    symbol: Pair,
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
        trading_state: Arc<RwLock<TradingState>>,
        redis_url: String,
        symbol: Pair,
    ) -> Self {
        Self {
            level_manager,
            level_executor,
            risk_manager,
            cancellation_manager,
            indicator_subscriber,
            trading_client,
            trading_state,
            redis_url,
            symbol,
        }
    }

    /// Main event loop
    pub async fn run(&self) -> Result<(), EngineError> {
        // Start periodic reconciliation task
        let trading_state_for_recon = Arc::clone(&self.trading_state);
        let trading_client_for_recon = Arc::clone(&self.trading_client);
        let symbol_for_recon = self.symbol.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes
            loop {
                interval.tick().await;

                let mut state = trading_state_for_recon.write().await;

                // Create a new reconciler for this task to avoid lifetime issues
                let task_reconciler =
                    PositionReconciler::new(Arc::clone(&trading_client_for_recon));
                match task_reconciler
                    .reconcile(&mut state, &symbol_for_recon, None)
                    .await
                {
                    Ok(result) => {
                        if result.reconciled {
                            tracing::debug!(
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
                            tracing::debug!(
                                "Position reconciliation skipped - WebSocket reconnecting. Will retry on next interval."
                            );
                        } else {
                            tracing::warn!("Position reconciliation failed: {e}");
                        }
                    }
                }
            }
        });

        // Start indicator subscriber in background
        let subscriber_cache = {
            let subscriber = self.indicator_subscriber.read().await;
            subscriber.cache()
        };

        // Subscribe to orderbook channel from Redis
        let redis_url: redis::ConnectionInfo = self.redis_url.parse().map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!(
                "Invalid Redis URL '{}': {}",
                self.redis_url,
                e
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
        let mut periodic_timer = interval(Duration::from_secs(1)); // Check every second

        loop {
            tokio::select! {
                // Handle orderbook updates from Redis
                message = stream.next() => {
                    if let Some(msg) = message {
                        let payload: String = msg.get_payload().map_err(|e| {
                            EngineError::CustomError(anyhow::anyhow!("Failed to get Redis message payload: {}", e))
                        })?;

                        match serde_json::from_str::<RedisMessage>(&payload) {
                            Ok(RedisMessage::OrderbookMessage(orderbook)) => {
                                if let Err(e) = self.handle_orderbook(&orderbook, &subscriber_cache).await {
                                    tracing::error!("Error handling orderbook: {}", e);
                                }
                            }
                            Ok(RedisMessage::TradeMessage(_)) => {
                                // Ignore trades for now
                            }
                            Err(e) => {
                                tracing::debug!("Failed to parse Redis message: {}", e);
                            }
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
            let state = {
                let trading_state = self.trading_state.read().await;
                trading_state.clone()
            };

            // Resolve level
            let resolved = self
                .level_executor
                .resolve_level(&level, &indicators, Some(orderbook), &balances)
                .await?;

            // Perform risk checks
            let risk_result = self
                .risk_manager
                .check_level(&resolved, &state, &balances)
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

                    // Update trading state
                    {
                        let mut state = self.trading_state.write().await;
                        state.add_order(order);
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

        let state = {
            let trading_state = self.trading_state.read().await;
            trading_state.clone()
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

                    // Update trading state
                    let mut state = self.trading_state.write().await;
                    state.update_order_status(
                        &order.order_id,
                        order.status.clone(),
                        Some(order.filled_amount),
                    );

                    // Update position
                    let pair = self.symbol.clone();
                    let qty_change = match order.side {
                        trading_core::types::common::Side::Buy => order.filled_amount,
                        trading_core::types::common::Side::Sell => -order.filled_amount,
                    };
                    state.update_position(pair, qty_change, order.price);
                }
                trading_core::types::common::OrderStatus::Canceled
                | trading_core::types::common::OrderStatus::Rejected
                | trading_core::types::common::OrderStatus::Expired => {
                    manager
                        .handle_order_cancelled(&level_id, &order.order_id)
                        .await?;

                    // Update trading state
                    let mut state = self.trading_state.write().await;
                    state.remove_order(&order.order_id);
                }
                _ => {
                    // Just update status
                    let mut state = self.trading_state.write().await;
                    state.update_order_status(
                        &order.order_id,
                        order.status.clone(),
                        Some(order.filled_amount),
                    );
                }
            }
        }

        Ok(())
    }
}
