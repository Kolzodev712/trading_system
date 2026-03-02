use clap::Parser;
use std::net::SocketAddr;
use tonic::transport::{Channel, Server};

use gateway::grpc::proxy::GatewayService;
use gateway::grpc::trading_cli::trading_cli_service_client::TradingCliServiceClient;
use gateway::grpc::trading_cli::trading_cli_service_server::TradingCliServiceServer;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Address the gateway listens on (gRPC)
    #[arg(long, default_value = "0.0.0.0:50052")]
    listen: SocketAddr,

    /// Runner gRPC endpoint to forward to
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    upstream: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let channel = Channel::from_shared(args.upstream.clone())?
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(5))
        .connect()
        .await?;

    let upstream_client = TradingCliServiceClient::new(channel);
    let service = GatewayService::new(upstream_client);

    tracing::info!("Gateway listening on {}", args.listen);
    tracing::info!("Forwarding to runner upstream {}", args.upstream);

    Server::builder()
        .add_service(TradingCliServiceServer::new(service))
        .serve(args.listen)
        .await?;

    Ok(())
}
