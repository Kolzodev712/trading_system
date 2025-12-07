//! Generic gRPC server utilities
//! Note: This provides utilities, NOT proto definitions
//! Each service defines its own proto files

use tonic::transport::Server;

/// Helper function to create a server builder
///
/// Example:
/// ```ignore
/// let server = create_server_builder()
///     .add_service(MyServiceServer::new(my_service))
///     .serve(addr)
///     .await?;
/// ```
pub fn create_server_builder() -> Server {
    Server::builder()
}

// Note: Services should typically use Server::builder() directly:
// Server::builder()
//     .add_service(MyServiceServer::new(my_service))
//     .serve(addr)
//     .await?;
