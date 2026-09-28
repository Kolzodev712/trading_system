//! Risk limit definitions
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Global risk limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct RiskLimits {
    /// Maximum total position size across all levels
    pub total_max_position: Option<Decimal>,

    /// Maximum total number of orders
    pub max_total_orders: Option<usize>,

    /// Maximum notional of a single order (price * qty).
    pub max_notional: Option<Decimal>,

    /// Maximum unsigned quantity of a single order.
    pub max_order_qty: Option<Decimal>,

    /// Daily loss limit for all pairs
    pub daily_loss_limit_all_pairs: Option<Decimal>,

    /// Daily loss limit per pair
    pub daily_loss_limit_per_pair: std::collections::HashMap<SmolStr, Decimal>,
}

impl RiskLimits {
    /// True when at least one production size/count limit is set.
    /// Daily-loss is production safety only when the execution ledger supplies PnL.
    pub fn is_explicit(&self) -> bool {
        self.total_max_position.is_some()
            || self.max_total_orders.is_some()
            || self.max_notional.is_some()
            || self.max_order_qty.is_some()
    }

    /// Exchange mode must name a size/count limit. Shadow may omit it.
    pub fn require_explicit_for_exchange(
        &self,
        execution_mode: trading_core::types::ExecutionMode,
    ) -> Result<(), String> {
        if execution_mode == trading_core::types::ExecutionMode::Exchange && !self.is_explicit() {
            return Err(
                "Exchange mode requires explicit risk_limits (total_max_position, max_total_orders, max_notional, or max_order_qty)"
                    .into(),
            );
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_limits_default() {
        let limits = RiskLimits::default();
        assert!(limits.total_max_position.is_none());
        assert!(limits.max_total_orders.is_none());
        assert!(limits.daily_loss_limit_all_pairs.is_none());
        assert!(limits.daily_loss_limit_per_pair.is_empty());
        assert!(!limits.is_explicit());
    }

    #[test]
    fn exchange_requires_explicit_size_limit() {
        let empty = RiskLimits::default();
        assert!(empty
            .require_explicit_for_exchange(trading_core::types::ExecutionMode::Exchange)
            .is_err());
        assert!(empty
            .require_explicit_for_exchange(trading_core::types::ExecutionMode::Shadow)
            .is_ok());
        let limited = RiskLimits {
            max_order_qty: Some(Decimal::ONE),
            ..RiskLimits::default()
        };
        assert!(limited
            .require_explicit_for_exchange(trading_core::types::ExecutionMode::Exchange)
            .is_ok());
    }

    #[test]
    fn test_risk_check_result_allowed() {
        let result = RiskCheckResult::allowed();
        assert!(result.allowed);
        assert!(result.reason.is_none());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_risk_check_result_blocked() {
        let reason = SmolStr::new("Test reason");
        let result = RiskCheckResult::blocked(reason.clone());
        assert!(!result.allowed);
        assert_eq!(result.reason, Some(reason));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_risk_check_result_with_warning() {
        let result = RiskCheckResult::allowed()
            .with_warning(SmolStr::new("Warning 1"))
            .with_warning(SmolStr::new("Warning 2"));

        assert!(result.allowed);
        assert_eq!(result.warnings.len(), 2);
    }
}
