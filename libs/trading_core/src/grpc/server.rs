//! Generic gRPC server utilities
//! Note: This provides utilities, NOT proto definitions
//! Each service defines its own proto files

use std::net::SocketAddr;

/// Check if a port is available for binding
/// Returns Ok(()) if available, Err with message if not
pub async fn check_port_available(addr: SocketAddr) -> Result<(), String> {
    match tokio::net::TcpListener::bind(&addr).await {
        Ok(_) => {
            tracing::info!("[gRPC] Port {} is available", addr);
            Ok(())
        }
        Err(e) => {
            tracing::error!("[gRPC] Cannot bind to {}: {}", addr, e);
            Err(format!("Port {} is already in use: {}", addr, e))
        }
    }
}
