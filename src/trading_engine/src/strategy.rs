use async_trait::async_trait;
use core_logic::{
    data::{
        base_data::{OrderId, Pair},
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
}

impl TradingState {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            active_orders: HashMap::new(),
        }
    }

    pub fn add_order(&mut self, order: Order) {
        self.active_orders.insert(order.order_id.clone(), order);
    }

    pub fn remove_order(&mut self, order_id: &OrderId) {
        self.active_orders.remove(order_id);
    }

    pub fn update_position(&mut self, symbol: Pair, quantity: Decimal, price: Decimal) {
        let position = self.positions.entry(symbol.clone()).or_insert(Position {
            symbol,
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
            position.quantity += quantity; // quantity is negative for sells
            if position.quantity <= Decimal::ZERO {
                self.positions.remove(&position.symbol);
            }
        }
    }

    pub fn get_position(&self, symbol: &Pair) -> Option<&Position> {
        self.positions.get(symbol)
    }

    pub fn get_active_order_count(&self, symbol: &Pair) -> usize {
        self.active_orders
            .values()
            .filter(|o| &o.symbol == symbol)
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
    async fn on_execution_report(
        &mut self,
        order: &Order,
        state: &mut TradingState,
    ) -> Result<(), EngineError>;

    /// Get the trading state (for external access)
    fn get_state(&self) -> &TradingState;

    /// Get mutable trading state
    fn get_state_mut(&mut self) -> &mut TradingState;
}
