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

// Legacy compatibility types - matching old core_logic::data::public_data format
/// Legacy Orderbook (old format - flat Vec<Decimal> for bids/asks)
/// This is kept for backward compatibility during migration
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyOrderbook {
    pub bids: Vec<Decimal>,
    pub asks: Vec<Decimal>,
}

/// Legacy Trade (old format)
/// This is kept for backward compatibility during migration  
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyTrade {
    pub price: Decimal,
    pub quantity: Decimal,
    pub time: u64,
    pub is_market_maker: bool,
}

// Re-export for backward compatibility - old code uses these names
pub use LegacyOrderbook as Orderbook;
pub use LegacyTrade as Trade;

// Also export PublicTrade as alias for StructuredPublicTrade for compatibility
pub use StructuredPublicTrade as PublicTrade;

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
    pub trade_id: Option<String>,
}
