//! Unified readiness checks for `/health` (CEX-critical when `cex` or `both`).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use exchange_engine::state::manager::ExchangeStateManager;
use market_data_engine::state::manager::MarketDataStateManager;
use trading_core::types::common::Pair;
use trading_core::venue::VenueMode;

/// How stale market data may be before readiness fails (seconds).
const MD_STALE_SECS: u64 = 120;
/// Grace period after process start before MD freshness is required.
const MD_GRACE_SECS: u64 = 45;

#[derive(Debug, Clone)]
pub struct PlatformReadiness {
    pub venue_mode: VenueMode,
    pub started_at: SystemTime,
    pub exchange_state_manager: Option<Arc<ExchangeStateManager>>,
    pub market_data_state_manager: Option<Arc<MarketDataStateManager>>,
    pub configured_pairs: Vec<Pair>,
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
        if self.configured_pairs.is_empty() {
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
        self.configured_pairs.iter().any(|pair| {
            guard
                .get_orderbook_timestamp(pair)
                .map(|ts| now.saturating_sub(ts / 1000) <= MD_STALE_SECS)
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
