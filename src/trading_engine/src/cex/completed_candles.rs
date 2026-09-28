//! Runner-owned completed-candle ingest: forming filter, idempotence, conflict and gap detection.
//!
//! Does not synthesize missing bars. Exchange-specific HTTP parsing stays in the caller.

use crate::cex::strategies::replay::Candle;
use trading_core::types::{
    common::{Instrument, Pair},
    CompletedCandle, DAILY_PERIOD_MS,
};

/// Sequential ingest keyed by last accepted completed bar.
#[derive(Debug, Default, Clone)]
pub struct SequentialCandleIngest {
    last: Option<CompletedCandle>,
}

/// Outcome of observing one candle. Only [`CandleIngestEvent::Accepted`] should reach a strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandleIngestEvent {
    IgnoredForming,
    Duplicate,
    Conflict {
        existing: Box<CompletedCandle>,
        incoming: Box<CompletedCandle>,
    },
    Gap {
        expected_start_ms: u64,
        candle: Box<CompletedCandle>,
    },
    OutOfOrder {
        last_start_ms: u64,
        candle: Box<CompletedCandle>,
    },
    Accepted(Box<CompletedCandle>),
}

impl SequentialCandleIngest {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn last(&self) -> Option<&CompletedCandle> {
        self.last.as_ref()
    }

    pub fn observe(&mut self, candle: CompletedCandle, now_ms: u64) -> CandleIngestEvent {
        if !candle.is_completed(now_ms) {
            return CandleIngestEvent::IgnoredForming;
        }
        if let Some(prev) = &self.last {
            if candle.start_time_ms < prev.start_time_ms {
                return CandleIngestEvent::OutOfOrder {
                    last_start_ms: prev.start_time_ms,
                    candle: Box::new(candle),
                };
            }
            if prev.start_time_ms == candle.start_time_ms {
                if prev.ohlcv_eq(&candle) {
                    return CandleIngestEvent::Duplicate;
                }
                return CandleIngestEvent::Conflict {
                    existing: Box::new(prev.clone()),
                    incoming: Box::new(candle),
                };
            }
            if prev.end_time_exclusive_ms != candle.start_time_ms {
                return CandleIngestEvent::Gap {
                    expected_start_ms: prev.end_time_exclusive_ms,
                    candle: Box::new(candle),
                };
            }
        }
        self.last = Some(candle.clone());
        CandleIngestEvent::Accepted(Box::new(candle))
    }

    /// Push a chronological batch. Returns accepted candles only; gaps/conflicts are not applied.
    pub fn accept_all(
        &mut self,
        candles: impl IntoIterator<Item = CompletedCandle>,
        now_ms: u64,
    ) -> Vec<CompletedCandle> {
        let mut accepted = Vec::new();
        for candle in candles {
            if let CandleIngestEvent::Accepted(candle) = self.observe(candle, now_ms) {
                accepted.push(*candle);
            }
        }
        accepted
    }
}

pub fn from_replay_candle(
    candle: &Candle,
    venue: impl Into<smol_str::SmolStr>,
    product: impl Into<smol_str::SmolStr>,
    instrument: Instrument,
    interval: impl Into<smol_str::SmolStr>,
    period_ms: u64,
) -> CompletedCandle {
    CompletedCandle {
        venue: venue.into(),
        product: product.into(),
        instrument,
        interval: interval.into(),
        start_time_ms: candle.timestamp_ms,
        end_time_exclusive_ms: candle.timestamp_ms.saturating_add(period_ms),
        open: candle.open,
        high: candle.high,
        low: candle.low,
        close: candle.close,
        volume: candle.volume,
    }
}

pub fn daily_from_replay_candle(
    candle: &Candle,
    venue: impl Into<smol_str::SmolStr>,
    product: impl Into<smol_str::SmolStr>,
    pair: Pair,
    instrument_type: trading_core::types::common::InstrumentType,
) -> CompletedCandle {
    from_replay_candle(
        candle,
        venue,
        product,
        Instrument::new(instrument_type, pair),
        "D",
        DAILY_PERIOD_MS,
    )
}

pub fn product_label(
    instrument_type: &trading_core::types::common::InstrumentType,
) -> &'static str {
    match instrument_type {
        trading_core::types::common::InstrumentType::Futures
        | trading_core::types::common::InstrumentType::Derivitives => "linear",
        trading_core::types::common::InstrumentType::Spot => "spot",
    }
}

pub fn venue_label(exchange: trading_core::types::common::Exchange) -> &'static str {
    match exchange {
        trading_core::types::common::Exchange::Bybit => "bybit",
        trading_core::types::common::Exchange::Binance => "binance",
        trading_core::types::common::Exchange::Paper => "paper",
        trading_core::types::common::Exchange::DeepBook => "deepbook",
        trading_core::types::common::Exchange::Unsupported => "unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::{InstrumentType, Pair};

    fn bar(start: u64, close: i64) -> CompletedCandle {
        let price = Decimal::new(close, 0);
        CompletedCandle {
            venue: "bybit".into(),
            product: "linear".into(),
            instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
            interval: "D".into(),
            start_time_ms: start,
            end_time_exclusive_ms: start + DAILY_PERIOD_MS,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: Decimal::ONE,
        }
    }

    #[test]
    fn forming_candle_is_ignored() {
        let mut ingest = SequentialCandleIngest::new();
        let candle = bar(1_000, 100);
        let event = ingest.observe(candle.clone(), candle.end_time_exclusive_ms - 1);
        assert_eq!(event, CandleIngestEvent::IgnoredForming);
        assert!(ingest.last().is_none());
    }

    #[test]
    fn duplicate_is_idempotent() {
        let mut ingest = SequentialCandleIngest::new();
        let candle = bar(1_000, 100);
        let now = candle.end_time_exclusive_ms;
        assert!(matches!(
            ingest.observe(candle.clone(), now),
            CandleIngestEvent::Accepted(_)
        ));
        assert_eq!(ingest.observe(candle, now), CandleIngestEvent::Duplicate);
    }

    #[test]
    fn conflicting_duplicate_is_detected() {
        let mut ingest = SequentialCandleIngest::new();
        let first = bar(1_000, 100);
        let now = first.end_time_exclusive_ms;
        ingest.observe(first.clone(), now);
        let mut second = first.clone();
        second.close = Decimal::new(101, 0);
        match ingest.observe(second, now) {
            CandleIngestEvent::Conflict { existing, incoming } => {
                assert_eq!(existing.close, Decimal::new(100, 0));
                assert_eq!(incoming.close, Decimal::new(101, 0));
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        assert_eq!(ingest.last().unwrap().close, Decimal::new(100, 0));
    }

    #[test]
    fn gap_is_not_filled() {
        let mut ingest = SequentialCandleIngest::new();
        let first = bar(0, 100);
        let now = DAILY_PERIOD_MS * 3;
        ingest.observe(first, now);
        let skipped = bar(DAILY_PERIOD_MS * 2, 120);
        match ingest.observe(skipped, now) {
            CandleIngestEvent::Gap {
                expected_start_ms, ..
            } => {
                assert_eq!(expected_start_ms, DAILY_PERIOD_MS);
            }
            other => panic!("expected gap, got {other:?}"),
        }
        assert_eq!(ingest.last().unwrap().start_time_ms, 0);
    }
}
