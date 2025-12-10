//! gRPC client for connecting to Trading System
use crate::error::CliError;
use tonic::transport::Channel;

// Include the generated protobuf code
#[allow(clippy::all)]
pub mod trading_cli {
    include!(concat!(env!("OUT_DIR"), "/trading_cli.rs"));
}

use trading_cli::trading_cli_service_client::TradingCliServiceClient;
use trading_cli::{GetTradingStatusRequest, SetTradingEnabledRequest};

pub struct TradingCliClient {
    client: TradingCliServiceClient<Channel>,
}

impl TradingCliClient {
    /// Connect to trading system gRPC server
    pub async fn connect(endpoint: &str) -> Result<Self, CliError> {
        let channel = Channel::from_shared(endpoint.to_string())
            .map_err(|e| CliError::Custom(format!("Invalid endpoint: {}", e)))?
            .connect()
            .await?;

        let client = TradingCliServiceClient::new(channel);

        Ok(Self { client })
    }

    /// Get trading status
    pub async fn get_trading_status(&mut self) -> Result<bool, CliError> {
        let request = tonic::Request::new(GetTradingStatusRequest {});
        let response = self.client.get_trading_status(request).await?;
        Ok(response.into_inner().enabled)
    }

    /// Set trading enabled/disabled
    pub async fn set_trading_enabled(&mut self, enabled: bool) -> Result<Vec<String>, CliError> {
        let request = tonic::Request::new(SetTradingEnabledRequest { enabled });
        let response = self.client.set_trading_enabled(request).await?;
        let inner = response.into_inner();
        Ok(inner.cancelled_orders)
    }
}
