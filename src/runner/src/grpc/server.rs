//! gRPC Server setup and lifecycle management
use crate::grpc::service::TradingCliServiceImpl;
use crate::grpc::trading_cli::trading_cli_service_server::TradingCliServiceServer;
use exchange_engine::state::manager::ExchangeStateManager;
use indicator_engine::calculator::IndicatorCalculator;
use indicator_engine::IndicatorService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tonic::transport::Server;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

/// gRPC Server for Trading CLI
pub struct TradingGrpcServer {
    #[allow(dead_code)]
    service: TradingCliServiceImpl,
}

impl TradingGrpcServer {
    /// Create a new gRPC server
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
        indicator_service: Arc<Mutex<IndicatorService>>,
        indicator_calculator: Arc<IndicatorCalculator>,
        trading_control: Arc<trading_core::grpc::TradingControl>,
    ) -> Self {
        let service = TradingCliServiceImpl::new(
            level_manager,
            trading_state_manager,
            trading_client,
            exchange_state_manager,
            indicator_service,
            indicator_calculator,
            trading_control,
        );

        Self { service }
    }

    /// Create a new gRPC server with strategy info (for strategies mode)
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_strategy(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
        indicator_service: Arc<Mutex<IndicatorService>>,
        indicator_calculator: Arc<IndicatorCalculator>,
        trading_control: Arc<trading_core::grpc::TradingControl>,
        strategy_name: String,
        symbol: trading_core::types::common::Pair,
        max_active_orders: usize,
    ) -> Self {
        let service = TradingCliServiceImpl::new(
            level_manager,
            trading_state_manager,
            trading_client,
            exchange_state_manager,
            indicator_service,
            indicator_calculator,
            trading_control,
        )
        .with_strategy_info(strategy_name, symbol, max_active_orders);

        Self { service }
    }

    /// Create gRPC server with multiple strategies
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_strategies(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
        indicator_service: Arc<Mutex<IndicatorService>>,
        indicator_calculator: Arc<IndicatorCalculator>,
        trading_control: Arc<trading_core::grpc::TradingControl>,
        strategy_infos: Vec<crate::grpc::service::StrategyInfo>,
    ) -> Self {
        let service = TradingCliServiceImpl::new(
            level_manager,
            trading_state_manager,
            trading_client,
            exchange_state_manager,
            indicator_service,
            indicator_calculator,
            trading_control,
        )
        .with_strategy_infos(strategy_infos);

        Self { service }
    }

    /// Start the gRPC server
    pub async fn start(
        self,
        addr: SocketAddr,
    ) -> Result<JoinHandle<Result<(), tonic::transport::Error>>, Box<dyn std::error::Error>> {
        let service = self.service;
        let trading_cli_service = TradingCliServiceServer::new(service);
        let handle = tokio::spawn(async move {
            tracing::info!("[gRPC SERVER] Starting server on {}", addr);
            let result = Server::builder()
                .add_service(trading_cli_service)
                .serve(addr)
                .await;
            match &result {
                Ok(_) => {
                    tracing::info!("[gRPC SERVER] Server stopped normally");
                }
                Err(e) => {
                    tracing::error!("[gRPC SERVER] Server error: {}", e);
                    tracing::error!("[gRPC SERVER] Error details: {:?}", e);
                }
            }
            result
        });
        tracing::info!(
            "[gRPC SERVER] Server task spawned, should be listening on {}",
            addr
        );
        Ok(handle)
    }
}
