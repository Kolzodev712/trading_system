//! Test binary: Stream Binance orderbook updates
//!
//! This binary connects to Binance WebSocket and streams orderbook updates
//! for the configured pairs, printing them to stdout.
//!
//! Usage:
//!   cargo run -p market_data_engine --bin binance_orderbooks
//!
//! Configuration:
//!   Loads from config/binance_config.json

use market_data_engine::{
    exchanges::binance::{client::BinanceClient, config::BinanceConfig},
    messages::WSMessageHandler,
};
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info};
use trading_core::{
    error::EngineError, traits::MarketDataClient, types::market_data::StructuredOrderbook,
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

fn load_binance_config() -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    // Get config path relative to workspace root
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("src")
        .join("market_data_engine")
        .join("config")
        .join("binance_config.json");

    let config_str = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config from {:?}: {}", config_path, e))?;

    let config: BinanceConfig = serde_json::from_str(&config_str)
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
    let binance_config = load_binance_config()?;
    let pairs = binance_config.pairs.clone();

    info!("Starting Binance orderbook stream for pairs: {:?}", pairs);

    // Create message handler
    let handler = Box::new(PrintOrderbookHandler);

    // Create Binance client
    let mut client = BinanceClient::new(
        binance_config,
        1000, // Buffer size
        Some(handler),
        pairs,
    );

    // Spawn the client in a background task
    // Note: orderbooks() runs forever with reconnection, so it will never return normally
    let _client_handle = {
        tokio::spawn(async move {
            if let Err(e) = client.orderbooks().await {
                error!("Binance client error: {:?}", e);
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
