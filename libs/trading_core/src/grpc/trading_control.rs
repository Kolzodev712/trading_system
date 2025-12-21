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
            enabled: Arc::new(RwLock::new(false)), // Default to disabled
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
        pairs_with_orders: Vec<crate::types::common::Pair>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut enabled_guard = self.enabled.write().await;

        tracing::info!(
            "[TradingControl] set_enabled called: enabled={}, current_state={}, pairs={}, orders={}",
            enabled,
            *enabled_guard,
            pairs_with_orders.len(),
            active_order_ids.len()
        );

        if *enabled_guard == enabled {
            tracing::info!(
                "[TradingControl] No state change needed (already {})",
                if enabled { "enabled" } else { "disabled" }
            );
            return Ok(vec![]); // No change
        }

        *enabled_guard = enabled;

        if !enabled {
            // Cancel all active orders using cancel_all_orders for each pair
            tracing::info!(
                "[TradingControl] Disabling trading, cancelling orders for {} pair(s)...",
                pairs_with_orders.len()
            );
            let cancelled_orders = self
                .cancel_all_orders_for_pairs(pairs_with_orders, active_order_ids)
                .await?;
            tracing::info!(
                "[TradingControl] Trading disabled. Cancelled {} active orders",
                cancelled_orders.len()
            );
            Ok(cancelled_orders)
        } else {
            tracing::info!("[TradingControl] Trading enabled");
            Ok(vec![])
        }
    }

    /// Cancel all orders for the given pairs using cancel_all_orders endpoint
    /// Returns the list of order IDs that were cancelled (from active_order_ids parameter)
    async fn cancel_all_orders_for_pairs(
        &self,
        pairs: Vec<crate::types::common::Pair>,
        active_order_ids: Vec<crate::types::common::OrderId>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut cancelled_order_ids = Vec::new();
        let total_pairs = pairs.len();

        tracing::info!("[TradingControl] Attempting to cancel all orders for {} pair(s) using cancel_all_orders endpoint...", total_pairs);

        for pair in pairs {
            tracing::info!(
                "[TradingControl] Cancelling all orders for {}/{}...",
                pair.base,
                pair.quote
            );

            // Clone what we need and release the lock quickly to avoid blocking
            let trading_client_arc = Arc::clone(&self.trading_client);
            let pair_clone = pair.clone();
            let cancel_result = async move {
                let trading_client = trading_client_arc.read().await;
                trading_client.cancel_all_orders(&pair_clone).await
            }
            .await;

            match cancel_result {
                Ok(_) => {
                    tracing::info!(
                        "[TradingControl] Successfully cancelled all orders for {}/{}",
                        pair.base,
                        pair.quote
                    );
                }
                Err(e) => {
                    let error_msg = format!("{}", e);
                    if error_msg.contains("Unknown order") || error_msg.contains("-2011") {
                        tracing::info!(
                            "[TradingControl] Orders for {}/{} were already cancelled",
                            pair.base,
                            pair.quote
                        );
                    } else if error_msg.contains("Timeout acquiring RPC channel")
                        || error_msg.contains("RPC channel not ready")
                        || error_msg.contains("RPC channel closed")
                        || error_msg.contains("RPC request timeout")
                    {
                        tracing::error!(
                            "[TradingControl] RPC channel unavailable when cancelling orders for {}/{}: {} - connection may be down",
                            pair.base,
                            pair.quote,
                            e
                        );
                    } else {
                        tracing::warn!(
                            "[TradingControl] Failed to cancel all orders for {}/{}: {}",
                            pair.base,
                            pair.quote,
                            e
                        );
                    }
                }
            }
        }

        // Return all active order IDs as cancelled (since we called cancel_all_orders for each pair)
        // The actual cancellation status will be confirmed by executionReports from the exchange
        for order_id in active_order_ids {
            cancelled_order_ids.push(order_id.as_str().to_string());
        }

        tracing::info!(
            "[TradingControl] Cancellation complete: {} orders cancelled across {} pair(s)",
            cancelled_order_ids.len(),
            total_pairs
        );
        Ok(cancelled_order_ids)
    }
}
