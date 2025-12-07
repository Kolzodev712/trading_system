use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::HashMap;
use trading_core::{
    error::EngineError,
    types::{
        common::{OrderId, OrderStatus, Pair},
        exchange::{Order, OrderRequest},
        market_data::Orderbook,
    },
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Side;

    #[test]
    fn test_trading_state_new() {
        let state = TradingState::new();
        assert!(state.positions.is_empty());
        assert!(state.active_orders.is_empty());
        assert!(state.pending_orders.is_empty());
    }

    #[test]
    fn test_trading_state_add_order_new() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");
        let order = Order {
            order_id: OrderId::new("12345".into()),
            symbol: pair.convert_to_smolstr().into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order.clone());
        assert_eq!(state.active_orders.len(), 1);
        assert_eq!(state.pending_orders.len(), 1);
        assert!(state.pending_orders.contains_key(&order.order_id));
    }

    #[test]
    fn test_trading_state_add_order_filled() {
        let mut state = TradingState::new();
        let order = Order {
            order_id: OrderId::new("12345".into()),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::Filled,
            filled_amount: Decimal::new(10, 2),
        };

        state.add_order(order.clone());
        assert_eq!(state.active_orders.len(), 1);
        assert_eq!(state.pending_orders.len(), 0); // Filled orders not pending
    }

    #[test]
    fn test_trading_state_remove_order() {
        let mut state = TradingState::new();
        let order_id = OrderId::new("12345".into());
        let order = Order {
            order_id: order_id.clone(),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order);
        assert_eq!(state.active_orders.len(), 1);

        state.remove_order(&order_id);
        assert_eq!(state.active_orders.len(), 0);
        assert_eq!(state.pending_orders.len(), 0);
    }

    #[test]
    fn test_trading_state_update_order_status_to_filled() {
        let mut state = TradingState::new();
        let order_id = OrderId::new("12345".into());
        let order = Order {
            order_id: order_id.clone(),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order);
        assert_eq!(state.pending_orders.len(), 1);

        state.update_order_status(&order_id, OrderStatus::Filled, Some(Decimal::new(10, 2)));
        assert_eq!(state.pending_orders.len(), 0);
        assert_eq!(
            state.active_orders.get(&order_id).unwrap().status,
            OrderStatus::Filled
        );
        assert_eq!(
            state.active_orders.get(&order_id).unwrap().filled_amount,
            Decimal::new(10, 2)
        );
    }

    #[test]
    fn test_trading_state_update_position_add() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(90000, 0));
        let position = state.get_position(&pair).unwrap();
        assert_eq!(position.quantity, Decimal::new(1, 0));
        assert_eq!(position.avg_price, Decimal::new(90000, 0));
    }

    #[test]
    fn test_trading_state_update_position_add_multiple() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        // First buy at 90000
        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(90000, 0));
        // Second buy at 91000
        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(91000, 0));

        let position = state.get_position(&pair).unwrap();
        assert_eq!(position.quantity, Decimal::new(2, 0));
        // Average should be (90000 + 91000) / 2 = 90500
        assert_eq!(position.avg_price, Decimal::new(90500, 0));
    }

    #[test]
    fn test_trading_state_update_position_reduce() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        // Add position
        state.update_position(pair.clone(), Decimal::new(2, 0), Decimal::new(90000, 0));
        // Reduce position (sell)
        state.update_position(pair.clone(), Decimal::new(-1, 0), Decimal::new(91000, 0));

        let position = state.get_position(&pair).unwrap();
        assert_eq!(position.quantity, Decimal::new(1, 0));
    }

    #[test]
    fn test_trading_state_update_position_close() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        // Add position
        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(90000, 0));
        // Close position completely
        state.update_position(pair.clone(), Decimal::new(-1, 0), Decimal::new(91000, 0));

        assert!(state.get_position(&pair).is_none());
    }

    #[test]
    fn test_trading_state_get_active_order_count() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        let order1 = Order {
            order_id: OrderId::new("1".into()),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let order2 = Order {
            order_id: OrderId::new("2".into()),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let order3 = Order {
            order_id: OrderId::new("3".into()),
            symbol: "ETHUSDT".into(), // Different symbol
            side: Side::Buy,
            price: Decimal::new(3000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order1);
        state.add_order(order2);
        state.add_order(order3);

        assert_eq!(state.get_active_order_count(&pair), 2);
    }

    #[test]
    fn test_trading_state_get_stale_orders() {
        let mut state = TradingState::new();
        let order_id = OrderId::new("12345".into());
        let order = Order {
            order_id: order_id.clone(),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order);

        // Manually set a very old timestamp
        let old_timestamp = 1000; // Very old
        state.pending_orders.insert(order_id.clone(), old_timestamp);

        let stale = state.get_stale_orders(60); // 60 second timeout
        assert!(stale.contains(&order_id));
    }
}
