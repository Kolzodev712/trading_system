//! gRPC Service Implementation
mod converters;
mod levels;
mod monitoring;
mod trading_control;

// Re-export StrategyInfo for use in other modules
pub use monitoring::StrategyInfo;

use async_trait::async_trait;
use exchange_engine::state::manager::ExchangeStateManager;
use indicator_engine::IndicatorService;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

// Import the generated protobuf code
use crate::grpc::trading_cli;
use crate::grpc::trading_cli::trading_cli_service_server::TradingCliService;

/// Trading CLI Service Implementation
pub struct TradingCliServiceImpl {
    level_manager: Arc<RwLock<LevelManager>>,
    trading_state_manager: Arc<TradingStateManager>,
    trading_control: Arc<TradingControl>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    exchange_state_manager: Arc<ExchangeStateManager>,
    indicator_service: Arc<Mutex<IndicatorService>>,
    // Store calculator directly - it's thread-safe, no locking needed for reads
    indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    /// Strategy infos (only set in strategies mode, supports multiple)
    strategy_infos: Vec<monitoring::StrategyInfo>,
}

impl TradingCliServiceImpl {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
        indicator_service: Arc<Mutex<IndicatorService>>,
        indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
        trading_control: Arc<TradingControl>,
    ) -> Self {
        Self {
            level_manager,
            trading_state_manager,
            trading_control,
            trading_client,
            exchange_state_manager,
            indicator_service,
            indicator_calculator,
            strategy_infos: Vec::new(),
        }
    }

    /// Set strategy info (called in strategies mode)
    pub fn with_strategy_info(
        mut self,
        strategy_name: String,
        symbol: trading_core::types::common::Pair,
        max_active_orders: usize,
    ) -> Self {
        self.strategy_infos.push(monitoring::StrategyInfo {
            strategy_name,
            symbol,
            max_active_orders,
        });
        self
    }

    /// Set multiple strategy infos (called in strategies mode with multiple strategies)
    pub fn with_strategy_infos(mut self, strategy_infos: Vec<monitoring::StrategyInfo>) -> Self {
        self.strategy_infos = strategy_infos;
        self
    }

    /// Clone self for use in streaming tasks
    fn clone_for_streaming(&self) -> Self {
        Self {
            level_manager: self.level_manager.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            trading_control: self.trading_control.clone(),
            trading_client: self.trading_client.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            indicator_service: self.indicator_service.clone(),
            indicator_calculator: self.indicator_calculator.clone(),
            strategy_infos: self.strategy_infos.clone(),
        }
    }

    /// Get monitoring service helper
    fn monitoring_service(&self) -> monitoring::MonitoringService {
        monitoring::MonitoringService {
            level_manager: self.level_manager.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            indicator_calculator: self.indicator_calculator.clone(),
            strategy_infos: self.strategy_infos.clone(),
        }
    }

    /// Get level service helper
    fn level_service(&self) -> levels::LevelService {
        levels::LevelService {
            level_manager: self.level_manager.clone(),
            trading_client: self.trading_client.clone(),
        }
    }

    /// Get trading control service helper
    fn trading_control_service(&self) -> trading_control::TradingControlService {
        trading_control::TradingControlService {
            trading_control: self.trading_control.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
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
        self.trading_control_service()
            .set_trading_enabled(request)
            .await
    }

    /// Get trading status
    async fn get_trading_status(
        &self,
        request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        self.trading_control_service()
            .get_trading_status(request)
            .await
    }

    /// Get all levels
    async fn get_levels(
        &self,
        request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        self.level_service().get_levels(request).await
    }

    /// Get a single level by ID
    async fn get_level(
        &self,
        request: Request<trading_cli::GetLevelRequest>,
    ) -> Result<Response<trading_cli::GetLevelResponse>, Status> {
        self.level_service().get_level(request).await
    }

    /// Create a new level
    async fn create_level(
        &self,
        request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        self.level_service().create_level(request).await
    }

    /// Update an existing level
    async fn update_level(
        &self,
        request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        self.level_service().update_level(request).await
    }

    /// Delete a level (not yet implemented)
    async fn delete_level(
        &self,
        _request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    /// Toggle level enabled/disabled
    async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        self.level_service().toggle_level(request).await
    }

    /// Get monitoring data (one-shot)
    async fn get_monitoring_data(
        &self,
        _request: Request<trading_cli::GetMonitoringDataRequest>,
    ) -> Result<Response<trading_cli::GetMonitoringDataResponse>, Status> {
        let monitoring_data = self.monitoring_service().collect_monitoring_data().await?;

        Ok(Response::new(trading_cli::GetMonitoringDataResponse {
            data: Some(monitoring_data),
        }))
    }

    type StreamMonitoringDataStream = tonic::codegen::tokio_stream::wrappers::ReceiverStream<
        Result<trading_cli::MonitoringDataUpdate, Status>,
    >;

    /// Stream monitoring data updates
    async fn stream_monitoring_data(
        &self,
        _request: Request<trading_cli::StreamMonitoringDataRequest>,
    ) -> Result<Response<Self::StreamMonitoringDataStream>, Status> {
        tracing::info!("[gRPC] stream_monitoring_data: RPC called");
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let service = self.clone_for_streaming();

        tracing::info!("[gRPC] stream_monitoring_data: spawning streaming task");
        tokio::spawn(async move {
            // Add panic handler to catch silent failures
            std::panic::set_hook(Box::new(|panic_info| {
                tracing::error!(
                    "[gRPC] stream_monitoring_data: PANIC in streaming task: {:?}",
                    panic_info
                );
            }));

            tracing::info!("[gRPC] stream_monitoring_data: task started");
            // Use longer interval to reduce load - 1 second instead of 500ms
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            // Skip the first tick which happens immediately
            interval.tick().await;
            loop {
                interval.tick().await;

                // Collect data - no timeout needed since we access calculator directly (no lock contention)
                match service.monitoring_service().collect_monitoring_data().await {
                    Ok(data) => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;

                        let update = trading_cli::MonitoringDataUpdate {
                            data: Some(data),
                            timestamp,
                        };

                        if tx.send(Ok(update)).await.is_err() {
                            tracing::info!(
                                "[gRPC] stream_monitoring_data: receiver dropped, stopping"
                            );
                            break; // Receiver dropped
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "[gRPC] stream_monitoring_data: collect_monitoring_data failed: {}",
                            e
                        );
                        let _ = tx
                            .send(Err(Status::internal(format!(
                                "Failed to collect data: {}",
                                e
                            ))))
                            .await;
                        break;
                    }
                }
            }
            tracing::info!("[gRPC] stream_monitoring_data: task ended");
        });

        Ok(Response::new(
            tonic::codegen::tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }
}
