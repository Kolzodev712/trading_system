//! Trading Control Service - Manages trading enable/disable state
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::state::manager::TradingStateManager;

/// Trading Control Service
/// Manages whether trading is enabled or disabled
/// When disabled, prevents new orders and cancels all active orders
pub struct TradingControl {
    enabled: Arc<RwLock<bool>>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    trading_state_manager: Arc<TradingStateManager>,
}

impl TradingControl {
    /// Create a new TradingControl service
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        trading_state_manager: Arc<TradingStateManager>,
    ) -> Self {
        Self {
            enabled: Arc::new(RwLock::new(true)), // Default to enabled
            trading_client,
            trading_state_manager,
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
    /// When disabling, cancels all active orders
    pub async fn set_enabled(
        &self,
        enabled: bool,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut enabled_guard = self.enabled.write().await;

        if *enabled_guard == enabled {
            return Ok(vec![]); // No change
        }

        *enabled_guard = enabled;

        if !enabled {
            // Cancel all active orders
            let cancelled_orders = self.cancel_all_orders().await?;
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

    /// Cancel all active orders
    async fn cancel_all_orders(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let state_arc = self.trading_state_manager.get_state();
        let state = state_arc.read().await;
        let active_orders: Vec<_> = state.active_orders.values().collect();
        let order_ids: Vec<_> = active_orders.iter().map(|o| o.order_id.clone()).collect();
        drop(state); // Release the lock before awaiting cancel operations

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
