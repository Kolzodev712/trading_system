//! Strong types for DEX strategy config, runtime, and observability.

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::types::common::Pair;

use super::runtime::DexStrategyRuntimeState;

/// Stable instance id from config (`strategies[].id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DexStrategyId(pub SmolStr);

impl DexStrategyId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DexStrategyId {
    fn from(value: &str) -> Self {
        Self(SmolStr::new(value))
    }
}

impl fmt::Display for DexStrategyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Registered DEX strategy implementation (`strategies[].type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DexStrategyKind {
    ObserveMid,
    Liquidation,
}

impl DexStrategyKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObserveMid => "ObserveMid",
            Self::Liquidation => "Liquidation",
        }
    }
}

impl fmt::Display for DexStrategyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configured market / pool label (e.g. `SUI_DBUSDC` or `SUI/DBUSDC`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DexMarket(pub SmolStr);

impl DexMarket {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches_pair(&self, pair: &Pair) -> bool {
        self.matches_label(&pair.to_string())
    }

    pub fn matches_label(&self, label: &str) -> bool {
        if self.0.as_str() == label {
            return true;
        }
        let normalized_market = self.0.replace('_', "/");
        if normalized_market == label {
            return true;
        }
        let normalized_label = label.replace('/', "_");
        self.0.as_str() == normalized_label || self.0.contains(label)
    }
}

/// Wire / TUI label for strategy runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DexRuntimeStateLabel {
    Idle,
    EvalCooldown,
    FailureBackoff,
    Disabled,
}

impl From<&DexStrategyRuntimeState> for DexRuntimeStateLabel {
    fn from(state: &DexStrategyRuntimeState) -> Self {
        match state {
            DexStrategyRuntimeState::Idle => Self::Idle,
            DexStrategyRuntimeState::EvalCooldown { .. } => Self::EvalCooldown,
            DexStrategyRuntimeState::FailureBackoff { .. } => Self::FailureBackoff,
            DexStrategyRuntimeState::Disabled => Self::Disabled,
        }
    }
}

impl fmt::Display for DexRuntimeStateLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Idle => "idle",
            Self::EvalCooldown => "eval_cooldown",
            Self::FailureBackoff => "failure_backoff",
            Self::Disabled => "disabled",
        })
    }
}

/// Non-trading strategy output (monitoring / smoke tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DexObserveSignal {
    MidPrice { mid: Decimal },
}

impl fmt::Display for DexObserveSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MidPrice { mid } => write!(f, "mid={mid}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Pair;

    #[test]
    fn market_matches_pool_name_and_pair() {
        let market = DexMarket(SmolStr::new("SUI_DBUSDC"));
        assert!(market.matches_label("SUI/DBUSDC"));
        assert!(market.matches_pair(&Pair::new("SUI", "DBUSDC")));
    }
}
