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
    CreateLevelRequest, GetLevelRequest, GetLevelsRequest, GetMonitoringDataRequest,
    GetTradingStatusRequest, SetTradingEnabledRequest, StreamMonitoringDataRequest,
    ToggleLevelRequest, ToggleStrategyRequest, UpdateLevelRequest,
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
        .map_err(CliError::Grpc)?;

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
        .map_err(CliError::Grpc)?;

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
                .map_err(CliError::Grpc)?;

        Ok(response.into_inner().levels)
    }

    /// Toggle level enabled/disabled
    pub async fn toggle_level(&mut self, level_id: &str, enabled: bool) -> Result<bool, CliError> {
        let request = tonic::Request::new(ToggleLevelRequest {
            level_id: level_id.to_string(),
            enabled,
        });
        let response =
            tokio::time::timeout(Duration::from_secs(5), self.client.toggle_level(request))
                .await
                .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
                .map_err(CliError::Grpc)?;

        Ok(response.into_inner().success)
    }

    /// Toggle strategy enabled/disabled
    pub async fn toggle_strategy(
        &mut self,
        strategy_name: &str,
        symbol: &str,
        enabled: bool,
    ) -> Result<Vec<String>, CliError> {
        let request = tonic::Request::new(ToggleStrategyRequest {
            strategy_name: strategy_name.to_string(),
            symbol: symbol.to_string(),
            enabled,
        });
        let response =
            tokio::time::timeout(Duration::from_secs(5), self.client.toggle_strategy(request))
                .await
                .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
                .map_err(CliError::Grpc)?;

        Ok(response.into_inner().cancelled_orders)
    }

    /// Get single level details
    pub async fn get_level(&mut self, level_id: &str) -> Result<trading_cli::Level, CliError> {
        let request = tonic::Request::new(GetLevelRequest {
            level_id: level_id.to_string(),
        });
        let response = tokio::time::timeout(Duration::from_secs(5), self.client.get_level(request))
            .await
            .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
            .map_err(CliError::Grpc)?;

        response
            .into_inner()
            .level
            .ok_or_else(|| CliError::Custom("Level not found".to_string()))
    }

    /// Update level configuration
    pub async fn update_level(
        &mut self,
        level_id: &str,
        config: trading_cli::LevelConfig,
    ) -> Result<bool, CliError> {
        let request = tonic::Request::new(UpdateLevelRequest {
            level_id: level_id.to_string(),
            config: Some(config),
        });
        let response =
            tokio::time::timeout(Duration::from_secs(5), self.client.update_level(request))
                .await
                .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
                .map_err(CliError::Grpc)?;

        Ok(response.into_inner().success)
    }

    /// Create a new level
    pub async fn create_level(
        &mut self,
        config: trading_cli::LevelConfig,
    ) -> Result<String, CliError> {
        let request = tonic::Request::new(CreateLevelRequest {
            config: Some(config),
        });
        let response =
            tokio::time::timeout(Duration::from_secs(5), self.client.create_level(request))
                .await
                .map_err(|_| CliError::Custom("gRPC call timed out".to_string()))?
                .map_err(CliError::Grpc)?;

        Ok(response.into_inner().level_id)
    }
}
