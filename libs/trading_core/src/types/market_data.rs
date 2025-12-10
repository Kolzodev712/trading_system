//! Market data types (orderbooks, public trades)
use crate::types::common::Pair;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// New Orderbook format (with pair and structured bids/asks)
/// This is the new format for the refactored system
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructuredOrderbook {
    /// Trading pair
    pub pair: Pair,
    /// Bid prices and quantities (price, quantity pairs)
    pub bids: Vec<(Decimal, Decimal)>,
    /// Ask prices and quantities (price, quantity pairs)
    pub asks: Vec<(Decimal, Decimal)>,
    /// Last update timestamp
    pub timestamp: u64,
}

impl StructuredOrderbook {
    /// Get the best bid price
    pub fn best_bid_price(&self) -> Option<Decimal> {
        self.bids.first().map(|(price, _)| *price)
    }

    /// Get the best ask price
    pub fn best_ask_price(&self) -> Option<Decimal> {
        self.asks.first().map(|(price, _)| *price)
    }

    /// Get the mid price (average of best bid and ask)
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid_price(), self.best_ask_price()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::from(2)),
            _ => None,
        }
    }

    /// Get total volume (sum of all bid and ask quantities)
    pub fn total_volume(&self) -> Decimal {
        let bid_volume: Decimal = self.bids.iter().map(|(_, qty)| qty).sum();
        let ask_volume: Decimal = self.asks.iter().map(|(_, qty)| qty).sum();
        bid_volume + ask_volume
    }
}

// Re-export for convenience
pub use StructuredOrderbook as Orderbook;
pub use StructuredPublicTrade as PublicTrade;
pub use StructuredPublicTrade as Trade;

/// New PublicTrade format (with pair)
/// This is the new format for the refactored system
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructuredPublicTrade {
    /// Trading pair
    pub pair: Pair,
    /// Trade price
    pub price: Decimal,
    /// Trade quantity
    pub quantity: Decimal,
    /// Trade timestamp
    pub timestamp: u64,
    /// Whether the trade was made by a market maker
    pub is_market_maker: bool,
    /// Trade ID (if available)
    pub trade_id: Option<smol_str::SmolStr>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_structured_orderbook_best_bid_price() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![
                (Decimal::new(90000, 0), Decimal::new(1, 0)),
                (Decimal::new(89900, 0), Decimal::new(2, 0)),
            ],
            asks: vec![
                (Decimal::new(91000, 0), Decimal::new(1, 0)),
                (Decimal::new(91100, 0), Decimal::new(2, 0)),
            ],
            timestamp: 1734819696263,
        };

        assert_eq!(orderbook.best_bid_price(), Some(Decimal::new(90000, 0)));
    }

    #[test]
    fn test_structured_orderbook_best_ask_price() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![(Decimal::new(90000, 0), Decimal::new(1, 0))],
            asks: vec![
                (Decimal::new(91000, 0), Decimal::new(1, 0)),
                (Decimal::new(91100, 0), Decimal::new(2, 0)),
            ],
            timestamp: 1734819696263,
        };

        assert_eq!(orderbook.best_ask_price(), Some(Decimal::new(91000, 0)));
    }

    #[test]
    fn test_structured_orderbook_best_prices_empty() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![],
            asks: vec![],
            timestamp: 1734819696263,
        };

        assert_eq!(orderbook.best_bid_price(), None);
        assert_eq!(orderbook.best_ask_price(), None);
    }

    #[test]
    fn test_structured_orderbook_mid_price() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![(Decimal::new(90000, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(91000, 0), Decimal::new(1, 0))],
            timestamp: 1734819696263,
        };

        let mid = orderbook.mid_price().unwrap();
        assert_eq!(mid, Decimal::new(90500, 0));
    }

    #[test]
    fn test_structured_orderbook_mid_price_missing_bid() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![],
            asks: vec![(Decimal::new(91000, 0), Decimal::new(1, 0))],
            timestamp: 1734819696263,
        };

        assert_eq!(orderbook.mid_price(), None);
    }

    #[test]
    fn test_structured_orderbook_total_volume() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![
                (Decimal::new(90000, 0), Decimal::new(1, 0)),
                (Decimal::new(89900, 0), Decimal::new(2, 0)),
            ],
            asks: vec![
                (Decimal::new(91000, 0), Decimal::new(3, 0)),
                (Decimal::new(91100, 0), Decimal::new(4, 0)),
            ],
            timestamp: 1734819696263,
        };

        let total_volume = orderbook.total_volume();
        assert_eq!(total_volume, Decimal::new(10, 0)); // 1 + 2 + 3 + 4
    }

    #[test]
    fn test_structured_orderbook_total_volume_empty() {
        let orderbook = StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![],
            asks: vec![],
            timestamp: 1734819696263,
        };

        assert_eq!(orderbook.total_volume(), Decimal::ZERO);
    }

    #[test]
    fn test_structured_orderbook_serialization() {
        use crate::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        let orderbook = StructuredOrderbook {
            pair,
            bids: vec![
                (Decimal::new(90000, 0), Decimal::new(1, 0)),
                (Decimal::new(89900, 0), Decimal::new(2, 0)),
            ],
            asks: vec![(Decimal::new(91000, 0), Decimal::new(1, 0))],
            timestamp: 1734819696263,
        };

        let serialized = serde_json::to_string(&orderbook).unwrap();
        assert!(serialized.contains("bids"));
        assert!(serialized.contains("asks"));
    }

    #[test]
    fn test_structured_trade_serialization() {
        use crate::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        let trade = StructuredPublicTrade {
            pair,
            price: Decimal::new(90000, 0),
            quantity: Decimal::new(10, 2),
            timestamp: 1734819696263,
            is_market_maker: false,
            trade_id: None,
        };

        let serialized = serde_json::to_string(&trade).unwrap();
        assert!(serialized.contains("90000"));
    }

    #[test]
    fn test_structured_public_trade_creation() {
        let trade = StructuredPublicTrade {
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(90000, 0),
            quantity: Decimal::new(10, 2),
            timestamp: 1734819696263,
            is_market_maker: false,
            trade_id: Some(smol_str::SmolStr::new("12345")),
        };

        assert_eq!(trade.pair.base, "BTC");
        assert_eq!(trade.price, Decimal::new(90000, 0));
    }
}
