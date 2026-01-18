//! Exchange state manager
//!
//! This module provides `ExchangeStateManager` which manages raw exchange data
//! (balances, orders, trades from WebSocket), performs WS vs REST reconciliation,
//! and publishes updates via broadcast channels.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use trading_core::{
    channels::exchange::ExchangeUpdateSender,
    error::EngineError,
    registry::symbol::SymbolRegistry,
    traits::TradingClient,
    types::{
        common::{OrderId, Pair},
        exchange::{Balance, Order, OrderRequest, Position, Trade},
    },
};

use super::{publisher::ExchangeStatePublisher, reconciler::StateReconciler, state::ExchangeState};

/// Manages raw exchange data (balances, orders, trades from WebSocket)
///
/// This is the **single point of contact with exchange** for account data.
/// Performs WS vs REST reconciliation and publishes all updates via broadcast channels.
#[derive(Debug)]
pub struct ExchangeStateManager {
    /// In-memory state (raw exchange data)
    state: Arc<RwLock<ExchangeState>>,

    /// Publisher for state updates via broadcast channels
    publisher: ExchangeStatePublisher,

    /// Reconciler for WS vs REST reconciliation
    reconciler: StateReconciler,

    /// Trading client for REST reconciliation calls
    /// Stored here for potential future use (e.g., exposing to external callers)
    /// Currently only used by the reconciler, which receives it during construction
    #[allow(dead_code)]
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
}

impl ExchangeStateManager {
    /// Create a new ExchangeStateManager
    ///
    /// # Arguments
    /// * `exchange_update_tx` - Broadcast channel sender for publishing updates
    /// * `symbol_registry` - Symbol registry for Symbol↔Pair conversions
    /// * `trading_client` - Trading client for REST reconciliation
    /// * `reconciliation_interval_secs` - Interval for periodic reconciliation (0 = disabled)
    /// * `reconciliation_enabled` - Whether reconciliation is enabled
    pub fn new(
        exchange_update_tx: ExchangeUpdateSender,
        symbol_registry: Arc<SymbolRegistry>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        reconciliation_interval_secs: u64,
        reconciliation_enabled: bool,
    ) -> Self {
        let state = Arc::new(RwLock::new(ExchangeState::with_registry(symbol_registry)));
        let publisher = ExchangeStatePublisher::new(exchange_update_tx);
        let reconciler = StateReconciler::new(
            trading_client.clone(),
            reconciliation_interval_secs,
            reconciliation_enabled,
        );

        Self {
            state,
            publisher,
            reconciler,
            trading_client,
        }
    }

    /// Update order from WebSocket and publish
    ///
    /// # Arguments
    /// * `order` - Order update from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails
    pub async fn update_order(&self, order: Order) -> Result<(), EngineError> {
        // Clone before lock to minimize lock time
        let order_for_state = order.clone();
        let order_id = order.order_id.clone();

        // 1. Update in-memory state
        {
            let mut state = self.state.write().await;
            state.update_order(order_for_state);
        }

        // 2. Publish via broadcast channel (use original order)
        if let Err(e) = self.publisher.publish_order(&order).await {
            error!("Failed to publish order update: {}", e);
            return Err(e);
        }

        debug!(
            "Updated and published order: order_id={}",
            order_id.as_str()
        );
        Ok(())
    }

    /// Update balance from WebSocket and publish
    ///
    /// # Arguments
    /// * `balance` - Balance update from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails
    pub async fn update_balance(&self, balance: Balance) -> Result<(), EngineError> {
        // Clone before lock to minimize lock time
        let balance_for_state = balance.clone();
        let symbol = balance.symbol.clone();

        // 1. Update in-memory state
        {
            let mut state = self.state.write().await;
            state.update_balance(balance_for_state);
        }

        // 2. Publish via broadcast channel (use original balance)
        if let Err(e) = self.publisher.publish_balance(&balance).await {
            error!("Failed to publish balance update: {}", e);
            return Err(e);
        }

        debug!("Updated and published balance: symbol={}", symbol);
        Ok(())
    }

    /// Update multiple balances from WebSocket and publish
    ///
    /// Used for OutboundAccountPosition which contains multiple balances.
    ///
    /// # Arguments
    /// * `balances` - Vector of balance updates from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails
    pub async fn update_balances(&self, balances: Vec<Balance>) -> Result<(), EngineError> {
        // Clone before lock to minimize lock time
        let balances_for_state = balances.clone();
        let balance_count = balances.len();

        // 1. Update in-memory state
        {
            let mut state = self.state.write().await;
            state.update_balances(balances_for_state);
        }

        // 2. Publish each balance via broadcast channel (use original balances)
        for balance in &balances {
            if let Err(e) = self.publisher.publish_balance(balance).await {
                warn!(
                    "Failed to publish balance update for {}: {}",
                    balance.symbol, e
                );
                // Continue publishing other balances even if one fails
            }
        }

        debug!("Updated and published {} balances", balance_count);
        Ok(())
    }

    /// Add trade from WebSocket and publish
    ///
    /// # Arguments
    /// * `trade` - Trade update from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails
    pub async fn add_trade(&self, trade: Trade) -> Result<(), EngineError> {
        // Clone before lock to minimize lock time
        let trade_for_state = trade.clone();
        let trade_id = trade.trade_id.clone();

        // 1. Update in-memory state
        {
            let mut state = self.state.write().await;
            state.add_trade(trade_for_state);
        }

        // 2. Publish via broadcast channel (use original trade)
        if let Err(e) = self.publisher.publish_trade(&trade).await {
            error!("Failed to publish trade update: {}", e);
            return Err(e);
        }

        debug!("Added and published trade: trade_id={}", trade_id);
        Ok(())
    }

    /// Update position from WebSocket and publish
    ///
    /// # Arguments
    /// * `position` - Position update from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails
    pub async fn update_position(&self, position: Position) -> Result<(), EngineError> {
        // Clone before lock to minimize lock time
        let position_for_state = position.clone();
        let pair = position.instrument.pair.clone();

        // 1. Update in-memory state
        {
            let mut state = self.state.write().await;
            state.update_position(position_for_state);
        }

        // 2. Publish via broadcast channel (use original position)
        if let Err(e) = self.publisher.publish_position(&position).await {
            error!("Failed to publish position update: {}", e);
            return Err(e);
        }

        debug!("Updated and published position: pair={}", pair);
        Ok(())
    }

    /// Get current state (for reconciliation and external access)
    pub fn get_state(&self) -> Arc<RwLock<ExchangeState>> {
        self.state.clone()
    }

    /// Get balance for a currency
    ///
    /// # Arguments
    /// * `currency` - Currency symbol (e.g., "BTC")
    pub async fn get_balance(&self, currency: &str) -> Option<Balance> {
        let state = self.state.read().await;
        state.get_balance(currency).cloned()
    }

    /// Get orders for a pair
    ///
    /// # Arguments
    /// * `pair` - Trading pair
    pub async fn get_orders_for_pair(&self, pair: &Pair) -> Vec<Order> {
        let state = self.state.read().await;
        state
            .get_orders_for_pair(pair)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get open orders for a specific pair (only New and PartiallyFilled orders)
    pub async fn get_open_orders_for_pair(&self, pair: &Pair) -> Vec<Order> {
        let state = self.state.read().await;
        state
            .get_open_orders_for_pair(pair)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get an order by ID
    ///
    /// # Arguments
    /// * `order_id` - Order ID
    ///
    /// # Returns
    /// Cloned order if found, None otherwise
    pub async fn get_order_by_id(&self, order_id: &OrderId) -> Option<Order> {
        let state = self.state.read().await;
        state.get_order_by_id(order_id).cloned()
    }

    /// Perform initial bootstrap reconciliation
    ///
    /// Fetches balances, orders, and trades from REST API to initialize state.
    ///
    /// # Arguments
    /// * `configured_pairs` - Pairs to fetch trades for
    ///
    /// # Errors
    /// Returns an error if REST calls fail
    pub async fn bootstrap_reconcile(
        &self,
        configured_pairs: &[Pair],
    ) -> Result<super::reconciler::ReconciliationResult, EngineError> {
        // Fetch balances from REST and update ExchangeStateManager
        let balances = self.trading_client.read().await.fetch_balance().await?;

        {
            let mut state = self.state.write().await;
            state.update_balances(balances.clone());
        }
        info!(
            "Bootstrap: Updated ExchangeStateManager with {} balances",
            balances.len()
        );

        // Fetch trades for each configured pair
        let mut total_trades = 0;
        let trading_client = self.trading_client.clone();
        for pair in configured_pairs {
            match trading_client.read().await.fetch_trades(Some(pair)).await {
                Ok(trades) => {
                    if !trades.is_empty() {
                        let mut state = self.state.write().await;
                        state.update_trades(pair, trades.clone());
                        drop(state);
                        total_trades += trades.len();
                        info!("Bootstrap: Fetched {} trades for {}", trades.len(), pair);
                    }
                }
                Err(e) => {
                    warn!("Bootstrap: Failed to fetch trades for {}: {}", pair, e);
                }
            }
        }

        if total_trades > 0 {
            info!(
                "Bootstrap: Updated ExchangeStateManager with {} total trades",
                total_trades
            );
        }

        // Call reconciler for the result (it will log the counts)
        self.reconciler
            .bootstrap_reconcile(configured_pairs, self.state.clone(), total_trades)
            .await
    }

    /// Start periodic WS vs REST reconciliation
    ///
    /// This spawns a background task that periodically reconciles WebSocket state
    /// with REST API state (REST is authoritative).
    ///
    /// # Arguments
    /// * `configured_pairs` - Pairs to reconcile trades for
    ///
    /// # Returns
    /// JoinHandle for the reconciliation task
    pub fn start_reconciliation(&self, configured_pairs: Vec<Pair>) -> tokio::task::JoinHandle<()> {
        self.reconciler
            .start_periodic_reconciliation(configured_pairs, self.state.clone())
    }
}

// Note: Unit tests that require mock clients have been moved to integration tests
// to avoid coupling. See tests/integration/exchange.rs for those tests.
