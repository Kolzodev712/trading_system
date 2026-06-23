use crate::grpc::trading_cli;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status};

use trading_cli::trading_cli_service_client::TradingCliServiceClient;
use trading_cli::trading_cli_service_server::TradingCliService;

pub struct GatewayService {
    upstream: TradingCliServiceClient<tonic::transport::Channel>,
}

impl GatewayService {
    pub fn new(upstream: TradingCliServiceClient<tonic::transport::Channel>) -> Self {
        Self { upstream }
    }

    fn client(&self) -> TradingCliServiceClient<tonic::transport::Channel> {
        self.upstream.clone()
    }
}

#[tonic::async_trait]
impl TradingCliService for GatewayService {
    async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        self.client().set_trading_enabled(request).await
    }

    async fn get_trading_status(
        &self,
        request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        self.client().get_trading_status(request).await
    }

    async fn toggle_strategy(
        &self,
        request: Request<trading_cli::ToggleStrategyRequest>,
    ) -> Result<Response<trading_cli::ToggleStrategyResponse>, Status> {
        self.client().toggle_strategy(request).await
    }

    async fn get_levels(
        &self,
        request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        self.client().get_levels(request).await
    }

    async fn get_level(
        &self,
        request: Request<trading_cli::GetLevelRequest>,
    ) -> Result<Response<trading_cli::GetLevelResponse>, Status> {
        self.client().get_level(request).await
    }

    async fn create_level(
        &self,
        request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        self.client().create_level(request).await
    }

    async fn update_level(
        &self,
        request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        self.client().update_level(request).await
    }

    async fn delete_level(
        &self,
        request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        self.client().delete_level(request).await
    }

    async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        self.client().toggle_level(request).await
    }

    async fn toggle_dex_quote(
        &self,
        request: Request<trading_cli::ToggleDexQuoteRequest>,
    ) -> Result<Response<trading_cli::ToggleDexQuoteResponse>, Status> {
        self.client().toggle_dex_quote(request).await
    }

    async fn cancel_dex_pool_orders(
        &self,
        request: Request<trading_cli::CancelDexPoolOrdersRequest>,
    ) -> Result<Response<trading_cli::CancelDexPoolOrdersResponse>, Status> {
        self.client().cancel_dex_pool_orders(request).await
    }

    async fn toggle_dex_strategy(
        &self,
        request: Request<trading_cli::ToggleDexStrategyRequest>,
    ) -> Result<Response<trading_cli::ToggleDexStrategyResponse>, Status> {
        self.client().toggle_dex_strategy(request).await
    }

    async fn get_monitoring_data(
        &self,
        request: Request<trading_cli::GetMonitoringDataRequest>,
    ) -> Result<Response<trading_cli::GetMonitoringDataResponse>, Status> {
        self.client().get_monitoring_data(request).await
    }

    type StreamMonitoringDataStream = Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<trading_cli::MonitoringDataUpdate, Status>>
                + Send,
        >,
    >;

    async fn stream_monitoring_data(
        &self,
        request: Request<trading_cli::StreamMonitoringDataRequest>,
    ) -> Result<Response<Self::StreamMonitoringDataStream>, Status> {
        let response = self.client().stream_monitoring_data(request).await?;
        let mut inbound = response.into_inner();

        let (tx, rx) = mpsc::channel::<Result<trading_cli::MonitoringDataUpdate, Status>>(128);

        tokio::spawn(async move {
            while let Some(item) = inbound.next().await {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}
