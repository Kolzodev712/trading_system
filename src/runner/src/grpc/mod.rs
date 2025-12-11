pub mod server;
pub mod service;

// Include the generated protobuf code
#[allow(clippy::all)]
pub mod trading_cli {
    tonic::include_proto!("trading_cli");
}

pub use server::TradingGrpcServer;
pub use service::TradingCliServiceImpl;
