use rust_decimal::Decimal;

/// A price point used for indicator calculations
#[derive(Debug, Clone, Copy)]
pub struct PricePoint {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: u64,
}

impl PricePoint {
    /// Create a price point from a trade
    pub fn from_trade(price: Decimal, quantity: Decimal, timestamp: u64) -> Self {
        Self {
            open: price,
            high: price,
            low: price,
            close: price,
            volume: quantity,
            timestamp,
        }
    }

    /// Create a price point from an orderbook snapshot
    pub fn from_orderbook(
        bid_price: Decimal,
        ask_price: Decimal,
        volume: Decimal,
        timestamp: u64,
    ) -> Self {
        let mid_price = (bid_price + ask_price) / Decimal::from(2);
        Self {
            open: mid_price,
            high: ask_price,
            low: bid_price,
            close: mid_price,
            volume,
            timestamp,
        }
    }

    /// Create a price point from OHLCV data
    pub fn from_ohlcv(
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Decimal,
        timestamp: u64,
    ) -> Self {
        Self {
            open,
            high,
            low,
            close,
            volume,
            timestamp,
        }
    }
}
