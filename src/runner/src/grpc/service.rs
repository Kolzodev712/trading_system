//! gRPC Service Implementation
use crate::grpc::trading_control::TradingControl;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

// Import the generated protobuf code
use crate::grpc::trading_cli;
use crate::grpc::trading_cli::trading_cli_service_server::TradingCliService;

/// Trading CLI Service Implementation
pub struct TradingCliServiceImpl {
    #[allow(dead_code)] // Will be used in Phase 2-5 for level management
    level_manager: Arc<RwLock<LevelManager>>,
    #[allow(dead_code)] // Will be used in Phase 2-5 for level management
    trading_state_manager: Arc<TradingStateManager>,
    trading_control: Arc<TradingControl>,
}

impl TradingCliServiceImpl {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    ) -> Self {
        let trading_control = Arc::new(TradingControl::new(
            trading_client,
            trading_state_manager.clone(),
        ));

        Self {
            level_manager,
            trading_state_manager,
            trading_control,
        }
    }
}

#[async_trait]
impl TradingCliService for TradingCliServiceImpl {
    /// Set trading enabled/disabled
    async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        let req = request.into_inner();

        match self.trading_control.set_enabled(req.enabled).await {
            Ok(cancelled_orders) => Ok(Response::new(trading_cli::SetTradingEnabledResponse {
                success: true,
                message: if req.enabled {
                    "Trading enabled".to_string()
                } else {
                    format!(
                        "Trading disabled. Cancelled {} orders.",
                        cancelled_orders.len()
                    )
                },
                cancelled_orders: cancelled_orders.iter().map(|id| id.to_string()).collect(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to set trading status: {}",
                e
            ))),
        }
    }

    /// Get trading status
    async fn get_trading_status(
        &self,
        _request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        let enabled = self.trading_control.is_enabled().await;
        Ok(Response::new(trading_cli::GetTradingStatusResponse {
            enabled,
        }))
    }

    // Placeholder implementations for Phase 2-5 RPCs
    async fn get_levels(
        &self,
        _request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn get_level(
        &self,
        _request: Request<trading_cli::GetLevelRequest>,
    ) -> Result<Response<trading_cli::GetLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn create_level(
        &self,
        _request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn update_level(
        &self,
        _request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn delete_level(
        &self,
        _request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn toggle_level(
        &self,
        _request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn get_monitoring_data(
        &self,
        _request: Request<trading_cli::GetMonitoringDataRequest>,
    ) -> Result<Response<trading_cli::GetMonitoringDataResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    type StreamMonitoringDataStream = tonic::codegen::tokio_stream::wrappers::ReceiverStream<
        Result<trading_cli::MonitoringDataUpdate, Status>,
    >;

    async fn stream_monitoring_data(
        &self,
        _request: Request<trading_cli::StreamMonitoringDataRequest>,
    ) -> Result<Response<Self::StreamMonitoringDataStream>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn get_system_health(
        &self,
        _request: Request<trading_cli::GetSystemHealthRequest>,
    ) -> Result<Response<trading_cli::GetSystemHealthResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn get_protocol_health(
        &self,
        _request: Request<trading_cli::GetProtocolHealthRequest>,
    ) -> Result<Response<trading_cli::GetProtocolHealthResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn replace_config(
        &self,
        _request: Request<trading_cli::ReplaceConfigRequest>,
    ) -> Result<Response<trading_cli::ReplaceConfigResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn validate_config(
        &self,
        _request: Request<trading_cli::ValidateConfigRequest>,
    ) -> Result<Response<trading_cli::ValidateConfigResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn preview_config_changes(
        &self,
        _request: Request<trading_cli::PreviewConfigChangesRequest>,
    ) -> Result<Response<trading_cli::PreviewConfigChangesResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }
}
