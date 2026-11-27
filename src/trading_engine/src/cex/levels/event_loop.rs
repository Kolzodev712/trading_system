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
use trading_core::diagnostics::{HoldAggregator, ReasonCode};
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
    /// Snapshot of the MD cache so enable can seed without waiting for a broadcast.
    cached_books: Option<Arc<dyn Fn() -> Vec<StructuredOrderbook> + Send + Sync>>,
    max_market_data_age_ms: Option<u64>,
    hold_agg: parking_lot::Mutex<HoldAggregator>,
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
            cached_books: None,
            max_market_data_age_ms: Some(120_000),
            hold_agg: parking_lot::Mutex::new(HoldAggregator::default()),
        }
    }

    pub fn with_submission_journal(
        mut self,
        journal: Arc<crate::cex::state::SubmissionJournal>,
    ) -> Self {
        self.submission_journal = Some(journal);
        self
    }

    pub fn with_cached_books(
        mut self,
        source: Arc<dyn Fn() -> Vec<StructuredOrderbook> + Send + Sync>,
    ) -> Self {
        self.cached_books = Some(source);
        self
    }

    pub fn with_max_market_data_age_ms(mut self, max_age_ms: Option<u64>) -> Self {
        self.max_market_data_age_ms = max_age_ms;
        self
    }

    async fn log_control_snapshot(&self, at: &str) {
        let trading = self.level_executor.is_trading_enabled().await;
        let paused = self.level_executor.entries_paused().await;
        let flatten = self.level_executor.flatten_latched().await;
        let manager = self.level_manager.read().await;
        let ready = manager
            .get_all_levels()
            .iter()
            .filter(|level| level.is_ready_to_post())
            .count();
        tracing::info!(
            at,
            trading,
            paused,
            flatten,
            ready_levels = ready,
            total_levels = manager.get_all_levels().len(),
            "levels loop control snapshot"
        );
    }

    async fn seed_from_cached_books(
        &self,
        indicator_cache: &Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
    ) -> Result<(), EngineError> {
        let Some(source) = &self.cached_books else {
            tracing::info!("levels enable: no cached-book source configured");
            return Ok(());
        };
        let books = source();
        tracing::info!(
            count = books.len(),
            "levels enable: seeding from cached books"
        );
        for book in books {
            self.handle_orderbook(&book, indicator_cache).await?;
        }
        Ok(())
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
        let mut trading_was_enabled = self.level_executor.is_trading_enabled().await;
        if trading_was_enabled {
            self.log_control_snapshot("loop_start").await;
            self.seed_from_cached_books(&subscriber_cache).await?;
        }
        let mut heartbeat: u64 = 0;
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
                            tracing::warn!("Exchange update receiver lagged, skipped {} messages; requesting reconcile", skipped);
                            // Missed updates may include fills; the periodic tick below reconciles.
                            self.trading_state_manager.request_reconcile();
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
                            tracing::info!(
                                pair = %format!("{}/{}", orderbook.pair.base, orderbook.pair.quote),
                                ts_ms = orderbook.timestamp,
                                mid = ?orderbook.mid_price(),
                                "levels loop received book"
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
                    let enabled = self.level_executor.is_trading_enabled().await;
                    heartbeat = heartbeat.saturating_add(1);
                    if heartbeat.is_multiple_of(10) {
                        self.log_control_snapshot("heartbeat").await;
                    }
                    if enabled && !trading_was_enabled {
                        self.log_control_snapshot("enable").await;
                        if let Err(e) = self.seed_from_cached_books(&subscriber_cache).await {
                            tracing::error!("Error seeding levels from cached books: {}", e);
                        }
                    }
                    trading_was_enabled = enabled;
                    if self.trading_state_manager.take_reconcile_request() {
                        self.reconcile_all_pairs().await;
                    }
                    if let Err(e) = self.handle_periodic_tasks(&subscriber_cache).await {
                        tracing::error!("Error in periodic tasks: {}", e);
                    }
                }
            }
        }
    }

    /// Out-of-band reconcile after a lagged broadcast (fills may have been dropped).
    async fn reconcile_all_pairs(&self) {
        for pair in &self.pairs {
            if let Err(e) = self.trading_state_manager.reconcile(pair, None).await {
                tracing::error!("Lag-triggered reconciliation error for {}: {}", pair, e);
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
            if let Err(err) =
                super::submit::attach_recovered_level_orders(journal.as_ref(), &state, &mut manager)
            {
                drop(state);
                drop(manager);
                self.trading_state_manager
                    .get_state()
                    .write()
                    .await
                    .halt_new_exposure(format!("ambiguous level recovery: {err}"));
                return Err(err);
            }
        }
        match client.fetch_open_orders().await {
            Ok(open) => {
                let mut manager = self.level_manager.write().await;
                let state_lock = self.trading_state_manager.get_state();
                let mut state = state_lock.write().await;
                super::submit::classify_open_orders(&open, &mut manager, &mut state)?;
                let rest_working = open
                    .iter()
                    .filter(|order| order.is_working())
                    .map(|order| order.order_id.clone())
                    .collect();
                manager.retire_absent_owned_orders(&rest_working)?;
                manager.complete_ready_deletes()?;
            }
            Err(err) => {
                tracing::error!("REST open-order reconcile failed: {err}");
                self.trading_state_manager
                    .get_state()
                    .write()
                    .await
                    .halt_new_exposure_with(
                        trading_core::diagnostics::ReasonCode::RestLookupFailed,
                        format!("REST open-order reconcile failed: {err}"),
                    );
            }
        }
        Ok(())
    }

    async fn handle_orderbook(
        &self,
        orderbook: &StructuredOrderbook,
        indicator_cache: &Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
    ) -> Result<(), EngineError> {
        let flatten = self.level_executor.flatten_latched().await;
        let paused = self.level_executor.entries_paused().await;
        let trading = self.level_executor.is_trading_enabled().await;
        let pair_key = format!("{}/{}", orderbook.pair.base, orderbook.pair.quote);
        if flatten {
            self.hold_agg
                .lock()
                .note(ReasonCode::HoldFlattenLatched, &pair_key);
            return Ok(());
        }
        if paused {
            self.hold_agg
                .lock()
                .note(ReasonCode::HoldEntriesPaused, &pair_key);
            return Ok(());
        }
        if !trading {
            self.hold_agg
                .lock()
                .note(ReasonCode::HoldTradingDisabled, &pair_key);
            return Ok(());
        }
        if let Some(max_age_ms) = self.max_market_data_age_ms {
            if orderbook_is_stale(orderbook.timestamp, max_age_ms) {
                self.hold_agg
                    .lock()
                    .note(ReasonCode::HoldStaleBook, &pair_key);
                return Ok(());
            }
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
                    level.pair.base == orderbook.pair.base
                        && level.pair.quote == orderbook.pair.quote
                })
                .filter(|level| level.needs_book_tick())
                .collect();

            if !ready_levels.is_empty() {
                tracing::debug!(
                    "Found {} level(s) for post/amend on {}/{}: {:?}",
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
                    pair = %format!("{}/{}", orderbook.pair.base, orderbook.pair.quote),
                    total = all_levels.len(),
                    "levels skip book: no post/amend candidates"
                );
                for level in all_levels.iter() {
                    if level.pair.base == orderbook.pair.base
                        && level.pair.quote == orderbook.pair.quote
                    {
                        tracing::info!(
                            level = %level.config.id,
                            state = ?level.state,
                            enabled = level.config.enabled,
                            active_orders = level.active_orders.len(),
                            "levels skip book: level not a post/amend candidate"
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

        // Nothing to post/amend for this book: skip balance lookup and state cloning.
        if level_ids.is_empty() {
            return Ok(());
        }

        // Balances come from the ExchangeStateManager cache (REST bootstrap + WS updates);
        // only fall back to REST when the cache has not been populated yet.
        let balances = {
            let cached = self.trading_state_manager.cached_balances().await;
            if cached.is_empty() {
                let client = self.trading_client.read().await;
                client.fetch_balance().await?
            } else {
                cached
            }
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
                    tracing::info!(
                        level = %level.config.id,
                        pair = %format!("{}/{}", level.pair.base, level.pair.quote),
                        reason = %e,
                        "levels skip book: resolve failed"
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
                trading_core::diagnostics::emit_decision(
                    "hold",
                    ReasonCode::RiskRejected,
                    risk_result
                        .reason
                        .as_ref()
                        .map(|r| r.as_str())
                        .unwrap_or("unknown"),
                );
                tracing::info!(
                    event = "decision",
                    action = "hold",
                    reason_code = %ReasonCode::RiskRejected,
                    level_id = %level.config.id,
                    "Level blocked by risk check"
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
                                let mut exchange_params = std::collections::HashMap::new();
                                if let Some(cid) = existing_order.client_order_id.as_ref() {
                                    exchange_params
                                        .insert(smol_str::SmolStr::new("orderLinkId"), cid.clone());
                                }
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
                                    exchange_params,
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
                        drop(client);
                        let mut manager = self.level_manager.write().await;
                        manager.retire_verified_order_ids(
                            &expanded_id,
                            std::slice::from_ref(&order.order_id),
                        )?;
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
                        self.trading_state_manager
                            .get_state()
                            .write()
                            .await
                            .halt_new_exposure(format!("level submit apply failed: {err}"));
                        return Err(err);
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

            match bulk_response {
                Ok(response) => {
                    for (expanded_id, original_order_id, req) in &to_amend {
                        let expected_client = req
                            .exchange_params
                            .get("orderLinkId")
                            .map(|cid| cid.as_str());
                        let Some(result) = super::submit::find_amend_result(
                            original_order_id,
                            expected_client,
                            &response.results,
                        ) else {
                            if response
                                .failed_amendments
                                .iter()
                                .any(|(id, _)| id == original_order_id)
                            {
                                tracing::warn!(
                                    "Amend failed for level {} order {}",
                                    expanded_id,
                                    original_order_id.as_str()
                                );
                            } else {
                                tracing::warn!(
                                    "Bulk amend result not correlated for level {} order {}",
                                    expanded_id,
                                    original_order_id.as_str()
                                );
                            }
                            continue;
                        };
                        let Some(amended_order) = result.data.clone() else {
                            tracing::warn!(
                                "Amend result for {} has no order payload",
                                original_order_id.as_str()
                            );
                            continue;
                        };
                        tracing::info!(
                            "[SUCCESS] Level amended order: {} -> {} for {}/{}",
                            original_order_id.as_str(),
                            amended_order.order_id.as_str(),
                            orderbook.pair.base,
                            orderbook.pair.quote
                        );

                        self.trading_state_manager
                            .add_order(amended_order.clone())
                            .await;

                        let mut manager = self.level_manager.write().await;
                        if let Err(err) = manager.replace_owned_exchange_id(
                            expanded_id,
                            original_order_id,
                            &amended_order.order_id,
                        ) {
                            drop(manager);
                            self.trading_state_manager
                                .get_state()
                                .write()
                                .await
                                .halt_new_exposure(format!(
                                    "amend ownership persist failed: {err}"
                                ));
                            return Err(err);
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

        if levels_to_repost.is_empty() {
            return Ok(());
        }

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
        manager.persist_runtime_state()?;

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
                trading_core::types::common::OrderStatus::PartiallyFilled => {
                    manager
                        .handle_order_partially_filled(
                            &level_id,
                            &order.order_id,
                            order.filled_amount,
                        )
                        .await?;
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

/// `ts` is Unix ms when `> 10^10`, otherwise Unix seconds (legacy Paper adapter).
pub fn orderbook_is_stale(ts: u64, max_age_ms: u64) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let ts_ms = if ts > 10_000_000_000 {
        ts
    } else {
        ts.saturating_mul(1000)
    };
    now_ms.saturating_sub(ts_ms) > max_age_ms
}

#[cfg(test)]
mod stale_tests {
    use super::orderbook_is_stale;

    #[test]
    fn fresh_millisecond_book_is_not_stale() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(!orderbook_is_stale(now_ms, 120_000));
    }

    #[test]
    fn aged_millisecond_book_is_stale() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(orderbook_is_stale(now_ms.saturating_sub(180_000), 120_000));
    }
}
