//! Unified readiness checks for `/health` (CEX-critical when `cex` or `both`).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use exchange_engine::state::manager::ExchangeStateManager;
use market_data_engine::state::manager::MarketDataStateManager;
use trading_core::types::common::Pair;
use trading_core::venue::VenueMode;

use crate::config::BotConfig;

/// How stale market data may be before readiness fails (seconds).
const MD_STALE_SECS: u64 = 120;
/// Grace period after process start before MD freshness is required.
const MD_GRACE_SECS: u64 = 45;

/// DeepBook pool pairs from bot config (empty when not DEX/both).
#[cfg(not(feature = "dex"))]
pub fn dex_pairs_from_config(_bot_config: &BotConfig) -> Vec<Pair> {
    Vec::new()
}

/// DeepBook pool pairs from bot config (empty when not DEX/both).
#[cfg(feature = "dex")]
pub fn dex_pairs_from_config(bot_config: &BotConfig) -> Vec<Pair> {
    if !matches!(bot_config.venue.mode, VenueMode::Dex | VenueMode::Both) {
        return Vec::new();
    }
    let Some(dex) = bot_config.dex_engine.as_ref() else {
        return Vec::new();
    };
    dex.deepbook_config()
        .ok()
        .map(|db| {
            db.pools
                .iter()
                .map(|p| dex_engine::pool_name_to_pair(p.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct PlatformReadiness {
    pub venue_mode: VenueMode,
    pub started_at: SystemTime,
    pub exchange_state_manager: Option<Arc<ExchangeStateManager>>,
    pub market_data_state_manager: Option<Arc<MarketDataStateManager>>,
    pub configured_pairs: Vec<Pair>,
    /// DeepBook pool pairs (for `venue.mode: both` MD freshness).
    pub dex_pairs: Vec<Pair>,
    pub dex_ready: bool,
    pub mongo_enabled: bool,
}

impl PlatformReadiness {
    pub fn is_ready(&self) -> bool {
        if self.mongo_enabled {
            // Mongo ping not implemented; presence of enabled config is sufficient for now.
        }
        let cex_critical = matches!(self.venue_mode, VenueMode::Cex | VenueMode::Both);
        if cex_critical && !self.cex_ready() {
            return false;
        }
        if matches!(self.venue_mode, VenueMode::Dex | VenueMode::Both) && !self.dex_ready {
            return false;
        }
        true
    }

    fn cex_ready(&self) -> bool {
        if !self.md_fresh_enough() {
            return false;
        }
        let Some(esm) = &self.exchange_state_manager else {
            return false;
        };
        let state = esm.get_state();
        let Ok(guard) = state.try_read() else {
            return false;
        };
        !guard.get_balances().is_empty() || !guard.get_open_orders().is_empty()
    }

    fn md_fresh_enough(&self) -> bool {
        let elapsed = self.started_at.elapsed().unwrap_or(Duration::from_secs(0));
        if elapsed < Duration::from_secs(MD_GRACE_SECS) {
            return true;
        }
        let Some(mdsm) = &self.market_data_state_manager else {
            return true;
        };
        if self.configured_pairs.is_empty() && self.dex_pairs.is_empty() {
            return true;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        let state = mdsm.get_state();
        let Ok(guard) = state.try_read() else {
            return false;
        };
        self.configured_pairs
            .iter()
            .chain(self.dex_pairs.iter())
            .any(|pair| {
                guard
                    .get_orderbook_timestamp(pair)
                    .map(|ts| orderbook_age_secs(ts, now) <= MD_STALE_SECS)
                    .unwrap_or(false)
            })
    }

    pub fn status_detail(&self) -> String {
        format!(
            "venue_mode={:?} cex_ready={} dex_ready={} md_fresh={}",
            self.venue_mode,
            self.cex_ready(),
            self.dex_ready,
            self.md_fresh_enough()
        )
    }
}

/// Age of a book timestamp vs wall clock. Paper (and Bybit) books store Unix ms;
/// values below 10^10 are treated as Unix seconds so a leftover seconds stamp
/// does not look "fresh" after dividing by 1000.
pub fn orderbook_age_secs(ts: u64, now_secs: u64) -> u64 {
    let ts_secs = if ts > 10_000_000_000 { ts / 1000 } else { ts };
    now_secs.saturating_sub(ts_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::sync::Arc;
    use trading_core::channels::market_data::market_data_update_channel;
    use trading_core::registry::symbol::SymbolRegistry;
    use trading_core::types::common::Pair;
    use trading_core::types::market_data::{OrderbookUpdateKind, StructuredOrderbook};

    fn pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn book_with_ts(ts: u64) -> StructuredOrderbook {
        StructuredOrderbook {
            pair: pair(),
            bids: vec![(Decimal::new(50000, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(50010, 0), Decimal::new(1, 0))],
            timestamp: ts,
            update_kind: OrderbookUpdateKind::Snapshot,
            sequence: None,
        }
    }

    async fn readiness_with_book(ts: u64) -> PlatformReadiness {
        let (tx, _rx) = market_data_update_channel(8);
        let md = Arc::new(MarketDataStateManager::new(
            tx,
            Arc::new(SymbolRegistry::from_pairs(vec![pair()])),
        ));
        let symbol = pair().convert_to_smolstr().into();
        md.update_orderbook(&symbol, book_with_ts(ts))
            .await
            .expect("book");
        PlatformReadiness {
            venue_mode: VenueMode::Cex,
            started_at: SystemTime::now()
                .checked_sub(Duration::from_secs(MD_GRACE_SECS + 5))
                .expect("started_at"),
            exchange_state_manager: None,
            market_data_state_manager: Some(md),
            configured_pairs: vec![pair()],
            dex_pairs: Vec::new(),
            dex_ready: false,
            mongo_enabled: false,
        }
    }

    #[tokio::test]
    async fn fresh_millisecond_book_is_md_fresh() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let ready = readiness_with_book(now_ms).await;
        assert!(
            ready.status_detail().contains("md_fresh=true"),
            "{}",
            ready.status_detail()
        );
    }

    #[tokio::test]
    async fn stale_millisecond_book_is_not_md_fresh() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let stale_ms = now_ms.saturating_sub((MD_STALE_SECS + 60) * 1000);
        let ready = readiness_with_book(stale_ms).await;
        assert!(
            ready.status_detail().contains("md_fresh=false"),
            "{}",
            ready.status_detail()
        );
    }

    #[test]
    fn seconds_stamp_is_not_divided_as_milliseconds() {
        let now_secs = 1_790_764_687;
        assert_eq!(orderbook_age_secs(now_secs, now_secs), 0);
        assert_eq!(orderbook_age_secs(now_secs * 1000, now_secs), 0);
        assert!(orderbook_age_secs(now_secs * 1000 - 180_000, now_secs) > MD_STALE_SECS);
    }
}
