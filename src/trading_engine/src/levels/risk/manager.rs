//! Risk manager - orchestrates all risk checks before order placement
use super::limits::{RiskCheckResult, RiskLimits};
use crate::levels::ResolvedLevel;
use crate::state::TradingState;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use trading_core::types::exchange::Balance;

/// Risk manager - orchestrates all risk checks
pub struct RiskManager {
    limits: RiskLimits,
}

impl RiskManager {
    /// Create a new risk manager
    pub fn new(limits: RiskLimits) -> Self {
        Self { limits }
    }

    /// Get current risk limits
    pub fn get_limits(&self) -> &RiskLimits {
        &self.limits
    }

    /// Update risk limits
    pub fn update_limits(&mut self, limits: RiskLimits) {
        self.limits = limits;
    }

    /// Perform all risk checks before posting a level
    pub async fn check_level(
        &self,
        level: &ResolvedLevel,
        current_state: &TradingState,
        balances: &[Balance],
    ) -> RiskCheckResult {
        // Check position limits for this level
        let position_check = self.check_position_limits(level, current_state);
        if !position_check.allowed {
            return position_check;
        }

        // Check balance
        let balance_check = self.check_balance(level, balances);
        if !balance_check.allowed {
            return balance_check;
        }

        // Check total position limits
        let total_position_check = self.check_total_position_limits(level, current_state);
        if !total_position_check.allowed {
            return total_position_check;
        }

        // Check total order count
        let order_count_check = self.check_order_count(level, current_state);
        if !order_count_check.allowed {
            return order_count_check;
        }

        // All checks passed
        RiskCheckResult::allowed()
    }

    /// Check position limits for this level
    fn check_position_limits(
        &self,
        level: &ResolvedLevel,
        state: &TradingState,
    ) -> RiskCheckResult {
        if let Some(max_position) = level.level.config.risk.max_position {
            let current_position = state
                .get_position(&level.level.config.symbol)
                .map(|p| p.quantity.abs())
                .unwrap_or(Decimal::ZERO);

            let new_position = if level.level.config.side == trading_core::types::common::Side::Buy
            {
                current_position + level.resolved_quantity
            } else {
                current_position.saturating_sub(level.resolved_quantity)
            };

            if new_position > max_position {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Position limit exceeded: {} > {}",
                    new_position, max_position
                )));
            }
        }

        RiskCheckResult::allowed()
    }

    /// Check balance
    fn check_balance(&self, level: &ResolvedLevel, balances: &[Balance]) -> RiskCheckResult {
        match level.level.config.side {
            trading_core::types::common::Side::Buy => {
                // Need quote currency balance
                let required = level.resolved_price * level.resolved_quantity;
                let available = balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == level.level.config.symbol.quote.as_str())
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                if available < required {
                    return RiskCheckResult::blocked(SmolStr::new(format!(
                        "Insufficient balance: need {} {}, have {}",
                        required, level.level.config.symbol.quote, available
                    )));
                }
            }
            trading_core::types::common::Side::Sell => {
                // Need base currency balance
                let required = level.resolved_quantity;
                let available = balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == level.level.config.symbol.base.as_str())
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                if available < required {
                    return RiskCheckResult::blocked(SmolStr::new(format!(
                        "Insufficient balance: need {} {}, have {}",
                        required, level.level.config.symbol.base, available
                    )));
                }
            }
        }

        RiskCheckResult::allowed()
    }

    /// Check total position limits
    fn check_total_position_limits(
        &self,
        _level: &ResolvedLevel,
        _state: &TradingState,
    ) -> RiskCheckResult {
        // TODO: Implement total position calculation across all symbols
        // For now, just allow
        if let Some(_max_total_position) = self.limits.total_max_position {
            // This would require tracking total position across all pairs
            // Simplified for now - can be enhanced later
        }
        RiskCheckResult::allowed()
    }

    /// Check order count limits
    fn check_order_count(&self, level: &ResolvedLevel, state: &TradingState) -> RiskCheckResult {
        // Check per-level order count
        if let Some(max_orders) = level.level.config.risk.max_orders {
            let active_orders = level.level.active_orders.len();
            if active_orders >= max_orders {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Level order limit reached: {} >= {}",
                    active_orders, max_orders
                )));
            }
        }

        // Check total order count
        if let Some(max_total_orders) = self.limits.max_total_orders {
            let total_active_orders = state.get_active_order_count(&level.level.config.symbol);
            if total_active_orders >= max_total_orders {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Total order limit reached: {} >= {}",
                    total_active_orders, max_total_orders
                )));
            }
        }

        RiskCheckResult::allowed()
    }
}
