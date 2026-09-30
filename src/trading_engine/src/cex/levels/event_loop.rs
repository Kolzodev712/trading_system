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
use trading_core::types::common::{OrderId, Pair};
use trading_core::types::exchange::{BulkAmendItem, BulkAmendRequest};
use trading_core::types::exchange::{Order, OrderRequest};
use trading_core::types::market_data::StructuredOrderbook;
use trading_core::venue::cex::channels::{CexVenueUpdate, CexVenueUpdateReceiver};
use trading_core::venue::cex::traits::TradingClient;

/// Level-based event loop
pub struct LevelBasedEventLoop {
    level_manager: Arc<RwLock<LevelManager>>,
    level_executor: Arc<LevelExecutor>,
    risk_manager: Arc<RwLock<RiskManager>>,
    cancellation_manager: Arc<CancellationManager>,
    indicator_subscriber: Arc<RwLock<IndicatorSubscriber>>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    trading_state_manager: Arc<crate::cex::state::manager::TradingStateManager>,
    market_data_rx: MarketDataUpdateReceiver,
    exchange_update_rx: CexVenueUpdateReceiver,
    pairs: Vec<Pair>,
    reconciliation_interval_secs: u64,
    /// Last seen prices for periodic risk checks (one per pair)
    last_prices: Arc<RwLock<HashMap<Pair, Decimal>>>,
    submission_journal: Option<Arc<crate::cex::state::SubmissionJournal>>,
}

impl LevelBasedEventLoop {
    /// Create a new level-based event loop
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        level_executor: Arc<LevelExecutor>,
        risk_manager: Arc<RwLock<RiskManager>>,
        cancellation_manager: Arc<CancellationManager>,
        indicator_subscriber: Arc<RwLock<IndicatorSubscriber>>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        market_data_rx: MarketDataUpdateReceiver,
        exchange_update_rx: CexVenueUpdateReceiver,
        pairs: Vec<Pair>,
        trading_state_manager: Arc<crate::cex::state::manager::TradingStateManager>,
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
            exchange_update_rx,
            pairs,
            reconciliation_interval_secs,
            last_prices: Arc::new(RwLock::new(HashMap::new())),
            submission_journal: None,
        }
    }

    pub fn with_submission_journal(
        mut self,
        journal: Arc<crate::cex::state::SubmissionJournal>,
    ) -> Self {
        self.submission_journal = Some(journal);
        self
    }

    /// Main event loop
    pub async fn run(&self) -> Result<(), EngineError> {
        tracing::debug!("[LEVELS EVENT LOOP] ========== run() method ENTERED ==========");
        self.recover_level_submissions().await?;
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

        // Consume market data and exchange updates from channels
        tracing::info!("[LEVELS EVENT LOOP] Resubscribing to market_data_rx...");
        let mut market_data_rx = self.market_data_rx.resubscribe();
        let mut exchange_update_rx = self.exchange_update_rx.resubscribe();
        tracing::info!("[LEVELS EVENT LOOP] Market data resubscribe complete");
        let mut periodic_timer = interval(Duration::from_secs(1)); // Check every second
        tracing::info!("[LEVELS EVENT LOOP] Periodic timer created");

        tracing::info!(
            "[LEVELS EVENT LOOP] Starting event loop, waiting for market data and exchange updates..."
        );
        loop {
            tokio::select! {
                // Handle exchange order updates (fills, cancels) so levels clear active_orders
                exchange_update = exchange_update_rx.recv() => {
                    match exchange_update {
                        Ok(CexVenueUpdate::Order { order, .. }) => {
                            if let Err(e) = self.handle_execution_report(&order).await {
                                tracing::error!("Error handling execution report: {}", e);
                            }
                        }
                        Ok(_) => { /* ignore Balance, Trade, Position */ }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!("Exchange update receiver lagged, skipped {} messages", skipped);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("Exchange update channel closed");
                            return Ok(());
                        }
                    }
                }

                // Handle market data updates from channel
                update = market_data_rx.recv() => {
                    tracing::debug!("[LEVELS EVENT LOOP] Received update from market_data_rx channel");
                    match update {
                        Ok(MarketDataUpdate::Orderbook { orderbook, .. }) => {
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
                        Ok(MarketDataUpdate::DexAccountHealth { .. }) => {}
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

    async fn recover_level_submissions(&self) -> Result<(), EngineError> {
        let Some(journal) = &self.submission_journal else {
            tracing::error!("submission journal is required for levels");
            self.trading_state_manager
                .get_state()
                .write()
                .await
                .halt_new_exposure("submission journal is required for levels");
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "submission journal is required for levels"
            )));
        };
        let client = self.trading_client.read().await;
        {
            let state_lock = self.trading_state_manager.get_state();
            let mut state = state_lock.write().await;
            match crate::cex::state::recover_unknown_submissions(
                journal.as_ref(),
                &*client,
                &mut state,
            )
            .await
            {
                Ok(report) => {
                    tracing::info!(
                        imported_working = report.imported_working,
                        resolved_terminal = report.resolved_terminal,
                        still_ambiguous = report.still_ambiguous,
                        "level submission recovery finished"
                    );
                }
                Err(err) => {
                    state.halt_new_exposure(format!("level submission recovery failed: {err}"));
                    return Err(err);
                }
            }
        }
        {
            let mut manager = self.level_manager.write().await;
            let state_lock = self.trading_state_manager.get_state();
            let state = state_lock.read().await;
            super::submit::attach_recovered_level_orders(journal.as_ref(), &state, &mut manager)?;
        }
        match client.fetch_open_orders().await {
            Ok(open) => {
                let mut manager = self.level_manager.write().await;
                let state_lock = self.trading_state_manager.get_state();
                let mut state = state_lock.write().await;
                super::submit::classify_open_orders(&open, &mut manager, &mut state);
            }
            Err(err) => {
                tracing::error!("REST open-order reconcile failed: {err}");
                self.trading_state_manager
                    .get_state()
                    .write()
                    .await
                    .halt_new_exposure(format!("REST open-order reconcile failed: {err}"));
                return Err(err);
            }
        }
        Ok(())
    }

    async fn handle_orderbook(
        &self,
        orderbook: &StructuredOrderbook,
        indicator_cache: &Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
    ) -> Result<(), EngineError> {
        // When trading is disabled, skip resolve/risk/post entirely (no "blocked by risk" spam)
        if self.level_executor.flatten_latched().await || self.level_executor.entries_paused().await
        {
            return Ok(());
        }
        if !self.level_executor.is_trading_enabled().await {
            return Ok(());
        }

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
                tracing::debug!(
                    "No levels ready to post for {}/{} (checking level states...)",
                    orderbook.pair.base,
                    orderbook.pair.quote
                );
                for level in all_levels.iter() {
                    if level.pair.base == orderbook.pair.base
                        && level.pair.quote == orderbook.pair.quote
                    {
                        tracing::debug!(
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
                matching_level_ids.push(level.identity.instance_id.clone());
            }
            matching_level_ids
        };

        // Get balances
        let balances = {
            let client = self.trading_client.read().await;
            client.fetch_balance().await?
        };

        // Get fresh indicators once
        let fresh_indicators: HashMap<SmolStr, IndicatorValue> = {
            let cache = indicator_cache.read().await;
            cache.clone()
        };

        // Get trading state once
        let trading_state = self.trading_state_manager.get_state();
        let mut batch_state = {
            let state_guard = trading_state.read().await;
            state_guard.clone()
        };

        // Get current price for risk checks
        let current_price = orderbook.mid_price();
        let current_prices = current_price.map(|price| {
            let mut map = HashMap::new();
            map.insert(orderbook.pair.clone(), price);
            map
        });

        // Collect levels into batches: to_post and to_amend
        let mut to_post: Vec<(SmolStr, OrderRequest)> = Vec::new(); // (expanded_id, order_request)
        let mut to_amend: Vec<(SmolStr, OrderId, OrderRequest)> = Vec::new(); // (expanded_id, order_id, new_order_request)

        // First pass: resolve, check risk, validate state - collect into batches
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
            // Resolve level using fresh indicators

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
            tracing::debug!(
                "Performing risk check for level {} ({}/{}), current_price={:?}",
                level.config.id,
                level.pair.base,
                level.pair.quote,
                current_price
            );

            let risk_result = self
                .risk_manager
                .read()
                .await
                .check_level(&resolved, &batch_state, &balances, current_prices.as_ref())
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

            // CRITICAL: Check state again right before posting/amending to prevent race condition
            // Another orderbook update might have already posted this level
            // Use expanded ID to look up the level (matching how it's stored in HashMap)
            let expanded_id = level.identity.instance_id.clone();
            let current_level = {
                let manager = self.level_manager.read().await;
                manager.get_level(&expanded_id).cloned()
            };

            let current_level = match current_level {
                Some(level) => level,
                None => {
                    tracing::warn!(
                        "Level {} ({}/{}) no longer exists in manager (expanded_id={}), skipping",
                        level.config.id,
                        level.pair.base,
                        level.pair.quote,
                        expanded_id
                    );
                    continue;
                }
            };

            // Check if level has active orders - if so, collect for amendment batch
            if !current_level.active_orders.is_empty()
                && matches!(current_level.state, super::LevelState::Posted)
            {
                // Get the first active order (levels typically have one order)
                if let Some(order_id) = current_level.active_orders.first() {
                    // Get order from trading state (orders we placed)
                    let existing_order_opt = {
                        let state = trading_state.read().await;
                        state.active_orders.get(order_id).cloned()
                    };

                    if let Some(existing_order) = existing_order_opt {
                        // Check if order should be amended
                        use rust_decimal::Decimal;
                        let price_changed = (existing_order.price - resolved.resolved_price).abs()
                            > Decimal::new(1, 10);
                        let quantity_changed = (existing_order.amount - resolved.resolved_quantity)
                            .abs()
                            > Decimal::new(1, 10);

                        if price_changed || quantity_changed {
                            // Side must not have changed for amendment
                            if existing_order.side == resolved.level.config.side {
                                // Create order request for amendment
                                let order_request = OrderRequest {
                                    instrument: trading_core::types::common::Instrument::new(
                                        self.level_executor.instrument_type().clone(),
                                        current_level.pair.clone(),
                                    ),
                                    side: resolved.level.config.side.clone(),
                                    order_type: resolved.resolved_order_type,
                                    price: resolved.resolved_price,
                                    amount: resolved.resolved_quantity,
                                    timestamp: Some(trading_core::utils::current_timestamp()),
                                    exchange_params: std::collections::HashMap::new(),
                                };

                                to_amend.push((expanded_id, order_id.clone(), order_request));
                                continue; // Skip posting new order
                            }
                        } else {
                            // No amendment needed - order is already correct
                            tracing::debug!(
                                "Level {} order {} doesn't need amendment, skipping",
                                current_level.config.id,
                                order_id.as_str()
                            );
                            continue;
                        }
                    }
                }
            }

            // Check if level is ready to post (no active orders)
            if !current_level.is_ready_to_post() {
                tracing::info!(
                    "Level {} ({}/{}) is not ready to post (state={:?}, active_orders={}), skipping",
                    current_level.config.id,
                    current_level.pair.base,
                    current_level.pair.quote,
                    current_level.state,
                    current_level.active_orders.len()
                );
                continue;
            }

            // Create order request for posting
            let order_request = OrderRequest {
                instrument: trading_core::types::common::Instrument::new(
                    current_level.identity.instrument.clone(),
                    current_level.pair.clone(),
                ),
                side: resolved.level.config.side.clone(),
                order_type: resolved.resolved_order_type,
                price: resolved.resolved_price,
                amount: resolved.resolved_quantity,
                timestamp: Some(trading_core::utils::current_timestamp()),
                exchange_params: std::collections::HashMap::new(),
            };
            batch_state.begin_submission(crate::cex::state::PendingSubmission {
                client_order_id: SmolStr::new(format!("batch-reserve-{expanded_id}")),
                symbol: current_level.pair.clone(),
                side: order_request.side.clone(),
                amount: order_request.amount,
                price: order_request.price,
                strategy_id: None,
                level_id: Some(expanded_id.to_string()),
            });

            to_post.push((expanded_id, order_request));
        }

        // Check trading control before batching
        if self.level_executor.flatten_latched().await
            || self.level_executor.entries_paused().await
            || !self.level_executor.is_trading_enabled().await
        {
            // Only log periodically to avoid spam
            static SKIP_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let skip_count = SKIP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if skip_count.is_multiple_of(100) {
                if self.level_executor.flatten_latched().await
                    && matches!(
                        self.level_executor.instrument_type(),
                        trading_core::types::common::InstrumentType::Spot
                    )
                {
                    tracing::warn!(
                        "Spot flatten is unavailable; Pause/Cancel owned orders only (no pair-wide sale)"
                    );
                }
                tracing::debug!(
                    "Trading is disabled - skipping batch operations (skipped {skip_count} so far)"
                );
            }
            return Ok(());
        }

        // Individual persist-before-send (no bulk zip). Recheck pause/generation before each send.
        if !to_post.is_empty() {
            let Some(journal) = &self.submission_journal else {
                self.trading_state_manager
                    .get_state()
                    .write()
                    .await
                    .halt_new_exposure("submission journal is required for levels");
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "submission journal is required for levels"
                )));
            };
            tracing::info!(
                "Submitting {} level(s) individually for {}/{}",
                to_post.len(),
                orderbook.pair.base,
                orderbook.pair.quote
            );
            for (expanded_id, order_request) in to_post {
                let (level_snapshot, live_state) = {
                    let manager = self.level_manager.read().await;
                    let state_lock = self.trading_state_manager.get_state();
                    let state = state_lock.read().await;
                    (manager.get_level(&expanded_id).cloned(), state.clone())
                };
                let Some(level) = level_snapshot else {
                    continue;
                };
                let resolved = super::ResolvedLevel {
                    level,
                    resolved_price: order_request.price,
                    resolved_quantity: order_request.amount,
                    resolved_order_type: order_request.order_type.clone(),
                };
                let risk_result = self
                    .risk_manager
                    .read()
                    .await
                    .check_level(&resolved, &live_state, &balances, current_prices.as_ref())
                    .await;
                if !risk_result.allowed {
                    tracing::info!(
                        "Level {} blocked by prepare-time risk check: {}",
                        expanded_id,
                        risk_result
                            .reason
                            .as_ref()
                            .map(|r| r.as_str())
                            .unwrap_or("unknown")
                    );
                    continue;
                }
                let prepared = {
                    let mut manager = self.level_manager.write().await;
                    let state_lock = self.trading_state_manager.get_state();
                    let mut state = state_lock.write().await;
                    match super::submit::prepare_level_submission(
                        &mut manager,
                        journal.as_ref(),
                        &mut state,
                        &expanded_id,
                        order_request,
                    ) {
                        Ok(prepared) => prepared,
                        Err(err) => {
                            tracing::warn!(
                                "Level {} persist-before-send failed: {}",
                                expanded_id,
                                err
                            );
                            continue;
                        }
                    }
                };
                if self.level_executor.flatten_latched().await
                    || self.level_executor.entries_paused().await
                    || !self.level_executor.is_trading_enabled().await
                    || !{
                        let manager = self.level_manager.read().await;
                        manager.submit_still_allowed(&expanded_id, prepared.generation)
                    }
                {
                    let mut manager = self.level_manager.write().await;
                    let state_lock = self.trading_state_manager.get_state();
                    let mut state = state_lock.write().await;
                    if let Err(err) = super::submit::abort_unsent_submission(
                        &mut manager,
                        journal.as_ref(),
                        &mut state,
                        &prepared,
                        &expanded_id,
                    ) {
                        tracing::error!("Failed to abort unsent level {}: {}", expanded_id, err);
                        state.halt_new_exposure(format!("level abort journal write failed: {err}"));
                        return Err(err);
                    }
                    continue;
                }
                let send_result = {
                    let client = self.trading_client.write().await;
                    super::submit::send_prepared_level_order(&*client, &prepared).await
                };
                let apply = {
                    let mut manager = self.level_manager.write().await;
                    let state_lock = self.trading_state_manager.get_state();
                    let mut state = state_lock.write().await;
                    super::submit::apply_level_submit_result(
                        &mut manager,
                        journal.as_ref(),
                        &mut state,
                        &prepared,
                        &expanded_id,
                        send_result,
                    )
                };
                match apply {
                    Ok(super::submit::LevelSubmitResult::Posted(order)) => {
                        tracing::info!(
                            "[SUCCESS] Level posted order: {} for {}/{}",
                            order.order_id.as_str(),
                            orderbook.pair.base,
                            orderbook.pair.quote
                        );
                        if let Some(price) = current_price {
                            let mut current_prices = HashMap::new();
                            current_prices.insert(orderbook.pair.clone(), price);
                            let trading_state = self.trading_state_manager.get_state();
                            let state_after_trade = {
                                let state_guard = trading_state.read().await;
                                state_guard.clone()
                            };
                            let post_trade_result = self
                                .risk_manager
                                .read()
                                .await
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
                            }
                        }
                    }
                    Ok(super::submit::LevelSubmitResult::MustCancel(order)) => {
                        tracing::warn!(
                            "Level {} was disabled during send; cancelling owned order {}",
                            expanded_id,
                            order.order_id.as_str()
                        );
                        let client = self.trading_client.write().await;
                        if let Err(err) = super::cancel_owned_order_ids(
                            &*client,
                            std::slice::from_ref(&order.order_id),
                        )
                        .await
                        {
                            drop(client);
                            self.trading_state_manager
                                .get_state()
                                .write()
                                .await
                                .halt_new_exposure(format!(
                                    "disable/send race cancel unverified: {err}"
                                ));
                            return Err(err);
                        }
                    }
                    Ok(super::submit::LevelSubmitResult::Unknown) => {
                        tracing::warn!(
                            "Level {} submit outcome unknown; reservation retained",
                            expanded_id
                        );
                        break;
                    }
                    Ok(other) => {
                        tracing::warn!("Level {} submit result: {:?}", expanded_id, other);
                    }
                    Err(err) => {
                        tracing::error!("Level {} submit apply failed: {}", expanded_id, err);
                    }
                }
            }
        }

        // Batch process amendments
        if !to_amend.is_empty() {
            tracing::info!(
                "Batch amending {} level(s) for {}/{}",
                to_amend.len(),
                orderbook.pair.base,
                orderbook.pair.quote
            );

            let amendments: Vec<BulkAmendItem> = to_amend
                .iter()
                .map(|(_, order_id, req)| BulkAmendItem {
                    order_id: order_id.clone(),
                    new_request: req.clone(),
                })
                .collect();
            let bulk_request = BulkAmendRequest { amendments };
            let trading_client = self.trading_client.write().await;
            let bulk_response = trading_client.bulk_amend_orders(bulk_request).await;

            // Process results by request index (never zip truncated batches).
            match bulk_response {
                Ok(response) => {
                    let results = response.results;
                    for (idx, (expanded_id, original_order_id, _)) in to_amend.iter().enumerate() {
                        if idx >= results.len() {
                            tracing::warn!(
                                "Bulk amend response missing result for index {} (level {})",
                                idx,
                                expanded_id
                            );
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
                                tracing::info!(
                                    "[SUCCESS] Level amended order: {} -> {} for {}/{}",
                                    original_order_id.as_str(),
                                    amended_order.order_id.as_str(),
                                    orderbook.pair.base,
                                    orderbook.pair.quote
                                );

                                // Update trading state with amended order
                                self.trading_state_manager
                                    .add_order(amended_order.clone())
                                    .await;

                                // Update level state (order ID might have changed if amend used cancel+place fallback)
                                {
                                    let mut manager = self.level_manager.write().await;
                                    if let Some(level_mut) = manager.get_level_mut(expanded_id) {
                                        // If order ID changed, update active_orders
                                        if amended_order.order_id != *original_order_id {
                                            level_mut.active_orders.clear();
                                            level_mut
                                                .active_orders
                                                .push(amended_order.order_id.clone());
                                        }
                                        level_mut.last_posted_at =
                                            Some(trading_core::utils::current_timestamp());
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to amend level in batch: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Bulk amend orders failed: {}", e);
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
                .read()
                .await
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
                        if manager
                            .should_repost(&level.identity.instance_id)
                            .unwrap_or(false)
                        {
                            Some(level.identity.instance_id.clone())
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
        // Find level that owns this order. LevelManager keys are expanded (e.g. buy_level_1_BTCUSDT).
        let level_id = {
            let manager = self.level_manager.read().await;
            manager
                .get_all_levels()
                .iter()
                .find(|level| {
                    level.active_orders.contains(&order.order_id)
                        || order.client_order_id.as_ref().is_some_and(|cid| {
                            level.owned_client_ids.iter().any(|owned| owned == cid)
                        })
                })
                .map(|level| level.identity.instance_id.clone())
        };

        if let Some(level_id) = level_id {
            let mut manager = self.level_manager.write().await;

            match order.status {
                trading_core::types::common::OrderStatus::Filled => {
                    manager
                        .handle_order_filled(&level_id, &order.order_id, order.filled_amount)
                        .await?;

                    // TradingStateManager automatically handles order updates via ExchangeStateManager
                    // No manual update needed
                }
                trading_core::types::common::OrderStatus::Canceled
                | trading_core::types::common::OrderStatus::Rejected
                | trading_core::types::common::OrderStatus::Expired => {
                    manager
                        .handle_order_cancelled(&level_id, &order.order_id, order.filled_amount)
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
