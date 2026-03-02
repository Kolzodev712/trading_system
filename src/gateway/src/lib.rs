pub mod grpc;

// Backward-compat shims (older paths used `gateway::proto`, `gateway::server`, `gateway::service`)
pub mod proto {
    pub use crate::grpc::proto::*;
}

pub mod server {
    pub use crate::grpc::server::*;
}

pub mod service {
    pub use crate::grpc::proxy::*;
}

pub use grpc::trading_cli;
