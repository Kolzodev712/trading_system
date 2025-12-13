//! In-memory state structure for market data
//!
//! This module defines `MarketDataState` which holds the latest market data
//! (orderbooks, public trades) received from WebSocket streams.

use std::collections::HashMap;
use trading_core::types::{
    common::Pair,
    market_data::{StructuredOrderbook, StructuredPublicTrade},
};

/// In-memory state for market data
///
/// This state is updated from WebSocket streams and published to channels.
/// No reconciliation needed (market data is public).
#[derive(Debug, Clone)]
pub struct MarketDataState {
    /// Orderbooks by pair (latest orderbook per pair)
    orderbooks: HashMap<Pair, StructuredOrderbook>,

    /// Recent public trades by pair (last N per pair)
    public_trades: HashMap<Pair, Vec<StructuredPublicTrade>>,

    /// Last update timestamp
    last_update: u64,
}

impl MarketDataState {
    /// Create a new empty market data state
    pub fn new() -> Self {
        Self {
            orderbooks: HashMap::new(),
            public_trades: HashMap::new(),
            last_update: 0,
        }
    }

    /// Update orderbook for a pair
    pub fn update_orderbook(&mut self, pair: Pair, orderbook: StructuredOrderbook) {
        // Use timestamp from orderbook if available, otherwise use current time
        let timestamp = orderbook.timestamp;

        // Pair is moved into the HashMap, no need to clone
        self.orderbooks.insert(pair, orderbook);
        self.last_update = timestamp;
    }

    /// Add a public trade for a pair
    pub fn add_public_trade(&mut self, pair: Pair, trade: StructuredPublicTrade) {
        let trades = self.public_trades.entry(pair).or_default();
        trades.push(trade);

        // Keep only last 100 trades per pair to prevent unbounded growth
        if trades.len() > 100 {
            trades.remove(0);
        }

        // Use timestamp from trade if available, otherwise use current time
        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Get orderbook for a pair
    pub fn get_orderbook(&self, pair: &Pair) -> Option<&StructuredOrderbook> {
        self.orderbooks.get(pair)
    }

    /// Get orderbook timestamp for a pair
    pub fn get_orderbook_timestamp(&self, pair: &Pair) -> Option<u64> {
        self.orderbooks.get(pair).map(|ob| ob.timestamp)
    }

    /// Get recent public trades for a pair
    pub fn get_public_trades(&self, pair: &Pair) -> Vec<&StructuredPublicTrade> {
        self.public_trades
            .get(pair)
            .map(|trades| trades.iter().collect())
            .unwrap_or_default()
    }

    /// Get all pairs with orderbooks
    pub fn get_pairs_with_orderbooks(&self) -> Vec<Pair> {
        self.orderbooks.keys().cloned().collect()
    }

    /// Get last update timestamp
    pub fn last_update(&self) -> u64 {
        self.last_update
    }
}

impl Default for MarketDataState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn create_test_orderbook() -> StructuredOrderbook {
        StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![(Decimal::new(100, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(101, 0), Decimal::new(1, 0))],
            timestamp: 1000,
        }
    }

    fn create_test_trade() -> StructuredPublicTrade {
        StructuredPublicTrade {
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(100, 0),
            quantity: Decimal::new(1, 0),
            timestamp: 1000,
            is_market_maker: false,
            trade_id: None,
        }
    }

    #[test]
    fn test_market_data_state_new() {
        let state = MarketDataState::new();
        assert!(state.orderbooks.is_empty());
        assert!(state.public_trades.is_empty());
    }

    #[test]
    fn test_update_orderbook() {
        let mut state = MarketDataState::new();
        let pair = Pair::new("BTC", "USDT");
        let orderbook = create_test_orderbook();

        state.update_orderbook(pair.clone(), orderbook.clone());

        assert_eq!(state.orderbooks.len(), 1);
        assert_eq!(state.get_orderbook(&pair), Some(&orderbook));
        assert!(state.get_orderbook_timestamp(&pair).is_some());
    }

    #[test]
    fn test_add_public_trade() {
        let mut state = MarketDataState::new();
        let pair = Pair::new("BTC", "USDT");
        let trade = create_test_trade();

        state.add_public_trade(pair.clone(), trade.clone());

        let trades = state.get_public_trades(&pair);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, trade.price);
    }

    #[test]
    fn test_public_trades_limit() {
        let mut state = MarketDataState::new();
        let pair = Pair::new("BTC", "USDT");

        // Add more than 100 trades
        for i in 0..150 {
            let trade = StructuredPublicTrade {
                pair: pair.clone(),
                price: Decimal::new(i as i64, 0),
                quantity: Decimal::new(1, 0),
                timestamp: i as u64,
                is_market_maker: false,
                trade_id: None,
            };
            state.add_public_trade(pair.clone(), trade);
        }

        // Should only keep last 100
        let trades = state.get_public_trades(&pair);
        assert_eq!(trades.len(), 100);
    }
}
