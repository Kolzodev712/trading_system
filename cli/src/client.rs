//! gRPC client for connecting to Trading System
use crate::error::CliError;
use std::time::Duration;
use tonic::transport::Channel;

// Include the generated protobuf code
#[allow(clippy::all)]
pub mod trading_cli {
    include!(concat!(env!("OUT_DIR"), "/trading_cli.rs"));
}

use trading_cli::trading_cli_service_client::TradingCliServiceClient;
use trading_cli::{
    GetLevelsRequest, GetMonitoringDataRequest, GetTradingStatusRequest, SetTradingEnabledRequest,
    StreamMonitoringDataRequest,
};

pub struct TradingCliClient {
    client: TradingCliServiceClient<Channel>,
}

impl TradingCliClient {
    /// Connect to trading system gRPC server
    pub async fn connect(endpoint: &str) -> Result<Self, CliError> {
        let channel = Channel::from_shared(endpoint.to_string())
            .map_err(|e| CliError::Custom(format!("Invalid endpoint: {}", e)))?
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await?;

        let client = TradingCliServiceClient::new(channel);
        Ok(Self { client })
    }

    /// Get trading status
    pub async fn get_trading_status(&mut self) -> Result<bool, CliError> {
        let request = tonic::Request::new(GetTradingStatusRequest {});
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            self.client.get_trading_status(request),
        )
        .await
        .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
        .map_err(|e| CliError::Grpc(e))?;

        Ok(response.into_inner().enabled)
    }

    /// Set trading enabled/disabled
    pub async fn set_trading_enabled(&mut self, enabled: bool) -> Result<Vec<String>, CliError> {
        let request = tonic::Request::new(SetTradingEnabledRequest { enabled });
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            self.client.set_trading_enabled(request),
        )
        .await
        .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
        .map_err(|e| CliError::Grpc(e))?;

        Ok(response.into_inner().cancelled_orders)
    }

    /// Get monitoring data (one-shot)
    pub async fn get_monitoring_data(&mut self) -> Result<trading_cli::MonitoringData, CliError> {
        let request = tonic::Request::new(GetMonitoringDataRequest {});
        let response = self.client.get_monitoring_data(request).await?;
        Ok(response.into_inner().data.unwrap_or_default())
    }

    /// Stream monitoring data
    pub async fn stream_monitoring_data(
        &mut self,
    ) -> Result<tonic::codec::Streaming<trading_cli::MonitoringDataUpdate>, CliError> {
        let request = tonic::Request::new(StreamMonitoringDataRequest {});
        let stream = self.client.stream_monitoring_data(request).await?;
        Ok(stream.into_inner())
    }

    /// Get levels
    pub async fn get_levels(&mut self) -> Result<Vec<trading_cli::Level>, CliError> {
        let request = tonic::Request::new(GetLevelsRequest {});
        let response =
            tokio::time::timeout(Duration::from_secs(5), self.client.get_levels(request))
                .await
                .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
                .map_err(|e| CliError::Grpc(e))?;

        Ok(response.into_inner().levels)
    }
}
