//! Connection pooling utilities for gRPC
//! Note: This is a placeholder for future connection pooling implementation

use crate::error::EngineError;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub max_connections: usize,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
        }
    }
}

/// Simple connection pool using semaphore
pub struct ConnectionPool {
    semaphore: Arc<Semaphore>,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_connections)),
        }
    }

    pub async fn acquire(&self) -> Result<tokio::sync::SemaphorePermit<'_>, EngineError> {
        self.semaphore.acquire().await.map_err(|_| {
            EngineError::CustomError(smol_str::format_smolstr!("Failed to acquire connection"))
        })
    }
}
