//! Trading control RPC handlers
use crate::grpc::trading_cli;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_engine::state::manager::TradingStateManager;

/// Service data needed for trading control operations
pub struct TradingControlService {
    pub trading_control: Arc<TradingControl>,
    pub trading_state_manager: Arc<TradingStateManager>,
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

        // Get active order IDs from trading state
        let active_order_ids = {
            let state_arc = self.trading_state_manager.get_state();
            let state = state_arc.read().await;
            state
                .active_orders
                .values()
                .map(|o| o.order_id.clone())
                .collect()
        };

        match self
            .trading_control
            .set_enabled(req.enabled, active_order_ids)
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
        tracing::info!("[gRPC] get_trading_status called");
        let enabled = self.trading_control.is_enabled().await;
        tracing::info!("[gRPC] get_trading_status returning: enabled={}", enabled);
        Ok(Response::new(trading_cli::GetTradingStatusResponse {
            enabled,
        }))
    }
}
