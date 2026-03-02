//! gRPC Service Implementation
mod converters;
mod levels;
mod monitoring;
mod trading_control;

pub use monitoring::{MonitoringService, StrategyInfo};

use async_trait::async_trait;
use exchange_engine::state::manager::ExchangeStateManager;
use indicator_engine::IndicatorService;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;
use trading_engine::strategies::strategy_manager::StrategyManager;

use crate::grpc::trading_cli;
use crate::grpc::trading_cli::trading_cli_service_server::TradingCliService;

pub struct TradingCliServiceImpl {
    level_manager: Arc<RwLock<LevelManager>>,
    trading_state_manager: Arc<TradingStateManager>,
    trading_control: Arc<TradingControl>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    exchange_state_manager: Arc<ExchangeStateManager>,
    market_data_state_manager: Arc<market_data_engine::state::manager::MarketDataStateManager>,
    indicator_service: Arc<Mutex<IndicatorService>>,
    indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    strategy_infos: Vec<monitoring::StrategyInfo>,
    strategy_manager: Option<Arc<RwLock<StrategyManager>>>,
    instrument_type: trading_core::types::common::InstrumentType,
    exchange: String,
}

impl TradingCliServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
        market_data_state_manager: Arc<market_data_engine::state::manager::MarketDataStateManager>,
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
            market_data_state_manager,
            indicator_service,
            indicator_calculator,
            strategy_infos: Vec::new(),
            strategy_manager: None,
            instrument_type: trading_core::types::common::InstrumentType::Spot,
            exchange: "Unknown".to_string(),
        }
    }

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

    pub fn with_strategy_infos(mut self, strategy_infos: Vec<monitoring::StrategyInfo>) -> Self {
        self.strategy_infos = strategy_infos;
        self
    }

    pub fn with_strategy_manager(mut self, strategy_manager: Arc<RwLock<StrategyManager>>) -> Self {
        self.strategy_manager = Some(strategy_manager);
        self
    }

    pub fn with_instrument_type(
        mut self,
        instrument_type: trading_core::types::common::InstrumentType,
    ) -> Self {
        self.instrument_type = instrument_type;
        self
    }

    pub fn with_exchange(mut self, exchange: impl Into<String>) -> Self {
        self.exchange = exchange.into();
        self
    }

    fn clone_for_streaming(&self) -> Self {
        Self {
            level_manager: self.level_manager.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            trading_control: self.trading_control.clone(),
            trading_client: self.trading_client.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            market_data_state_manager: self.market_data_state_manager.clone(),
            indicator_service: self.indicator_service.clone(),
            indicator_calculator: self.indicator_calculator.clone(),
            strategy_infos: self.strategy_infos.clone(),
            strategy_manager: self.strategy_manager.clone(),
            instrument_type: self.instrument_type.clone(),
            exchange: self.exchange.clone(),
        }
    }

    fn monitoring_service(&self) -> monitoring::MonitoringService {
        monitoring::MonitoringService {
            level_manager: self.level_manager.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            market_data_state_manager: self.market_data_state_manager.clone(),
            indicator_calculator: self.indicator_calculator.clone(),
            strategy_infos: self.strategy_infos.clone(),
            strategy_manager: self.strategy_manager.clone(),
            instrument_type: self.instrument_type.clone(),
            exchange: self.exchange.clone(),
        }
    }

    fn level_service(&self) -> levels::LevelService {
        levels::LevelService {
            level_manager: self.level_manager.clone(),
            trading_client: self.trading_client.clone(),
            trading_control: self.trading_control.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
        }
    }

    fn trading_control_service(&self) -> trading_control::TradingControlService {
        trading_control::TradingControlService {
            trading_control: self.trading_control.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            level_manager: self.level_manager.clone(),
        }
    }
}

#[async_trait]
impl TradingCliService for TradingCliServiceImpl {
    async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        self.trading_control_service()
            .set_trading_enabled(request)
            .await
    }

    async fn get_trading_status(
        &self,
        request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        self.trading_control_service()
            .get_trading_status(request)
            .await
    }

    async fn get_levels(
        &self,
        request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        self.level_service().get_levels(request).await
    }

    async fn get_level(
        &self,
        request: Request<trading_cli::GetLevelRequest>,
    ) -> Result<Response<trading_cli::GetLevelResponse>, Status> {
        self.level_service().get_level(request).await
    }

    async fn create_level(
        &self,
        request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        self.level_service().create_level(request).await
    }

    async fn update_level(
        &self,
        request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        self.level_service().update_level(request).await
    }

    async fn delete_level(
        &self,
        _request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        self.level_service().toggle_level(request).await
    }

    async fn toggle_strategy(
        &self,
        request: Request<trading_cli::ToggleStrategyRequest>,
    ) -> Result<Response<trading_cli::ToggleStrategyResponse>, Status> {
        // keep exact behavior from runner version
        let req = request.into_inner();

        let strategy_manager = self.strategy_manager.as_ref().ok_or_else(|| {
            Status::failed_precondition("Strategy manager not available (not in strategies mode)")
        })?;

        let parts: Vec<&str> = req.symbol.split('/').collect();
        if parts.len() != 2 {
            return Err(Status::invalid_argument(format!(
                "Invalid symbol format: {}. Expected 'BASE/QUOTE'",
                req.symbol
            )));
        }
        let symbol = trading_core::types::common::Pair::new(parts[0], parts[1]);
        let strategy_id = StrategyManager::make_strategy_id(&req.strategy_name, &symbol);

        let global_trading_enabled = self.trading_control.is_enabled().await;
        if !global_trading_enabled && req.enabled {
            return Err(Status::failed_precondition(
                "Cannot enable individual strategy when global trading is disabled. Enable global trading first.",
            ));
        }

        let current_enabled = strategy_manager
            .read()
            .await
            .is_strategy_enabled(&strategy_id)
            .await;

        if current_enabled != req.enabled {
            strategy_manager
                .write()
                .await
                .set_strategy_enabled(&strategy_id, req.enabled)
                .await;

            let mut cancelled_orders = Vec::new();
            if !req.enabled {
                let trading_state_arc = self.trading_state_manager.get_state();
                let trading_state = trading_state_arc.read().await;

                let symbol_str = req.symbol.clone();
                let symbol_no_slash = symbol_str.replace('/', "");
                let active_order_ids: Vec<_> = trading_state
                    .active_orders
                    .values()
                    .filter(|o| {
                        let order_symbol: smol_str::SmolStr = o.instrument.symbol().clone();
                        order_symbol.as_str() == symbol_str
                            || order_symbol.as_str() == symbol_no_slash
                    })
                    .map(|o| o.order_id.clone())
                    .collect();

                drop(trading_state);

                if !active_order_ids.is_empty() {
                    let trading_client = self.trading_client.read().await;
                    for order_id in active_order_ids {
                        match trading_client.cancel_order_by_id(order_id.clone()).await {
                            Ok(_) => {
                                cancelled_orders.push(order_id.as_str().to_string());
                                tracing::info!(
                                    "Cancelled order {} for disabled strategy {}",
                                    order_id.as_str(),
                                    strategy_id
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to cancel order {} for strategy {}: {}",
                                    order_id.as_str(),
                                    strategy_id,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            tracing::info!(
                "Strategy {} {} (cancelled {} orders)",
                strategy_id,
                if req.enabled { "enabled" } else { "disabled" },
                cancelled_orders.len()
            );

            Ok(Response::new(trading_cli::ToggleStrategyResponse {
                success: true,
                message: if req.enabled {
                    format!("Strategy {} enabled", strategy_id)
                } else {
                    format!(
                        "Strategy {} disabled. Cancelled {} orders.",
                        strategy_id,
                        cancelled_orders.len()
                    )
                },
                cancelled_orders,
            }))
        } else {
            Ok(Response::new(trading_cli::ToggleStrategyResponse {
                success: true,
                message: format!(
                    "Strategy {} is already {}",
                    strategy_id,
                    if req.enabled { "enabled" } else { "disabled" }
                ),
                cancelled_orders: vec![],
            }))
        }
    }

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

    async fn stream_monitoring_data(
        &self,
        _request: Request<trading_cli::StreamMonitoringDataRequest>,
    ) -> Result<Response<Self::StreamMonitoringDataStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let service = self.clone_for_streaming();

        tokio::spawn(async move {
            std::panic::set_hook(Box::new(|panic_info| {
                tracing::error!(
                    "[gRPC] stream_monitoring_data: PANIC in streaming task: {:?}",
                    panic_info
                );
            }));
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            interval.tick().await;
            loop {
                interval.tick().await;

                let collect_result = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    service.monitoring_service().collect_monitoring_data(),
                )
                .await;

                match collect_result {
                    Ok(Ok(data)) => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;

                        let update = trading_cli::MonitoringDataUpdate {
                            data: Some(data),
                            timestamp,
                        };

                        if tx.send(Ok(update)).await.is_err() {
                            tracing::debug!(
                                "[gRPC] stream_monitoring_data: receiver dropped, stopping"
                            );
                            break;
                        }
                    }
                    Ok(Err(e)) => {
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
                    Err(_) => {
                        tracing::error!(
                            "[gRPC] stream_monitoring_data: collect_monitoring_data timed out after 5 seconds"
                        );
                        continue;
                    }
                }
            }
        });

        Ok(Response::new(
            tonic::codegen::tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }
}
