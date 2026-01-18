//! Trading control RPC handlers
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

/// Service data needed for trading control operations
pub struct TradingControlService {
    pub trading_control: Arc<TradingControl>,
    pub trading_state_manager: Arc<TradingStateManager>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
    pub level_manager: Arc<RwLock<LevelManager>>,
}

impl TradingControlService {
    /// Set trading enabled/disabled
    pub async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "[gRPC] set_trading_enabled called with enabled={}",
            req.enabled
        );

        // Get ALL active order IDs from multiple sources:
        // 1. TradingStateManager (orders we placed)
        // 2. ExchangeStateManager (all open orders on exchange)
        // 3. LevelManager (orders tracked by levels)
        // Use a HashSet to deduplicate
        let mut active_order_ids_set = HashSet::new();
        let mut pairs_with_orders_set = HashSet::new();

        // Get orders from TradingStateManager
        let mut trading_state_count = 0;
        {
            let state_arc = self.trading_state_manager.get_state();
            let state = state_arc.read().await;
            for order in state.active_orders.values() {
                active_order_ids_set.insert(order.order_id.clone());
                trading_state_count += 1;
            }
        }

        // Get orders and pairs from ExchangeStateManager (all open orders on exchange)
        // This is the authoritative source for pairs with orders
        let mut exchange_state_count = 0;
        {
            let exchange_state_arc = self.exchange_state_manager.get_state();
            let exchange_state = exchange_state_arc.read().await;
            for order in exchange_state.get_open_orders() {
                active_order_ids_set.insert(order.order_id.clone());
                // Extract pair from symbol using symbol registry
                if let Some(pair) = exchange_state
                    .symbol_registry()
                    .get_pair(&order.instrument.symbol().into())
                {
                    pairs_with_orders_set.insert(pair.clone());
                }
                exchange_state_count += 1;
            }
        }

        // Get orders from LevelManager (orders tracked by levels)
        // Also add pairs from levels (in case they have orders but ExchangeStateManager doesn't have them yet)
        let mut level_manager_count = 0;
        {
            let level_manager = self.level_manager.read().await;
            for level in level_manager.get_all_levels() {
                for order_id in &level.active_orders {
                    active_order_ids_set.insert(order_id.clone());
                    pairs_with_orders_set.insert(level.pair.clone());
                }
                level_manager_count += 1;
            }
        }

        tracing::info!(
            "[gRPC] Collected {} unique order IDs and {} unique pairs for cancellation (TradingState: {}, ExchangeState: {}, LevelManager: {})",
            active_order_ids_set.len(),
            pairs_with_orders_set.len(),
            trading_state_count,
            exchange_state_count,
            level_manager_count
        );

        let active_order_ids: Vec<_> = active_order_ids_set.into_iter().collect();
        let pairs_with_orders: Vec<_> = pairs_with_orders_set.into_iter().collect();

        match self
            .trading_control
            .set_enabled(req.enabled, active_order_ids, pairs_with_orders)
            .await
        {
            Ok(cancelled_orders) => {
                tracing::info!(
                    "[gRPC] set_trading_enabled succeeded, cancelled {} orders",
                    cancelled_orders.len()
                );
                Ok(Response::new(trading_cli::SetTradingEnabledResponse {
                    success: true,
                    message: if req.enabled {
                        "Trading enabled".to_string()
                    } else {
                        format!(
                            "Trading disabled. Cancelled {} orders.",
                            cancelled_orders.len()
                        )
                    },
                    cancelled_orders,
                }))
            }
            Err(e) => {
                tracing::error!("[gRPC] set_trading_enabled failed: {}", e);
                Err(Status::internal(format!(
                    "Failed to set trading status: {}",
                    e
                )))
            }
        }
    }

    /// Get trading status
    pub async fn get_trading_status(
        &self,
        _request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        let enabled = self.trading_control.is_enabled().await;
        Ok(Response::new(trading_cli::GetTradingStatusResponse {
            enabled,
        }))
    }
}
