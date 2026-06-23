//! Per-strategy runtime state (separate from CEX strategy state).

use std::time::Instant;

use super::config::DexStrategyEntryConfig;
use super::types::DexObserveSignal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DexStrategyRuntimeState {
    Idle,
    /// Skipping evaluation until cooldown elapses.
    EvalCooldown {
        after: Instant,
    },
    /// Backing off after a failed on-chain execution.
    FailureBackoff {
        after: Instant,
    },
    Disabled,
}

#[derive(Debug, Clone)]
pub struct DexStrategyRuntime {
    pub config: DexStrategyEntryConfig,
    pub state: DexStrategyRuntimeState,
    pub last_signal: Option<DexObserveSignal>,
    pub eval_count: u64,
    pub last_eval_at: Option<Instant>,
}

impl DexStrategyRuntime {
    pub fn new(config: DexStrategyEntryConfig) -> Self {
        let state = if config.enabled {
            DexStrategyRuntimeState::Idle
        } else {
            DexStrategyRuntimeState::Disabled
        };
        Self {
            config,
            state,
            last_signal: None,
            eval_count: 0,
            last_eval_at: None,
        }
    }

    pub fn can_evaluate(&self, now: Instant) -> bool {
        match &self.state {
            DexStrategyRuntimeState::Idle => self.config.enabled,
            DexStrategyRuntimeState::EvalCooldown { after } => self.config.enabled && now >= *after,
            DexStrategyRuntimeState::FailureBackoff { after } => {
                self.config.enabled && now >= *after
            }
            DexStrategyRuntimeState::Disabled => false,
        }
    }
}
