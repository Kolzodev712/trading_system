//! Trading Control Service - Manages trading enable/disable state
//! This is a general service that can be used by any component that needs to control trading

use crate::traits::TradingClient;
use crate::types::exchange::OrderRequest;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trading Control Service
/// Manages whether trading is enabled or disabled
/// When disabled, prevents new orders and cancels all active orders
pub struct TradingControl {
    enabled: Arc<RwLock<bool>>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
}

impl TradingControl {
    /// Create a new TradingControl service
    pub fn new(trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>) -> Self {
        Self {
            enabled: Arc::new(RwLock::new(true)), // Default to enabled
            trading_client,
        }
    }

    /// Check if trading is enabled
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Get a reference to the enabled flag (for executor checks)
    pub fn enabled_flag(&self) -> Arc<RwLock<bool>> {
        self.enabled.clone()
    }

    /// Set trading enabled/disabled
    /// Returns the list of cancelled order IDs if disabling
    pub async fn set_enabled(
        &self,
        enabled: bool,
        active_order_ids: Vec<crate::types::common::OrderId>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut enabled_guard = self.enabled.write().await;

        if *enabled_guard == enabled {
            return Ok(vec![]); // No change
        }

        *enabled_guard = enabled;

        if !enabled {
            // Cancel all active orders
            let cancelled_orders = self.cancel_orders(active_order_ids).await?;
            tracing::info!(
                "Trading disabled. Cancelled {} active orders",
                cancelled_orders.len()
            );
            Ok(cancelled_orders)
        } else {
            tracing::info!("Trading enabled");
            Ok(vec![])
        }
    }

    /// Cancel a list of orders
    async fn cancel_orders(
        &self,
        order_ids: Vec<crate::types::common::OrderId>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut cancelled_order_ids = Vec::new();
        let client = self.trading_client.write().await;

        for order_id in order_ids {
            let order_id_str = order_id.as_str().to_string();
            match client.cancel_order_by_id(order_id.clone()).await {
                Ok(_) => {
                    cancelled_order_ids.push(order_id_str.clone());
                    tracing::info!("Cancelled order: {}", order_id_str);
                }
                Err(e) => {
                    tracing::warn!("Failed to cancel order {}: {}", order_id_str, e);
                }
            }
        }

        Ok(cancelled_order_ids)
    }
}
