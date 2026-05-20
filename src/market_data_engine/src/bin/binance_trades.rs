//! Test binary: Stream Binance public trade updates
//!
//! This binary connects to Binance WebSocket and streams public trade updates
//! for the configured pairs, printing them to stdout.
//!
//! Usage:
//!   cargo run -p market_data_engine --bin binance_trades
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
    error::EngineError, types::market_data::StructuredPublicTrade,
    venue::cex::traits::MarketDataClient,
};

/// Simple message handler that prints trade updates
#[derive(Debug)]
struct PrintTradeHandler;

#[async_trait::async_trait]
impl WSMessageHandler for PrintTradeHandler {
    async fn handle_orderbook(
        &self,
        _orderbook: trading_core::types::market_data::StructuredOrderbook,
    ) -> Result<(), EngineError> {
        // Ignore orderbooks
        Ok(())
    }

    async fn handle_trade(&self, trade: StructuredPublicTrade) -> Result<(), EngineError> {
        let side = if trade.is_market_maker { "SELL" } else { "BUY" };
        let trade_id_str = trade
            .trade_id
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or("N/A");

        println!(
            "[{}] Trade - {}: {} {} @ {} (ID: {}, timestamp: {})",
            chrono::Utc::now().format("%H:%M:%S%.3f"),
            trade.pair,
            side,
            trade.quantity,
            trade.price,
            trade_id_str,
            trade.timestamp
        );

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

    info!("Starting Binance trade stream for pairs: {:?}", pairs);

    // Create message handler
    let handler = Box::new(PrintTradeHandler);

    // Create Binance client
    let mut client = BinanceClient::new(
        binance_config,
        1000, // Buffer size
        Some(handler),
        pairs,
    );

    // Spawn the client in a background task
    // Note: trades() runs forever with reconnection, so it will never return normally
    let _client_handle = {
        tokio::spawn(async move {
            if let Err(e) = client.trades().await {
                error!("Binance client error: {:?}", e);
            }
        })
    };

    // Listen for Ctrl-C
    info!("Listening for trade updates... (Press Ctrl-C to stop)");

    signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    info!("Ctrl-C received, shutting down...");

    Ok(())
}
