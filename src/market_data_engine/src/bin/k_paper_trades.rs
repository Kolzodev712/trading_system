//! Test binary: Stream k-paper public trade updates
//!
//! Usage:
//!   PAPER_EXCHANGE_URL=ws://127.0.0.1:19999/ws/market cargo run -p market_data_engine --bin k_paper_trades
//!   # or, if omitted:
//!   cargo run -p market_data_engine --bin k_paper_trades
//!
//! This connects to k-paper WebSocket and prints public_trade updates to stdout.

use market_data_engine::{exchanges::k_paper::PaperClient, exchanges::k_paper::PaperConfig};
use tokio::signal;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    traits::MarketDataClient,
    types::market_data::{StructuredOrderbook, StructuredPublicTrade},
};

#[derive(Debug)]
struct PrintTradeHandler;

#[async_trait::async_trait]
impl market_data_engine::messages::WSMessageHandler for PrintTradeHandler {
    async fn handle_orderbook(&self, _orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        Ok(())
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        println!(
            "[{}] k-paper Trade - {}: qty={} price={} side={} ts={} id={:?}",
            chrono::Utc::now().format("%H:%M:%S%.3f"),
            trade.pair,
            trade.quantity,
            trade.price,
            if trade.is_market_maker {
                "maker"
            } else {
                "taker"
            },
            trade.timestamp,
            trade.trade_id
        );
        Ok(())
    }
}

fn k_paper_config_from_env() -> PaperConfig {
    let url = std::env::var("PAPER_EXCHANGE_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:19999/ws/market".to_string());
    PaperConfig { url, pairs: vec![] }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = k_paper_config_from_env();
    info!("Starting k-paper trade stream (url={})", config.url);

    let handler = Box::new(PrintTradeHandler);
    let mut client = PaperClient::new(config, Some(handler), vec![]);

    let _client_handle = tokio::spawn(async move {
        // trades() is implemented as orderbooks() for k-paper, since both flow over same WS.
        if let Err(e) = client.trades().await {
            error!("k-paper market data client error: {:?}", e);
        }
    });

    info!("Listening for trade updates... (Press Ctrl-C to stop)");
    signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    info!("Ctrl-C received, shutting down...");

    Ok(())
}
