//! gRPC Server setup and lifecycle management
use crate::grpc::service::TradingCliServiceImpl;
use crate::grpc::trading_cli::trading_cli_service_server::TradingCliServiceServer;
use std::net::SocketAddr;
use std::sync::Arc;
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
    ) -> Self {
        let service = TradingCliServiceImpl::new(
            level_manager,
            trading_state_manager.clone(),
            trading_client.clone(),
        );

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
            Server::builder()
                .add_service(trading_cli_service)
                .serve(addr)
                .await
        });
        Ok(handle)
    }
}
