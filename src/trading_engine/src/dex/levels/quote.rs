//! Runtime lifecycle for a single DEX quote.

use smol_str::SmolStr;
use std::time::Instant;

use super::config::DexQuoteConfig;

/// Placement state for one configured quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DexQuoteState {
    /// Not on book; eligible for initial placement.
    Idle,
    /// Tx submitted; awaiting on-chain confirmation.
    PendingTx {
        signature: SmolStr,
    },
    /// Live on DeepBook (indexer order id).
    Placed {
        order_id: SmolStr,
    },
    /// Waiting before repost after fill/cancel.
    RepostScheduled {
        after: Instant,
    },
    Disabled,
}

#[derive(Debug, Clone)]
pub struct DexQuoteRuntime {
    pub config: DexQuoteConfig,
    pub state: DexQuoteState,
}

impl DexQuoteRuntime {
    pub fn new(config: DexQuoteConfig) -> Self {
        let state = if config.enabled {
            DexQuoteState::Idle
        } else {
            DexQuoteState::Disabled
        };
        Self { config, state }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && !matches!(self.state, DexQuoteState::Disabled)
    }

    pub fn needs_placement(&self, now: Instant) -> bool {
        match &self.state {
            DexQuoteState::Idle => self.config.enabled,
            DexQuoteState::RepostScheduled { after } => self.config.enabled && now >= *after,
            _ => false,
        }
    }
}
