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
        exchange::{Balance, Order, OrderRequest, Trade},
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

        // Call reconciler for the result (it will log the counts)
        self.reconciler
            .bootstrap_reconcile(configured_pairs, self.state.clone())
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use trading_core::{
        channels::exchange::exchange_update_channel,
        registry::symbol::SymbolRegistry,
        traits::TradingClient,
        types::{
            common::{OrderId, OrderStatus, Side, Symbol},
            exchange::{Balance, Order, OrderRequest, Trade},
        },
    };

    // Mock TradingClient for tests
    #[derive(Debug, Clone)]
    struct MockTradingClient {
        balances: Vec<Balance>,
        open_orders: Vec<Order>,
        trades: Vec<Trade>,
    }

    impl MockTradingClient {
        fn new() -> Self {
            Self {
                balances: vec![],
                open_orders: vec![],
                trades: vec![],
            }
        }

        /// Configure mock with specific balances for testing
        #[allow(dead_code)]
        fn with_balances(mut self, balances: Vec<Balance>) -> Self {
            self.balances = balances;
            self
        }

        /// Configure mock with specific open orders for testing
        #[allow(dead_code)]
        fn with_open_orders(mut self, orders: Vec<Order>) -> Self {
            self.open_orders = orders;
            self
        }

        /// Configure mock with specific trades for testing
        #[allow(dead_code)]
        fn with_trades(mut self, trades: Vec<Trade>) -> Self {
            self.trades = trades;
            self
        }
    }

    #[async_trait::async_trait]
    impl TradingClient<OrderRequest> for MockTradingClient {
        fn exchange_id(&self) -> trading_core::types::common::Exchange {
            trading_core::types::common::Exchange::Binance
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            // Mock: always succeeds for testing
            Ok(())
        }

        async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
            // Mock: create a simple order from the request
            use trading_core::types::common::OrderStatus;
            Ok(Order {
                order_id: OrderId::new(format!("mock_order_{}", req.symbol).into()),
                client_order_id: None,
                symbol: req.symbol,
                side: req.side,
                price: req.price,
                amount: req.amount,
                internal_created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                exchange_created_at: None,
                status: OrderStatus::New,
                filled_amount: Decimal::ZERO,
            })
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            // Mock: always succeeds
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            // Mock: always succeeds
            Ok(())
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            // Mock: return configured balances
            Ok(self.balances.clone())
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            // Mock: return configured open orders
            Ok(self.open_orders.clone())
        }

        async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
            // Mock: return configured trades, optionally filtered by pair
            let trades = if let Some(pair) = pair {
                self.trades
                    .iter()
                    .filter(|t| &t.pair == pair)
                    .cloned()
                    .collect()
            } else {
                self.trades.clone()
            };
            Ok(trades)
        }

        async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
            // Mock: always succeeds
            Ok(())
        }

        async fn amend_order(
            &self,
            _order_id: OrderId,
            new_request: OrderRequest,
        ) -> Result<Order, EngineError> {
            // Mock: cancel old order and create new one
            use trading_core::types::common::OrderStatus;
            Ok(Order {
                order_id: OrderId::new(format!("amended_{}", new_request.symbol).into()),
                client_order_id: None,
                symbol: new_request.symbol,
                side: new_request.side,
                price: new_request.price,
                amount: new_request.amount,
                internal_created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                exchange_created_at: None,
                status: OrderStatus::New,
                filled_amount: Decimal::ZERO,
            })
        }
    }

    fn create_test_manager() -> (
        ExchangeStateManager,
        trading_core::channels::exchange::ExchangeUpdateReceiver,
    ) {
        let (tx, rx) = exchange_update_channel(100);
        let registry = Arc::new(SymbolRegistry::new());
        let trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(MockTradingClient::new()));

        let manager = ExchangeStateManager::new(
            tx,
            registry,
            trading_client,
            0, // Disabled reconciliation for tests
            false,
        );
        (manager, rx)
    }

    fn create_test_order() -> Order {
        Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            symbol: Symbol::from("BTCUSDT"),
            side: Side::Buy,
            price: Decimal::new(100, 0),
            amount: Decimal::new(1, 0),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    #[tokio::test]
    async fn test_update_order() {
        let (manager, _rx) = create_test_manager();
        let order = create_test_order();

        manager.update_order(order.clone()).await.unwrap();

        let balance = manager.get_balance("BTC").await;
        // Order update doesn't affect balance, so this should be None
        assert!(balance.is_none());
    }

    #[tokio::test]
    async fn test_update_balance() {
        let (manager, _rx) = create_test_manager();
        let balance = Balance {
            symbol: Symbol::from("BTC"),
            free: Decimal::new(10, 0),
            used: Decimal::new(1, 0),
            total: Decimal::new(11, 0),
        };

        manager.update_balance(balance.clone()).await.unwrap();

        let retrieved = manager.get_balance("BTC").await.unwrap();
        assert_eq!(retrieved.symbol, balance.symbol);
        assert_eq!(retrieved.free, balance.free);
    }
}
