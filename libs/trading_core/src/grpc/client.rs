//! Generic gRPC client utilities
//! Note: This provides utilities, NOT proto definitions
//! Each service defines its own proto files

use crate::error::EngineError;
use std::time::Duration;
use tonic::transport::Channel;

/// Generic gRPC client configuration
#[derive(Debug, Clone)]
pub struct GrpcClientConfig {
    pub endpoint: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://[::1]:50051".to_string(),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
        }
    }
}

/// Create a gRPC channel with default configuration
pub async fn create_channel(endpoint: &str) -> Result<Channel, EngineError> {
    Channel::from_shared(endpoint.to_string())
        .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Invalid endpoint: {}", e)))?
        .connect()
        .await
        .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Connection failed: {}", e)))
}
