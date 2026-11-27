//! Trading Control Service - Manages trading enable/disable state
//! This is a general service that can be used by any component that needs to control trading

use crate::traits::TradingClient;
use crate::types::exchange::OrderRequest;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Trading Control Service
/// Manages whether trading is enabled or disabled
/// When disabled, prevents new orders and cancels all active orders
pub struct TradingControl {
    enabled: Arc<AtomicBool>,
    /// PauseEntries: evaluation continues; exposure increases stop.
    entries_paused: Arc<AtomicBool>,
    /// FlattenAndPause latch: strategy target cannot reopen until Resume.
    flatten_latched: Arc<AtomicBool>,
    /// Serialises enable/disable transitions; flag *reads* stay lock-free.
    set_enabled_lock: Mutex<()>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
}

impl TradingControl {
    /// Create a new TradingControl service
    pub fn new(trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)), // Default to disabled
            entries_paused: Arc::new(AtomicBool::new(false)),
            flatten_latched: Arc::new(AtomicBool::new(false)),
            set_enabled_lock: Mutex::new(()),
            trading_client,
        }
    }

    /// Start PauseEntries without an RPC. Non-Paper venues use this so demo
    /// cannot enter until an authenticated Resume.
    pub fn with_entries_paused(self, paused: bool) -> Self {
        Self {
            entries_paused: Arc::new(AtomicBool::new(paused)),
            ..self
        }
    }

    /// Check if trading is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Get a reference to the enabled flag (for executor checks)
    pub fn enabled_flag(&self) -> Arc<AtomicBool> {
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
        // Serialize transitions so concurrent enable/disable cannot interleave
        // cancel-all with a re-enable. Flag *readers* still use AtomicBool only.
        let _guard = self.set_enabled_lock.lock().await;

        // Publish the new state before any cancel-all work so concurrent order
        // submission observes `enabled == false` immediately.
        let previous = self.enabled.swap(enabled, Ordering::AcqRel);

        tracing::info!(
            "[TradingControl] set_enabled called: enabled={}, current_state={}, pairs={}, orders={}",
            enabled,
            previous,
            pairs_with_orders.len(),
            active_order_ids.len()
        );

        if previous == enabled {
            tracing::info!(
                "[TradingControl] No state change needed (already {})",
                if enabled { "enabled" } else { "disabled" }
            );
            return Ok(vec![]); // No change
        }

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
                        continue;
                    }
                    tracing::error!(
                        "[TradingControl] CancelWorking unverified for {}/{}: {}",
                        pair.base,
                        pair.quote,
                        e
                    );
                    return Err(format!("CancelWorking unverified: {e}").into());
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

    pub fn entries_paused_flag(&self) -> Arc<AtomicBool> {
        self.entries_paused.clone()
    }

    pub fn flatten_latched_flag(&self) -> Arc<AtomicBool> {
        self.flatten_latched.clone()
    }

    pub fn entries_paused(&self) -> bool {
        self.entries_paused.load(Ordering::Acquire)
    }

    pub fn flatten_latched(&self) -> bool {
        self.flatten_latched.load(Ordering::Acquire)
    }

    /// PauseEntries: stop increases; do not flatten; do not change the legacy global toggle.
    pub fn pause_entries(&self) {
        self.entries_paused.store(true, Ordering::Release);
    }

    /// Resume clears PauseEntries and the flatten latch.
    pub fn resume_entries(&self) {
        self.entries_paused.store(false, Ordering::Release);
        self.flatten_latched.store(false, Ordering::Release);
    }

    /// FlattenAndPause latch: target zero until Resume.
    pub fn latch_flatten_and_pause(&self) {
        // Pause first, then latch: a reader that sees the latch also sees the pause.
        self.entries_paused.store(true, Ordering::Release);
        self.flatten_latched.store(true, Ordering::Release);
    }

    pub fn trading_client(&self) -> Arc<RwLock<dyn TradingClient<OrderRequest>>> {
        self.trading_client.clone()
    }

    /// Cancel working orders without changing the legacy global toggle.
    pub async fn cancel_working(
        &self,
        active_order_ids: Vec<crate::types::common::OrderId>,
        pairs_with_orders: Vec<crate::types::common::Pair>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.cancel_all_orders_for_pairs(pairs_with_orders, active_order_ids)
            .await
    }
}

/// PauseEntries blocks increases. FlattenAndPause allows reductions even when the
/// legacy global toggle is off, and blocks strategy reopen until Resume.
pub fn operator_allows_submit(
    increasing: bool,
    trading_enabled: bool,
    entries_paused: bool,
    flatten_latched: bool,
) -> bool {
    if flatten_latched {
        return !increasing;
    }
    if !trading_enabled {
        return false;
    }
    if entries_paused && increasing {
        return false;
    }
    true
}

pub const GLOBAL_TOGGLE_SEMANTICS: &str =
    "legacy SetTradingEnabled: disabling cancels working orders; not PauseEntries/FlattenAndPause";

#[cfg(test)]
mod tests {
    use super::operator_allows_submit;
    use super::TradingControl;
    use crate::error::EngineError;
    use crate::types::common::{Exchange, OrderId, Pair};
    use crate::types::exchange::{Balance, Order, OrderRequest, Trade};
    use crate::venue::cex::traits::TradingClient;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn pause_blocks_increase_allows_reduce() {
        assert!(!operator_allows_submit(true, true, true, false));
        assert!(operator_allows_submit(false, true, true, false));
    }

    #[test]
    fn flatten_latch_allows_reduce_when_global_off() {
        assert!(operator_allows_submit(false, false, true, true));
        assert!(!operator_allows_submit(true, false, true, true));
        assert!(!operator_allows_submit(true, true, true, true));
    }

    #[test]
    fn resume_requires_global_enable_for_increases() {
        assert!(!operator_allows_submit(true, false, false, false));
        assert!(operator_allows_submit(true, true, false, false));
    }

    #[derive(Debug)]
    struct UnverifiedCancelClient;

    #[async_trait]
    impl TradingClient<OrderRequest> for UnverifiedCancelClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Err(EngineError::CustomError(smol_str::format_smolstr!(
                "CancelWorking unverified: REST GET /open_orders failed"
            )))
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Err(EngineError::CustomError(smol_str::format_smolstr!(
                "REST GET /open_orders: 503"
            )))
        }

        async fn fetch_trades(&self, _pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }

        async fn amend_order(
            &self,
            _order_id: OrderId,
            _new_request: OrderRequest,
        ) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_open_orders(
            &self,
            _req: crate::venue::cex::types::exchange::BulkOrderRequest,
        ) -> Result<crate::venue::cex::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_amend_orders(
            &self,
            _req: crate::venue::cex::types::exchange::BulkAmendRequest,
        ) -> Result<crate::venue::cex::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    #[tokio::test]
    async fn cancel_working_fails_when_venue_rest_is_unverified() {
        let control = TradingControl::new(Arc::new(RwLock::new(UnverifiedCancelClient)));
        let err = control
            .cancel_working(vec![], vec![Pair::new("BTC", "USDT")])
            .await
            .expect_err("CancelWorking must not report success without a venue check");
        assert!(
            err.to_string().contains("unverified"),
            "unexpected error: {err}"
        );
    }
}
