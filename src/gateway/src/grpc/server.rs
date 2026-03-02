use crate::grpc::trading_cli::trading_cli_service_server::{
    TradingCliService, TradingCliServiceServer,
};
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tonic::transport::Server;

pub struct TradingGrpcServer<S> {
    service: S,
}

impl<S> TradingGrpcServer<S> {
    pub fn from_service(service: S) -> Self {
        Self { service }
    }
}

impl<S> TradingGrpcServer<S>
where
    S: TradingCliService + Send + 'static,
{
    pub async fn start(
        self,
        addr: SocketAddr,
    ) -> Result<JoinHandle<Result<(), tonic::transport::Error>>, Box<dyn std::error::Error>> {
        let trading_cli_service = TradingCliServiceServer::new(self.service);

        let handle = tokio::spawn(async move {
            tracing::info!("[gateway/grpc] Starting server on {}", addr);
            Server::builder()
                .add_service(trading_cli_service)
                .serve(addr)
                .await
        });

        Ok(handle)
    }
}
