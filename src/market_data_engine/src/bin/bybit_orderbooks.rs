//! Test binary: Stream Bybit orderbook updates
//!
//! This binary connects to Bybit WebSocket and streams orderbook updates
//! for the configured pairs, printing them to stdout.
//!
//! Usage:
//!   cargo run -p market_data_engine --bin bybit_orderbooks
//!
//! Configuration:
//!   Loads from config/bybit_config.json

use market_data_engine::{
    exchanges::bybit::{client::BybitClient, config::BybitConfig},
    messages::WSMessageHandler,
};
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info};
use trading_core::{
    error::EngineError, types::market_data::StructuredOrderbook,
    venue::cex::traits::MarketDataClient,
};

/// Simple message handler that prints orderbook updates
#[derive(Debug)]
struct PrintOrderbookHandler;

#[async_trait::async_trait]
impl WSMessageHandler for PrintOrderbookHandler {
    async fn handle_orderbook(&self, orderbook: StructuredOrderbook) -> Result<(), EngineError> {
        println!(
            "[{}] Orderbook - {}: {} bids, {} asks, timestamp: {}",
            chrono::Utc::now().format("%H:%M:%S%.3f"),
            orderbook.pair,
            orderbook.bids.len(),
            orderbook.asks.len(),
            orderbook.timestamp
        );

        // Print top 3 bids and asks
        if !orderbook.bids.is_empty() {
            println!("  Top bids:");
            for (idx, (price, qty)) in orderbook.bids.iter().take(3).enumerate() {
                println!("    {}: {} @ {}", idx + 1, qty, price);
            }
        }
        if !orderbook.asks.is_empty() {
            println!("  Top asks:");
            for (idx, (price, qty)) in orderbook.asks.iter().take(3).enumerate() {
                println!("    {}: {} @ {}", idx + 1, qty, price);
            }
        }

        Ok(())
    }

    async fn handle_trade(
        &self,
        _trade: trading_core::types::market_data::StructuredPublicTrade,
    ) -> Result<(), EngineError> {
        // Ignore trades
        Ok(())
    }
}

fn load_bybit_config() -> Result<BybitConfig, Box<dyn std::error::Error>> {
    // Get config path relative to workspace root
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("src")
        .join("market_data_engine")
        .join("config")
        .join("bybit_config.json");

    let config_str = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config from {:?}: {}", config_path, e))?;

    let config: BybitConfig = serde_json::from_str(&config_str)
        .map_err(|e| format!("Failed to parse config JSON: {}", e))?;

    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load config from JSON file
    let bybit_config = load_bybit_config()?;
    let pairs = bybit_config.pairs.clone();

    info!("Starting Bybit orderbook stream for pairs: {:?}", pairs);

    // Create message handler
    let handler = Box::new(PrintOrderbookHandler);

    // Create Bybit client
    let mut client = BybitClient::new(
        bybit_config,
        1000, // Buffer size
        Some(handler),
        pairs,
    );

    // Spawn the client in a background task
    // Note: orderbooks() runs forever with reconnection, so it will never return normally
    let _client_handle = {
        tokio::spawn(async move {
            if let Err(e) = client.orderbooks().await {
                error!("Bybit client error: {:?}", e);
            }
        })
    };

    // Listen for Ctrl-C
    info!("Listening for orderbook updates... (Press Ctrl-C to stop)");

    signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    info!("Ctrl-C received, shutting down...");

    Ok(())
}
