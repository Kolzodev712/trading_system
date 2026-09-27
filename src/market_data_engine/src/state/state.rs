//! In-memory state structure for market data
//!
//! This module defines `MarketDataState` which holds the latest market data
//! (orderbooks, public trades) received from WebSocket streams.

use std::collections::HashMap;
use trading_core::types::{
    common::Pair,
    market_data::{OrderbookUpdateKind, StructuredOrderbook, StructuredPublicTrade},
};

/// In-memory state for market data
///
/// This state is updated from WebSocket streams and published to channels.
/// No reconciliation needed (market data is public).
#[derive(Debug, Clone)]
pub struct MarketDataState {
    /// Orderbooks by pair (latest orderbook per pair)
    orderbooks: HashMap<Pair, StructuredOrderbook>,

    /// Last applied exchange sequence per pair. Absent after a gap until the next snapshot.
    last_sequence: HashMap<Pair, u64>,

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
            last_sequence: HashMap::new(),
            public_trades: HashMap::new(),
            last_update: 0,
        }
    }

    /// Apply a snapshot or delta.
    ///
    /// Snapshots replace the local book. Deltas merge only when `sequence == last + 1`.
    /// A gap or a delta before any snapshot drops the local book and returns false.
    /// Sources without a sequence merge onto an existing book and do not replace it.
    pub fn update_orderbook(&mut self, pair: Pair, orderbook: StructuredOrderbook) -> bool {
        let timestamp = orderbook.timestamp;
        let applied = match orderbook.update_kind {
            OrderbookUpdateKind::Snapshot => {
                if let Some(seq) = orderbook.sequence {
                    self.last_sequence.insert(pair.clone(), seq);
                } else {
                    self.last_sequence.remove(&pair);
                }
                self.orderbooks.insert(pair, orderbook);
                true
            }
            OrderbookUpdateKind::Delta => self.apply_delta(pair, orderbook),
        };
        if applied {
            self.last_update = timestamp;
        }
        applied
    }

    fn apply_delta(&mut self, pair: Pair, delta: StructuredOrderbook) -> bool {
        if let Some(seq) = delta.sequence {
            match self.last_sequence.get(&pair).copied() {
                Some(last) if seq == last.saturating_add(1) => {}
                _ => {
                    self.orderbooks.remove(&pair);
                    self.last_sequence.remove(&pair);
                    return false;
                }
            }
            self.last_sequence.insert(pair.clone(), seq);
        } else if !self.orderbooks.contains_key(&pair) {
            self.orderbooks.insert(pair, delta);
            return true;
        }

        if let Some(existing) = self.orderbooks.get_mut(&pair) {
            Self::merge_orderbook_delta(existing, &delta);
            true
        } else {
            false
        }
    }

    /// Merge delta update into existing orderbook
    /// - Levels with size 0 are removed
    /// - Levels with new prices are added
    /// - Levels with existing prices are updated
    fn merge_orderbook_delta(existing: &mut StructuredOrderbook, delta: &StructuredOrderbook) {
        // Update bids
        for (price, size) in &delta.bids {
            if size.is_zero() {
                // Remove level if size is 0
                existing.bids.retain(|(p, _)| p != price);
            } else {
                // Update or add level
                if let Some(level) = existing.bids.iter_mut().find(|(p, _)| p == price) {
                    *level = (*price, *size);
                } else {
                    existing.bids.push((*price, *size));
                }
            }
        }

        // Update asks
        for (price, size) in &delta.asks {
            if size.is_zero() {
                // Remove level if size is 0
                existing.asks.retain(|(p, _)| p != price);
            } else {
                // Update or add level
                if let Some(level) = existing.asks.iter_mut().find(|(p, _)| p == price) {
                    *level = (*price, *size);
                } else {
                    existing.asks.push((*price, *size));
                }
            }
        }

        // Sort bids descending (highest first) and asks ascending (lowest first)
        existing.bids.sort_by_key(|a| std::cmp::Reverse(a.0));
        existing.asks.sort_by_key(|a| a.0);

        // Update timestamp
        existing.timestamp = delta.timestamp;
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
            update_kind: OrderbookUpdateKind::Snapshot,
            sequence: Some(1),
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

    fn level(pair: Pair, bid: i64, seq: u64, kind: OrderbookUpdateKind) -> StructuredOrderbook {
        StructuredOrderbook {
            pair,
            bids: vec![(Decimal::new(bid, 0), Decimal::ONE)],
            asks: vec![(Decimal::new(bid + 1, 0), Decimal::ONE)],
            timestamp: seq,
            update_kind: kind,
            sequence: Some(seq),
        }
    }

    #[test]
    fn snapshot_replaces_existing_book() {
        let mut state = MarketDataState::new();
        let pair = Pair::new("BTC", "USDT");
        assert!(state.update_orderbook(
            pair.clone(),
            level(pair.clone(), 100, 1, OrderbookUpdateKind::Snapshot)
        ));
        assert!(state.update_orderbook(
            pair.clone(),
            level(pair.clone(), 50, 8, OrderbookUpdateKind::Snapshot)
        ));
        let book = state.get_orderbook(&pair).unwrap();
        assert_eq!(book.bids.len(), 1);
        assert_eq!(book.bids[0].0, Decimal::new(50, 0));
        assert_eq!(state.last_sequence.get(&pair), Some(&8));
    }

    #[test]
    fn delta_merges_only_when_sequence_continues() {
        let mut state = MarketDataState::new();
        let pair = Pair::new("BTC", "USDT");
        state.update_orderbook(
            pair.clone(),
            level(pair.clone(), 100, 1, OrderbookUpdateKind::Snapshot),
        );
        let mut delta = level(pair.clone(), 90, 2, OrderbookUpdateKind::Delta);
        delta.bids = vec![(Decimal::new(100, 0), Decimal::ZERO)];
        assert!(state.update_orderbook(pair.clone(), delta));
        assert!(state.get_orderbook(&pair).unwrap().bids.is_empty());

        let gap = level(pair.clone(), 70, 9, OrderbookUpdateKind::Delta);
        assert!(!state.update_orderbook(pair.clone(), gap));
        assert!(state.get_orderbook(&pair).is_none());
    }

    #[test]
    fn delta_before_snapshot_is_dropped() {
        let mut state = MarketDataState::new();
        let pair = Pair::new("BTC", "USDT");
        assert!(!state.update_orderbook(
            pair.clone(),
            level(pair.clone(), 100, 1, OrderbookUpdateKind::Delta)
        ));
        assert!(state.get_orderbook(&pair).is_none());
    }
}
