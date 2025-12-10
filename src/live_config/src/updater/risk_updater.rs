//! Risk updater - updates risk limits in RiskManager
use crate::types::RiskLimitType;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_engine::levels::RiskManager;

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
        let mut risk_manager = self.risk_manager.write().await;

        // Get current limits
        let mut limits = risk_manager.get_limits().clone();

        // Update the specific limit
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

        // Update the risk manager with new limits
        risk_manager.update_limits(limits.clone());

        Ok(())
    }

    /// Get current risk limits (for persistence)
    pub async fn get_limits(&self) -> trading_engine::levels::risk::RiskLimits {
        let risk_manager = self.risk_manager.read().await;
        risk_manager.get_limits().clone()
    }
}
