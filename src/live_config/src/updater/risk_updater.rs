//! Risk updater - updates risk limits in RiskManager
use crate::types::RiskLimitType;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_engine::risk::RiskManager;

pub struct RiskUpdater {
    pub(crate) risk_manager: Arc<RwLock<RiskManager>>,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Invalid limit type: {0}")]
    InvalidLimitType(String),

    #[error("Update failed: {0}")]
    UpdateFailed(String),
}

impl RiskUpdater {
    pub fn new(risk_manager: Arc<RwLock<RiskManager>>) -> Self {
        Self { risk_manager }
    }

    /// Update a risk limit
    pub async fn update_risk_limit(
        &self,
        limit_type: RiskLimitType,
        value: Decimal,
    ) -> Result<(), UpdateError> {
        // Get current limits and update before acquiring write lock
        let mut limits = {
            let risk_manager = self.risk_manager.read().await;
            risk_manager.get_limits().clone()
        };

        // Update the specific limit (no lock held during computation)
        match limit_type {
            RiskLimitType::TotalMaxPosition => {
                limits.total_max_position = Some(value);
            }
            RiskLimitType::MaxTotalOrders => {
                // Convert Decimal to usize (assuming value is a whole number)
                let order_count = value.to_string().parse::<usize>().map_err(|_| {
                    UpdateError::UpdateFailed(
                        "Invalid order count - must be a whole number".to_string(),
                    )
                })?;
                limits.max_total_orders = Some(order_count);
            }
            RiskLimitType::DailyLossLimitAllPairs => {
                limits.daily_loss_limit_all_pairs = Some(value);
            }
            RiskLimitType::DailyLossLimitPerPair => {
                // This requires a pair parameter - would need to extend the request type
                return Err(UpdateError::InvalidLimitType(
                    "DailyLossLimitPerPair requires pair specification".to_string(),
                ));
            }
        }

        // Update the risk manager with new limits (acquire write lock only for update)
        {
            let mut risk_manager = self.risk_manager.write().await;
            risk_manager.update_limits(limits);
        }

        Ok(())
    }

    /// Get current risk limits (for persistence)
    pub async fn get_limits(&self) -> trading_engine::risk::RiskLimits {
        let risk_manager = self.risk_manager.read().await;
        risk_manager.get_limits().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use trading_core::registry::symbol::SymbolRegistry;
    use trading_engine::risk::{RiskLimits, RiskManager};

    fn create_test_risk_manager() -> RiskManager {
        let limits = RiskLimits::default();
        let symbol_registry = Arc::new(SymbolRegistry::new());
        RiskManager::new(limits, symbol_registry)
    }

    #[tokio::test]
    async fn test_risk_updater_new() {
        let risk_manager = Arc::new(RwLock::new(create_test_risk_manager()));
        let _updater = RiskUpdater::new(risk_manager);
        // Should not panic - test passes if we get here
    }

    #[tokio::test]
    async fn test_risk_updater_update_total_max_position() {
        let risk_manager = Arc::new(RwLock::new(create_test_risk_manager()));
        let updater = RiskUpdater::new(risk_manager.clone());

        let result = updater
            .update_risk_limit(RiskLimitType::TotalMaxPosition, dec!(5000))
            .await;
        assert!(result.is_ok());

        // Verify update
        let rm = risk_manager.read().await;
        assert_eq!(rm.get_limits().total_max_position, Some(dec!(5000)));
    }

    #[tokio::test]
    async fn test_risk_updater_update_max_total_orders() {
        let risk_manager = Arc::new(RwLock::new(create_test_risk_manager()));
        let updater = RiskUpdater::new(risk_manager.clone());

        let result = updater
            .update_risk_limit(RiskLimitType::MaxTotalOrders, dec!(100))
            .await;
        assert!(result.is_ok());

        // Verify update
        let rm = risk_manager.read().await;
        assert_eq!(rm.get_limits().max_total_orders, Some(100));
    }

    #[tokio::test]
    async fn test_risk_updater_update_daily_loss_limit_all_pairs() {
        let risk_manager = Arc::new(RwLock::new(create_test_risk_manager()));
        let updater = RiskUpdater::new(risk_manager.clone());

        let result = updater
            .update_risk_limit(RiskLimitType::DailyLossLimitAllPairs, dec!(10000))
            .await;
        assert!(result.is_ok());

        // Verify update
        let rm = risk_manager.read().await;
        assert_eq!(
            rm.get_limits().daily_loss_limit_all_pairs,
            Some(dec!(10000))
        );
    }

    #[tokio::test]
    async fn test_risk_updater_update_daily_loss_limit_per_pair() {
        let risk_manager = Arc::new(RwLock::new(create_test_risk_manager()));
        let updater = RiskUpdater::new(risk_manager);

        // This should fail as it requires pair specification
        let result = updater
            .update_risk_limit(RiskLimitType::DailyLossLimitPerPair, dec!(5000))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_risk_updater_get_limits() {
        let risk_manager = Arc::new(RwLock::new(create_test_risk_manager()));
        let updater = RiskUpdater::new(risk_manager.clone());

        let limits = updater.get_limits().await;
        // Should return default limits
        assert!(limits.total_max_position.is_none());
    }
}
