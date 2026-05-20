//! Test binary: Stream k-paper orderbook updates
//!
//! Usage:
//!   PAPER_EXCHANGE_URL=ws://127.0.0.1:19999/ws/market cargo run -p market_data_engine --bin k_paper_orderbooks
//!   # or, if omitted:
//!   cargo run -p market_data_engine --bin k_paper_orderbooks
//!
//! This connects to k-paper WebSocket and prints orderbook updates to stdout.

use market_data_engine::{exchanges::k_paper::PaperClient, exchanges::k_paper::PaperConfig};
use tokio::signal;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    types::market_data::{StructuredOrderbook, StructuredPublicTrade},
    venue::cex::traits::MarketDataClient,
};

#[derive(Debug)]
struct PrintOrderbookHandler;

#[async_trait::async_trait]
impl market_data_engine::messages::WSMessageHandler for PrintOrderbookHandler {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        println!(
            "[{}] k-paper Orderbook - {}: {} bids, {} asks, ts={}",
            chrono::Utc::now().format("%H:%M:%S%.3f"),
            orderbook.pair,
            orderbook.bids.len(),
            orderbook.asks.len(),
            orderbook.timestamp
        );

        if let (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) =
            (orderbook.bids.first(), orderbook.asks.first())
        {
            println!(
                "  Top: bid {} @ {} | ask {} @ {}",
                bid_qty, bid_px, ask_qty, ask_px
            );
        }
        Ok(())
    }

    async fn handle_trade(&self, _trade: StructuredPublicTrade) -> Result<(), EngineError> {
        Ok(())
    }
}

fn k_paper_config_from_env() -> PaperConfig {
    let url = std::env::var("PAPER_EXCHANGE_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:19999/ws/market".to_string());
    PaperConfig {
        url,
        // Not used by PaperClient today; instrument_id is parsed from incoming messages.
        pairs: vec![],
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = k_paper_config_from_env();
    info!("Starting k-paper orderbook stream (url={})", config.url);

    let handler = Box::new(PrintOrderbookHandler);
    let mut client = PaperClient::new(config, Some(handler), vec![]);

    let _client_handle = tokio::spawn(async move {
        if let Err(e) = client.orderbooks().await {
            error!("k-paper market data client error: {:?}", e);
        }
    });

    info!("Listening for orderbook updates... (Press Ctrl-C to stop)");
    signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    info!("Ctrl-C received, shutting down...");

    Ok(())
}
