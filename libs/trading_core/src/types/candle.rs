//! Normalized completed candles. Exchange-specific parsing stays outside this type.

use crate::types::common::Instrument;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// UTC daily bar length used by Bybit `interval=D` klines.
pub const DAILY_PERIOD_MS: u64 = 86_400_000;

/// Shared completed-candle shape for strategy runtime input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedCandle {
    pub venue: SmolStr,
    pub product: SmolStr,
    pub instrument: Instrument,
    pub interval: SmolStr,
    pub start_time_ms: u64,
    pub end_time_exclusive_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

impl CompletedCandle {
    pub fn is_completed(&self, now_ms: u64) -> bool {
        self.end_time_exclusive_ms <= now_ms
    }

    pub fn ohlcv_eq(&self, other: &Self) -> bool {
        self.start_time_ms == other.start_time_ms
            && self.end_time_exclusive_ms == other.end_time_exclusive_ms
            && self.open == other.open
            && self.high == other.high
            && self.low == other.low
            && self.close == other.close
            && self.volume == other.volume
    }
}
