//! Runner-owned completed daily candle source.
//!
//! Fetches public klines, drops forming bars, dedupes, and dispatches accepted
//! candles through StrategyManager. Does not place orders.

use std::collections::HashMap;
use tracing::warn;
use trading_core::types::common::{Exchange, InstrumentType, Pair};
use trading_core::types::{ExecutionMode, DAILY_PERIOD_MS};
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

fn is_fresh_entry_candle_fixture(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .is_some_and(|name| name == "inc10_demo_fresh_entry_candles.json")
}

/// Shift completed daily fixture bars so the last bar starts at yesterday UTC relative to `now_ms`.
///
/// OHLC values and bar spacing are unchanged. No-op when the series is empty or already aligned.
pub fn align_daily_replay_to_previous_utc_day(
    mut candles: Vec<trading_engine::cex::strategies::replay::Candle>,
    now_ms: u64,
) -> Vec<trading_engine::cex::strategies::replay::Candle> {
    let Some(last) = candles.last() else {
        return candles;
    };
    let today_start = now_ms - (now_ms % DAILY_PERIOD_MS);
    let target_last_start = today_start.saturating_sub(DAILY_PERIOD_MS);
    let shift = target_last_start as i128 - last.timestamp_ms as i128;
    if shift == 0 {
        return candles;
    }
    for candle in &mut candles {
        let aligned = candle.timestamp_ms as i128 + shift;
        candle.timestamp_ms = u64::try_from(aligned).unwrap_or(0);
    }
    candles
}

impl DailyCompletedCandleSource {
    pub async fn poll_pair(
        &mut self,
        pair: &Pair,
        manager: &StrategyManager,
        execution_mode: ExecutionMode,
        venue: &str,
        environment: &str,
    ) -> Result<usize, String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let candles_path = std::env::var("TRADING_TEST_CANDLES_PATH").ok();
        let now_override = std::env::var("TRADING_TEST_NOW_MS").ok();
        crate::config::exchange_fixture_is_authorized(
            execution_mode,
            venue,
            environment,
            candles_path.as_deref(),
            now_override.is_some(),
        )?;
        let as_of = match now_override.as_deref() {
            Some(raw) => raw
                .parse()
                .map_err(|err| format!("TRADING_TEST_NOW_MS: {err}"))?,
            None => now_ms,
        };
        let candles = if let Some(path) = candles_path.as_deref() {
            tracing::info!(
                path,
                venue,
                environment,
                execution_mode = execution_mode.as_str(),
                synthetic_candle_fixture = true,
                "loading synthetic completed-candle fixture; this is not live market data"
            );
            let loaded = load_fixture_candles(path)?;
            if is_fresh_entry_candle_fixture(path) {
                align_daily_replay_to_previous_utc_day(loaded, as_of)
            } else {
                loaded
            }
        } else {
            fetch_bybit_klines(pair, "D", DAILY_BOOTSTRAP_LIMIT).await?
        };
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

    pub async fn poll_pairs(
        &mut self,
        pairs: &[Pair],
        manager: &StrategyManager,
        execution_mode: ExecutionMode,
        venue: &str,
        environment: &str,
    ) -> usize {
        let mut accepted = 0;
        for pair in pairs {
            match self
                .poll_pair(pair, manager, execution_mode, venue, environment)
                .await
            {
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

    #[test]
    fn wall_clock_as_of_drops_forming_last_bar() {
        let mut source = DailyCompletedCandleSource::new(Exchange::Bybit, InstrumentType::Futures);
        let pair = Pair::new("BTC", "USDT");
        let now = DAILY_PERIOD_MS * 2 + 12 * 3_600_000;
        let bars = vec![
            candle(0, 100),
            candle(DAILY_PERIOD_MS, 100),
            candle(DAILY_PERIOD_MS * 2, 110),
        ];
        let accepted = source.ingest_fetched(&pair, bars, now);
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[1].start_time_ms, DAILY_PERIOD_MS);
    }

    #[test]
    fn align_shifts_stale_last_bar_to_yesterday_utc() {
        let now = DAILY_PERIOD_MS * 10 + 12 * 3_600_000;
        let bars = vec![
            candle(DAILY_PERIOD_MS * 7, 100),
            candle(DAILY_PERIOD_MS * 8, 110),
        ];
        let aligned = align_daily_replay_to_previous_utc_day(bars, now);
        assert_eq!(aligned[0].timestamp_ms, DAILY_PERIOD_MS * 8);
        assert_eq!(aligned[1].timestamp_ms, DAILY_PERIOD_MS * 9);
        assert_eq!(aligned[1].close, Decimal::new(110, 0));
    }

    #[test]
    fn align_is_noop_when_last_bar_is_already_yesterday() {
        let now = DAILY_PERIOD_MS * 10 + 1;
        let bars = vec![candle(DAILY_PERIOD_MS * 9, 110)];
        let aligned = align_daily_replay_to_previous_utc_day(bars.clone(), now);
        assert_eq!(aligned[0].timestamp_ms, bars[0].timestamp_ms);
    }
}
