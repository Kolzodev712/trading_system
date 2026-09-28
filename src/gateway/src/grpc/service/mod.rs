//! gRPC Service Implementation
mod converters;
mod dex_quotes;
mod dex_strategies;
mod levels;
mod monitoring;
mod strategy_config;
mod trading_control;

pub use monitoring::{MonitoringService, StrategyInfo};
pub use strategy_config::{OperatorFields, StrategyOperator, StrategyOperatorHandle};

use async_trait::async_trait;
use dex_engine::runtime::DexEngineRuntime;
use exchange_engine::state::manager::ExchangeStateManager;
use indicator_engine::IndicatorService;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status};
use trading_core::grpc::{TradingControl, GLOBAL_TOGGLE_SEMANTICS};
use trading_core::types::exchange::OrderRequest;
use trading_core::venue::cex::traits::TradingClient;
use trading_engine::dex::{DexIndicatorEngine, DexQuoteManager, DexStrategyManager};
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
    dex_state_manager: Option<Arc<dex_engine::state::manager::DexStateManager>>,
    dex_venue_label: Option<String>,
    dex_quote_manager: Option<Arc<RwLock<DexQuoteManager>>>,
    dex_strategy_manager: Option<Arc<RwLock<DexStrategyManager>>>,
    dex_indicator_engine: Option<Arc<DexIndicatorEngine>>,
    dex_runtime: Option<Arc<DexEngineRuntime>>,
    dex_balance_manager_id: Option<SmolStr>,
    strategy_operator: Option<strategy_config::StrategyOperatorHandle>,
    strategy_caps: Option<Arc<RwLock<std::collections::HashMap<String, rust_decimal::Decimal>>>>,
    execution_mode: trading_core::types::ExecutionMode,
    venue_label: String,
    environment: String,
    operator_results: Arc<Mutex<HashMap<String, trading_cli::OperatorCommandResponse>>>,
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
            dex_state_manager: None,
            dex_venue_label: None,
            dex_quote_manager: None,
            dex_strategy_manager: None,
            dex_indicator_engine: None,
            dex_runtime: None,
            dex_balance_manager_id: None,
            strategy_operator: None,
            strategy_caps: None,
            execution_mode: trading_core::types::ExecutionMode::Exchange,
            venue_label: "UNKNOWN".to_string(),
            environment: "UNKNOWN".to_string(),
            operator_results: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_dex_strategy_manager(
        mut self,
        strategy_manager: Arc<RwLock<DexStrategyManager>>,
    ) -> Self {
        self.dex_strategy_manager = Some(strategy_manager);
        self
    }

    pub fn with_dex_indicator_engine(mut self, engine: Arc<DexIndicatorEngine>) -> Self {
        self.dex_indicator_engine = Some(engine);
        self
    }

    pub fn with_dex_quote_control(
        mut self,
        quote_manager: Arc<RwLock<DexQuoteManager>>,
        dex_runtime: Arc<DexEngineRuntime>,
        balance_manager_id: SmolStr,
    ) -> Self {
        self.dex_quote_manager = Some(quote_manager);
        self.dex_runtime = Some(dex_runtime);
        self.dex_balance_manager_id = Some(balance_manager_id);
        self
    }

    pub fn with_dex_state_manager(
        mut self,
        dex_state_manager: Arc<dex_engine::state::manager::DexStateManager>,
        venue_label: impl Into<String>,
    ) -> Self {
        self.dex_state_manager = Some(dex_state_manager);
        self.dex_venue_label = Some(venue_label.into());
        self
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

    pub fn with_strategy_operator(
        mut self,
        operator: strategy_config::StrategyOperatorHandle,
    ) -> Self {
        self.strategy_operator = Some(operator);
        self
    }

    pub fn with_strategy_caps(
        mut self,
        caps: Arc<RwLock<std::collections::HashMap<String, rust_decimal::Decimal>>>,
    ) -> Self {
        self.strategy_caps = Some(caps);
        self
    }

    pub fn with_execution_status(
        mut self,
        execution_mode: trading_core::types::ExecutionMode,
        venue_label: impl Into<String>,
        environment: impl Into<String>,
    ) -> Self {
        self.execution_mode = execution_mode;
        self.venue_label = venue_label.into();
        self.environment = environment.into();
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
            dex_state_manager: self.dex_state_manager.clone(),
            dex_venue_label: self.dex_venue_label.clone(),
            dex_quote_manager: self.dex_quote_manager.clone(),
            dex_strategy_manager: self.dex_strategy_manager.clone(),
            dex_indicator_engine: self.dex_indicator_engine.clone(),
            dex_runtime: self.dex_runtime.clone(),
            dex_balance_manager_id: self.dex_balance_manager_id.clone(),
            strategy_operator: self.strategy_operator.clone(),
            strategy_caps: self.strategy_caps.clone(),
            execution_mode: self.execution_mode,
            venue_label: self.venue_label.clone(),
            environment: self.environment.clone(),
            operator_results: self.operator_results.clone(),
        }
    }

    fn dex_quote_service(&self) -> Option<dex_quotes::DexQuoteService> {
        Some(dex_quotes::DexQuoteService {
            quote_manager: self.dex_quote_manager.clone()?,
            dex_runtime: self.dex_runtime.clone()?,
            balance_manager_id: self.dex_balance_manager_id.clone()?,
        })
    }

    fn dex_strategy_service(&self) -> Option<dex_strategies::DexStrategyService> {
        Some(dex_strategies::DexStrategyService {
            strategy_manager: self.dex_strategy_manager.clone()?,
        })
    }

    fn monitoring_service(&self) -> monitoring::MonitoringService {
        monitoring::MonitoringService {
            level_manager: self.level_manager.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            dex_state_manager: self.dex_state_manager.clone(),
            market_data_state_manager: self.market_data_state_manager.clone(),
            indicator_calculator: self.indicator_calculator.clone(),
            strategy_infos: self.strategy_infos.clone(),
            strategy_manager: self.strategy_manager.clone(),
            instrument_type: self.instrument_type.clone(),
            exchange: self.exchange.clone(),
            dex_venue_label: self.dex_venue_label.clone(),
            dex_quote_manager: self.dex_quote_manager.clone(),
            dex_strategy_manager: self.dex_strategy_manager.clone(),
            dex_indicator_engine: self.dex_indicator_engine.clone(),
            strategy_caps: self.strategy_caps.clone(),
            execution_mode: self.execution_mode,
            venue_label: self.venue_label.clone(),
            environment: self.environment.clone(),
            trading_control: self.trading_control.clone(),
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
            operator_results: self.operator_results.clone(),
        }
    }

    fn require_mutating<T>(&self, request: &Request<T>) -> Result<(), Status> {
        crate::grpc::access::check_mutating_access(request.metadata())
    }
}

#[async_trait]
impl TradingCliService for TradingCliServiceImpl {
    async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        self.require_mutating(&request)?;
        self.trading_control_service()
            .set_trading_enabled(request)
            .await
    }

    async fn get_trading_status(
        &self,
        _request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        let enabled = self.trading_control.is_enabled().await;
        Ok(Response::new(trading_cli::GetTradingStatusResponse {
            enabled,
            execution_mode: self.execution_mode.as_str().to_string(),
            venue: self.venue_label.clone(),
            environment: self.environment.clone(),
            entries_paused: self.trading_control.entries_paused().await,
            flatten_latched: self.trading_control.flatten_latched().await,
            global_toggle_semantics: GLOBAL_TOGGLE_SEMANTICS.to_string(),
        }))
    }

    async fn pause_entries(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        self.require_mutating(&request)?;
        self.trading_control_service()
            .pause_entries(request, self.venue_label.clone(), self.environment.clone())
            .await
    }

    async fn cancel_working(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        self.require_mutating(&request)?;
        self.trading_control_service()
            .cancel_working(request, self.venue_label.clone(), self.environment.clone())
            .await
    }

    async fn flatten_and_pause(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        self.require_mutating(&request)?;
        self.trading_control_service()
            .flatten_and_pause(
                request,
                self.venue_label.clone(),
                self.environment.clone(),
                self.instrument_type.clone(),
            )
            .await
    }

    async fn resume(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        self.require_mutating(&request)?;
        self.trading_control_service()
            .resume(request, self.venue_label.clone(), self.environment.clone())
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
        self.require_mutating(&request)?;
        self.level_service().create_level(request).await
    }

    async fn update_level(
        &self,
        request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        self.require_mutating(&request)?;
        self.level_service().update_level(request).await
    }

    async fn delete_level(
        &self,
        request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        self.require_mutating(&request)?;
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        self.require_mutating(&request)?;
        self.level_service().toggle_level(request).await
    }

    async fn toggle_dex_quote(
        &self,
        request: Request<trading_cli::ToggleDexQuoteRequest>,
    ) -> Result<Response<trading_cli::ToggleDexQuoteResponse>, Status> {
        self.require_mutating(&request)?;
        let Some(service) = self.dex_quote_service() else {
            return Err(Status::failed_precondition(
                "DEX quote control not available (not in dex levels mode)",
            ));
        };
        service.toggle_dex_quote(request).await
    }

    async fn cancel_dex_pool_orders(
        &self,
        request: Request<trading_cli::CancelDexPoolOrdersRequest>,
    ) -> Result<Response<trading_cli::CancelDexPoolOrdersResponse>, Status> {
        self.require_mutating(&request)?;
        let Some(service) = self.dex_quote_service() else {
            return Err(Status::failed_precondition(
                "DEX quote control not available (not in dex levels mode)",
            ));
        };
        service.cancel_dex_pool_orders(request).await
    }

    async fn toggle_dex_strategy(
        &self,
        request: Request<trading_cli::ToggleDexStrategyRequest>,
    ) -> Result<Response<trading_cli::ToggleDexStrategyResponse>, Status> {
        self.require_mutating(&request)?;
        let Some(service) = self.dex_strategy_service() else {
            return Err(Status::failed_precondition(
                "DEX strategy control not available (not in dex-strategies mode)",
            ));
        };
        service.toggle_dex_strategy(request).await
    }

    async fn toggle_strategy(
        &self,
        request: Request<trading_cli::ToggleStrategyRequest>,
    ) -> Result<Response<trading_cli::ToggleStrategyResponse>, Status> {
        self.require_mutating(&request)?;
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

    async fn get_strategy_config(
        &self,
        request: Request<trading_cli::GetStrategyConfigRequest>,
    ) -> Result<Response<trading_cli::GetStrategyConfigResponse>, Status> {
        let req = request.into_inner();
        let op = self.strategy_operator.as_ref().ok_or_else(|| {
            Status::failed_precondition("Strategy config not available (not in strategies mode)")
        })?;
        let fields = op
            .get_config(&req.strategy_name, &req.symbol)
            .await
            .map_err(Status::not_found)?;
        Ok(Response::new(trading_cli::GetStrategyConfigResponse {
            config: Some(operator_fields_to_proto(fields)),
        }))
    }

    async fn update_strategy_cap(
        &self,
        request: Request<trading_cli::UpdateStrategyCapRequest>,
    ) -> Result<Response<trading_cli::UpdateStrategyCapResponse>, Status> {
        self.require_mutating(&request)?;
        let req = request.into_inner();
        let op = self.strategy_operator.as_ref().ok_or_else(|| {
            Status::failed_precondition("Strategy config not available (not in strategies mode)")
        })?;
        op.update_cap(&req.strategy_name, &req.symbol, &req.max_position)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(trading_cli::UpdateStrategyCapResponse {
            success: true,
            message: format!(
                "Cap updated for {}:{} to {}",
                req.strategy_name, req.symbol, req.max_position
            ),
        }))
    }

    async fn update_strategy_config(
        &self,
        request: Request<trading_cli::UpdateStrategyConfigRequest>,
    ) -> Result<Response<trading_cli::UpdateStrategyConfigResponse>, Status> {
        self.require_mutating(&request)?;
        let req = request.into_inner();
        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("UpdateStrategyConfig requires config"))?;
        let op = self.strategy_operator.as_ref().ok_or_else(|| {
            Status::failed_precondition("Strategy config not available (not in strategies mode)")
        })?;
        let needs_restart = op
            .update_config(proto_to_operator_fields(config))
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(trading_cli::UpdateStrategyConfigResponse {
            success: true,
            message: if needs_restart {
                "Saved. Restart the runner to apply non-cap fields.".to_string()
            } else {
                "Saved. Cap applied live.".to_string()
            },
            needs_restart,
        }))
    }

    async fn request_restart(
        &self,
        request: Request<trading_cli::RequestRestartRequest>,
    ) -> Result<Response<trading_cli::RequestRestartResponse>, Status> {
        self.require_mutating(&request)?;
        let req = request.into_inner();
        let op = self.strategy_operator.as_ref().ok_or_else(|| {
            Status::failed_precondition("Strategy config not available (not in strategies mode)")
        })?;
        let on_stop = if req.on_stop.is_empty() {
            "cancel_working"
        } else {
            req.on_stop.as_str()
        };
        op.request_restart(on_stop)
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(trading_cli::RequestRestartResponse {
            success: true,
            message: "Restart requested. Supervisor script will relaunch.".to_string(),
        }))
    }
}

fn operator_fields_to_proto(
    fields: strategy_config::OperatorFields,
) -> trading_cli::OperatorStrategyConfig {
    trading_cli::OperatorStrategyConfig {
        strategy_type: fields.strategy_type,
        symbol: fields.symbol,
        position_size: fields.position_size,
        max_position: fields.max_position,
        buy_threshold_percent: fields.buy_threshold_percent,
        sell_threshold_percent: fields.sell_threshold_percent,
        stop_loss_percent: fields.stop_loss_percent,
        take_profit_percent: fields.take_profit_percent,
        max_active_orders: fields.max_active_orders,
        price_history_size: fields.price_history_size,
        min_price_history: fields.min_price_history,
    }
}

fn proto_to_operator_fields(
    config: trading_cli::OperatorStrategyConfig,
) -> strategy_config::OperatorFields {
    strategy_config::OperatorFields {
        strategy_type: config.strategy_type,
        symbol: config.symbol,
        position_size: config.position_size,
        max_position: config.max_position,
        buy_threshold_percent: config.buy_threshold_percent,
        sell_threshold_percent: config.sell_threshold_percent,
        stop_loss_percent: config.stop_loss_percent,
        take_profit_percent: config.take_profit_percent,
        max_active_orders: config.max_active_orders,
        price_history_size: config.price_history_size,
        min_price_history: config.min_price_history,
    }
}
