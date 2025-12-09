//! In-memory state structure for raw exchange data
//!
//! This module defines `ExchangeState` which holds the authoritative state
//! of what the exchange reports (balances, orders, trades from WebSocket).

use std::collections::HashMap;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::{
        common::{OrderId, Pair, Symbol},
        exchange::{Balance, Order, Trade},
    },
};

/// In-memory state for raw exchange data (what the exchange says)
///
/// This state is updated from WebSocket streams and reconciled with REST API.
/// It does NOT track positions - positions are derived from balances by Trading Engine.
#[derive(Debug, Clone)]
pub struct ExchangeState {
    /// Orders by order ID (all orders from exchange)
    orders: HashMap<OrderId, Order>,

    /// Orders by pair (for quick lookup by trading pair)
    orders_by_pair: HashMap<Pair, Vec<OrderId>>,

    /// Symbol registry for symbol-to-pair conversions
    symbol_registry: Arc<SymbolRegistry>,

    /// Balances by currency symbol (all balances from exchange)
    balances: HashMap<String, Balance>,

    /// Recent private trades (last N from exchange)
    private_trades: Vec<Trade>,

    /// Last update timestamp
    last_update: u64,
}

impl ExchangeState {
    /// Create a new empty exchange state
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            orders_by_pair: HashMap::new(),
            symbol_registry: Arc::new(SymbolRegistry::new()),
            balances: HashMap::new(),
            private_trades: Vec::new(),
            last_update: 0,
        }
    }

    /// Create a new exchange state with a symbol registry
    ///
    /// # Arguments
    /// * `symbol_registry` - Shared symbol registry for symbol-to-pair conversions
    pub fn with_registry(symbol_registry: Arc<SymbolRegistry>) -> Self {
        Self {
            orders: HashMap::new(),
            orders_by_pair: HashMap::new(),
            symbol_registry,
            balances: HashMap::new(),
            private_trades: Vec::new(),
            last_update: 0,
        }
    }

    /// Get a reference to the symbol registry
    pub fn symbol_registry(&self) -> &Arc<SymbolRegistry> {
        &self.symbol_registry
    }

    /// Update or add an order
    ///
    /// The order will be indexed by Pair if a symbol-to-pair mapping exists in the registry.
    /// If no mapping exists, the order is still stored but not indexed by pair.
    pub fn update_order(&mut self, order: Order) {
        let order_id = order.order_id.clone();
        let symbol = order.symbol.clone();

        // Update orders map
        self.orders.insert(order_id.clone(), order);

        // Update orders_by_pair index if we have a mapping in the registry
        if let Some(pair) = self.symbol_registry.get_pair(&symbol) {
            self.orders_by_pair
                .entry(pair.clone())
                .or_insert_with(Vec::new)
                .push(order_id);
        }

        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Remove an order
    pub fn remove_order(&mut self, order_id: &OrderId) {
        if let Some(order) = self.orders.remove(order_id) {
            let symbol = order.symbol;
            // Remove from orders_by_pair index if we have a mapping in the registry
            if let Some(pair) = self.symbol_registry.get_pair(&symbol) {
                if let Some(orders) = self.orders_by_pair.get_mut(pair) {
                    orders.retain(|id| id != order_id);
                    if orders.is_empty() {
                        self.orders_by_pair.remove(pair);
                    }
                }
            }
        }

        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Update or add a balance
    pub fn update_balance(&mut self, balance: Balance) {
        let symbol = balance.symbol.as_ref().to_string();
        self.balances.insert(symbol, balance);

        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Update all balances (used during reconciliation)
    pub fn update_balances(&mut self, balances: Vec<Balance>) {
        for balance in balances {
            self.update_balance(balance);
        }
    }

    /// Add a private trade
    pub fn add_trade(&mut self, trade: Trade) {
        self.private_trades.push(trade);
        // Keep only last 1000 trades to prevent unbounded growth
        if self.private_trades.len() > 1000 {
            self.private_trades.remove(0);
        }

        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Update trades for a pair (used during reconciliation)
    pub fn update_trades(&mut self, pair: &Pair, trades: Vec<Trade>) {
        // Remove old trades for this pair
        self.private_trades.retain(|t| &t.pair != pair);
        // Add new trades
        for trade in trades {
            self.add_trade(trade);
        }
    }

    /// Get all orders
    pub fn get_orders(&self) -> Vec<&Order> {
        self.orders.values().collect()
    }

    /// Get orders for a specific pair
    pub fn get_orders_for_pair(&self, pair: &Pair) -> Vec<&Order> {
        self.orders_by_pair
            .get(pair)
            .map(|order_ids| {
                order_ids
                    .iter()
                    .filter_map(|id| self.orders.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get orders for a specific symbol (uses symbol registry if available)
    pub fn get_orders_for_symbol(&self, symbol: &Symbol) -> Vec<&Order> {
        // Try to get via pair mapping first (more efficient)
        if let Some(pair) = self.symbol_registry.get_pair(symbol) {
            return self.get_orders_for_pair(pair);
        }
        // Fallback: filter all orders by symbol
        self.orders
            .values()
            .filter(|o| &o.symbol == symbol)
            .collect()
    }

    /// Get an order by ID
    ///
    /// # Arguments
    /// * `order_id` - Order ID
    ///
    /// # Returns
    /// Reference to the order if found, None otherwise
    pub fn get_order_by_id(&self, order_id: &OrderId) -> Option<&Order> {
        self.orders.get(order_id)
    }

    /// Get all balances
    pub fn get_balances(&self) -> Vec<&Balance> {
        self.balances.values().collect()
    }

    /// Get balance for a currency
    pub fn get_balance(&self, currency: &str) -> Option<&Balance> {
        self.balances.get(currency)
    }

    /// Get all private trades
    pub fn get_trades(&self) -> Vec<&Trade> {
        self.private_trades.iter().collect()
    }

    /// Get trades for a specific pair
    pub fn get_trades_for_pair(&self, pair: &trading_core::types::common::Pair) -> Vec<&Trade> {
        self.private_trades
            .iter()
            .filter(|t| &t.pair == pair)
            .collect()
    }

    /// Get all open orders (orders that are not filled, cancelled, rejected, or expired)
    pub fn get_open_orders(&self) -> Vec<&Order> {
        use trading_core::types::common::OrderStatus;
        self.orders
            .values()
            .filter(|o| matches!(o.status, OrderStatus::New | OrderStatus::PartiallyFilled))
            .collect()
    }

    /// Get last update timestamp
    pub fn last_update(&self) -> u64 {
        self.last_update
    }
}

impl Default for ExchangeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::{OrderStatus, Side, Symbol};

    fn create_test_order(order_id: &str, symbol: &str, status: OrderStatus) -> Order {
        Order {
            order_id: OrderId::new(order_id.into()),
            symbol: Symbol::from(symbol),
            side: Side::Buy,
            price: Decimal::new(100, 0),
            amount: Decimal::new(1, 0),
            internal_created_at: 0,
            exchange_created_at: None,
            status,
            filled_amount: Decimal::ZERO,
        }
    }

    fn create_test_balance(symbol: &str) -> Balance {
        Balance {
            symbol: Symbol::from(symbol),
            free: Decimal::new(100, 0),
            used: Decimal::new(10, 0),
            total: Decimal::new(110, 0),
        }
    }

    #[test]
    fn test_exchange_state_new() {
        let state = ExchangeState::new();
        assert!(state.orders.is_empty());
        assert!(state.balances.is_empty());
        assert!(state.private_trades.is_empty());
    }

    #[test]
    fn test_update_order() {
        use std::sync::Arc;
        use trading_core::registry::symbol::SymbolRegistry;

        let mut registry = SymbolRegistry::new();
        let pair = Pair::new("BTC", "USDT");
        let symbol = Symbol::from("BTCUSDT");
        registry.register(symbol.clone(), pair.clone());

        let mut state = ExchangeState::with_registry(Arc::new(registry));
        let order = create_test_order("123", "BTCUSDT", OrderStatus::New);

        state.update_order(order.clone());

        assert_eq!(state.orders.len(), 1);
        assert_eq!(state.orders_by_pair.get(&pair).unwrap().len(), 1);
    }

    #[test]
    fn test_remove_order() {
        use std::sync::Arc;
        use trading_core::registry::symbol::SymbolRegistry;

        let mut registry = SymbolRegistry::new();
        let pair = Pair::new("BTC", "USDT");
        let symbol = Symbol::from("BTCUSDT");
        registry.register(symbol, pair.clone());

        let mut state = ExchangeState::with_registry(Arc::new(registry));
        let order = create_test_order("123", "BTCUSDT", OrderStatus::New);
        let order_id = order.order_id.clone();

        state.update_order(order);
        assert_eq!(state.orders.len(), 1);

        state.remove_order(&order_id);
        assert_eq!(state.orders.len(), 0);
        assert!(
            state.orders_by_pair.get(&pair).is_none()
                || state.orders_by_pair.get(&pair).unwrap().is_empty()
        );
    }

    #[test]
    fn test_update_balance() {
        let mut state = ExchangeState::new();
        let balance = create_test_balance("BTC");

        state.update_balance(balance.clone());

        assert_eq!(state.balances.len(), 1);
        assert_eq!(state.get_balance("BTC").unwrap().symbol, balance.symbol);
    }

    #[test]
    fn test_get_orders_for_pair() {
        use std::sync::Arc;
        use trading_core::registry::symbol::SymbolRegistry;

        let mut registry = SymbolRegistry::new();
        let btc_pair = Pair::new("BTC", "USDT");
        let eth_pair = Pair::new("ETH", "USDT");
        registry.register(Symbol::from("BTCUSDT"), btc_pair.clone());
        registry.register(Symbol::from("ETHUSDT"), eth_pair);

        let mut state = ExchangeState::with_registry(Arc::new(registry));
        let order1 = create_test_order("1", "BTCUSDT", OrderStatus::New);
        let order2 = create_test_order("2", "BTCUSDT", OrderStatus::New);
        let order3 = create_test_order("3", "ETHUSDT", OrderStatus::New);

        state.update_order(order1);
        state.update_order(order2);
        state.update_order(order3);

        let btc_orders = state.get_orders_for_pair(&btc_pair);
        assert_eq!(btc_orders.len(), 2);
    }

    #[test]
    fn test_get_open_orders() {
        let mut state = ExchangeState::new();
        let order1 = create_test_order("1", "BTCUSDT", OrderStatus::New);
        let order2 = create_test_order("2", "BTCUSDT", OrderStatus::Filled);
        let order3 = create_test_order("3", "ETHUSDT", OrderStatus::PartiallyFilled);

        state.update_order(order1);
        state.update_order(order2);
        state.update_order(order3);

        let open_orders = state.get_open_orders();
        assert_eq!(open_orders.len(), 2); // New and PartiallyFilled
    }
}
