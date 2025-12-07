//! Risk limit definitions
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Global risk limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskLimits {
    /// Maximum total position size across all levels
    pub total_max_position: Option<Decimal>,

    /// Maximum total number of orders
    pub max_total_orders: Option<usize>,

    /// Daily loss limit for all pairs
    pub daily_loss_limit_all_pairs: Option<Decimal>,

    /// Daily loss limit per pair
    pub daily_loss_limit_per_pair: std::collections::HashMap<SmolStr, Decimal>,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            total_max_position: None,
            max_total_orders: None,
            daily_loss_limit_all_pairs: None,
            daily_loss_limit_per_pair: std::collections::HashMap::new(),
        }
    }
}

/// Risk check result
#[derive(Debug, Clone, PartialEq)]
pub struct RiskCheckResult {
    pub allowed: bool,
    pub reason: Option<SmolStr>,
    pub warnings: Vec<SmolStr>,
}

impl RiskCheckResult {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
            warnings: Vec::new(),
        }
    }

    pub fn blocked(reason: SmolStr) -> Self {
        Self {
            allowed: false,
            reason: Some(reason),
            warnings: Vec::new(),
        }
    }

    pub fn with_warning(mut self, warning: SmolStr) -> Self {
        self.warnings.push(warning);
        self
    }
}
