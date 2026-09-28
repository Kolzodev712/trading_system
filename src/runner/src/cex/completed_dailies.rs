//! Runner-owned completed daily candle source.
//!
//! Fetches public klines, drops forming bars, dedupes, and dispatches accepted
//! candles through StrategyManager. Does not place orders.

use std::collections::HashMap;
use tracing::warn;
use trading_core::types::common::{Exchange, InstrumentType, Pair};
use trading_core::types::DAILY_PERIOD_MS;
use trading_engine::cex::completed_candles::{
    daily_from_replay_candle, product_label, venue_label, SequentialCandleIngest,
};
use trading_engine::strategies::StrategyManager;

use super::{completed_candles, fetch_bybit_klines};

/// Extra completed days beyond N so bootstrap can reconstruct current/previous
/// state and the latest in-window transition. N=28 needs 29; 60 is margin.
pub const DAILY_BOOTSTRAP_LIMIT: u32 = 60;

pub struct DailyCompletedCandleSource {
    ingest_by_pair: HashMap<Pair, SequentialCandleIngest>,
    instrument_type: InstrumentType,
    venue: &'static str,
    product: &'static str,
}

impl DailyCompletedCandleSource {
    pub fn new(exchange: Exchange, instrument_type: InstrumentType) -> Self {
        let product = product_label(&instrument_type);
        Self {
            ingest_by_pair: HashMap::new(),
            venue: venue_label(exchange),
            product,
            instrument_type,
        }
    }

    pub fn ingest_fetched(
        &mut self,
        pair: &Pair,
        candles: Vec<trading_engine::cex::strategies::replay::Candle>,
        now_ms: u64,
    ) -> Vec<trading_core::types::CompletedCandle> {
        let closed = completed_candles(candles, now_ms, DAILY_PERIOD_MS);
        let completed = closed.iter().map(|candle| {
            daily_from_replay_candle(
                candle,
                self.venue,
                self.product,
                pair.clone(),
                self.instrument_type.clone(),
            )
        });
        let ingest = self.ingest_by_pair.entry(pair.clone()).or_default();
        ingest.accept_all(completed, now_ms)
    }
}

fn load_fixture_candles(
    path: &str,
) -> Result<Vec<trading_engine::cex::strategies::replay::Candle>, String> {
    let raw =
        std::fs::read_to_string(path).map_err(|err| format!("candle fixture {path}: {err}"))?;
    serde_json::from_str(&raw).map_err(|err| format!("candle fixture parse: {err}"))
}

impl DailyCompletedCandleSource {
    pub async fn poll_pair(
        &mut self,
        pair: &Pair,
        manager: &StrategyManager,
    ) -> Result<usize, String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let candles = if let Ok(path) = std::env::var("TRADING_TEST_CANDLES_PATH") {
            load_fixture_candles(&path)?
        } else {
            fetch_bybit_klines(pair, "D", DAILY_BOOTSTRAP_LIMIT).await?
        };
        let as_of = std::env::var("TRADING_TEST_NOW_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                candles
                    .iter()
                    .map(|c| c.timestamp_ms)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(DAILY_PERIOD_MS)
                    .max(now_ms)
            });
        let accepted = self.ingest_fetched(pair, candles, as_of);
        let n = accepted.len();
        for candle in &accepted {
            manager
                .on_completed_candle(candle)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(n)
    }

    pub async fn poll_pairs(&mut self, pairs: &[Pair], manager: &StrategyManager) -> usize {
        let mut accepted = 0;
        for pair in pairs {
            match self.poll_pair(pair, manager).await {
                Ok(n) => accepted += n,
                Err(err) => warn!("Completed daily candle poll failed for {pair}: {err}"),
            }
        }
        accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_engine::cex::strategies::replay::Candle;

    fn candle(start: u64, close: i64) -> Candle {
        let price = Decimal::new(close, 0);
        Candle {
            timestamp_ms: start,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: Decimal::ONE,
        }
    }

    #[test]
    fn forming_daily_is_not_dispatched() {
        let mut source = DailyCompletedCandleSource::new(Exchange::Bybit, InstrumentType::Futures);
        let pair = Pair::new("BTC", "USDT");
        let bars = vec![candle(0, 100), candle(DAILY_PERIOD_MS, 110)];
        let now = DAILY_PERIOD_MS + 1;
        let accepted = source.ingest_fetched(&pair, bars, now);
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].start_time_ms, 0);
    }

    #[test]
    fn duplicate_poll_is_idempotent() {
        let mut source = DailyCompletedCandleSource::new(Exchange::Bybit, InstrumentType::Futures);
        let pair = Pair::new("BTC", "USDT");
        let bars = vec![candle(0, 100)];
        let now = DAILY_PERIOD_MS;
        let first = source.ingest_fetched(&pair, bars.clone(), now);
        let second = source.ingest_fetched(&pair, bars, now);
        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[test]
    fn fixture_file_loads_completed_bars() {
        let dir = std::env::temp_dir().join(format!("inc9a-candles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.json");
        std::fs::write(
            &path,
            r#"[{"timestamp_ms":0,"open":"100","high":"100","low":"100","close":"100","volume":"1"}]"#,
        )
        .unwrap();
        let loaded = load_fixture_candles(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].close, Decimal::new(100, 0));
    }
}
