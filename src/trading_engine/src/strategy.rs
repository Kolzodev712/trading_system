use async_trait::async_trait;
use core_logic::{
    data::{
        base_data::{OrderId, OrderStatus, Pair},
        private_data::{Order, OrderRequest},
        public_data::Orderbook,
    },
    error::EngineError,
};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Represents a position in a trading pair
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: Pair,
    pub quantity: Decimal,
    pub avg_price: Decimal,
}

/// Manages active orders and positions
#[derive(Debug, Clone)]
pub struct TradingState {
    pub positions: HashMap<Pair, Position>,
    pub active_orders: HashMap<OrderId, Order>,
    /// Orders that are pending (not yet filled or cancelled)
    pub pending_orders: HashMap<OrderId, u64>, // order_id -> timestamp when order was placed
}

impl TradingState {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            active_orders: HashMap::new(),
            pending_orders: HashMap::new(),
        }
    }

    pub fn add_order(&mut self, order: Order) {
        let order_id = order.order_id.clone();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Track pending orders separately
        if matches!(
            order.status,
            OrderStatus::New | OrderStatus::PartiallyFilled
        ) {
            self.pending_orders.insert(order_id.clone(), timestamp);
        }

        self.active_orders.insert(order_id, order);
    }

    pub fn remove_order(&mut self, order_id: &OrderId) {
        self.active_orders.remove(order_id);
        self.pending_orders.remove(order_id);
    }

    /// Update order status (e.g., when filled or cancelled)
    pub fn update_order_status(
        &mut self,
        order_id: &OrderId,
        status: OrderStatus,
        filled_amount: Option<Decimal>,
    ) {
        if let Some(order) = self.active_orders.get_mut(order_id) {
            order.status = status.clone();
            if let Some(amount) = filled_amount {
                order.filled_amount = amount;
            }

            // Remove from pending if filled or cancelled
            if matches!(
                status,
                OrderStatus::Filled
                    | OrderStatus::Canceled
                    | OrderStatus::Rejected
                    | OrderStatus::Expired
            ) {
                self.pending_orders.remove(order_id);
            }
        }
    }

    /// Get stale orders (pending for more than timeout_secs)
    pub fn get_stale_orders(&self, timeout_secs: u64) -> Vec<OrderId> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.pending_orders
            .iter()
            .filter(|(_, &timestamp)| now.saturating_sub(timestamp) > timeout_secs)
            .map(|(order_id, _)| order_id.clone())
            .collect()
    }

    pub fn update_position(&mut self, symbol: Pair, quantity: Decimal, price: Decimal) {
        let position = self.positions.entry(symbol.clone()).or_insert(Position {
            symbol: symbol.clone(),
            quantity: Decimal::ZERO,
            avg_price: Decimal::ZERO,
        });

        if quantity > Decimal::ZERO {
            // Adding to position
            let total_cost = position.avg_price * position.quantity + price * quantity;
            position.quantity += quantity;
            if position.quantity > Decimal::ZERO {
                position.avg_price = total_cost / position.quantity;
            }
        } else {
            // Reducing position
            let symbol_to_remove = position.symbol.clone();
            position.quantity += quantity; // quantity is negative for sells
            if position.quantity <= Decimal::ZERO {
                self.positions.remove(&symbol_to_remove);
            }
        }
    }

    pub fn get_position(&self, symbol: &Pair) -> Option<&Position> {
        self.positions.get(symbol)
    }

    pub fn get_active_order_count(&self, symbol: &Pair) -> usize {
        let symbol_str = symbol.convert_to_smolstr();
        self.active_orders
            .values()
            .filter(|o| o.symbol.as_ref() == symbol_str.as_str())
            .count()
    }
}

/// Trait that all trading strategies must implement
#[async_trait]
pub trait Strategy: Send + Sync {
    /// Called when a new orderbook update arrives
    /// Returns an optional OrderRequest if the strategy wants to place an order
    async fn on_orderbook(
        &mut self,
        orderbook: &Orderbook,
        state: &TradingState,
    ) -> Result<Option<OrderRequest>, EngineError>;

    /// Called when an execution report is received
    /// This allows the strategy to update its internal state
    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError>;

    /// Get the trading state (for external access)
    fn get_state(&self) -> &TradingState;

    /// Get mutable trading state
    fn get_state_mut(&mut self) -> &mut TradingState;

    /// Get the last known market price (for reconciliation)
    /// Returns None if no price data is available
    fn get_last_price(&self) -> Option<rust_decimal::Decimal> {
        None // Default implementation - strategies can override
    }
}
