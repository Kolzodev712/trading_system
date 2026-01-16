//! Test binary: Stream Bybit public trade updates
//!
//! This binary connects to Bybit WebSocket and streams public trade updates
//! for the configured pairs, printing them to stdout.
//!
//! Usage:
//!   cargo run -p market_data_engine --bin bybit_trades
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
    error::EngineError, traits::MarketDataClient, types::market_data::StructuredPublicTrade,
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
        // Extract side from trade_id (format: "Buy:ID" or "Sell:ID" or just "Buy"/"Sell")
        let (side, trade_id_str) = trade
            .trade_id
            .as_ref()
            .map(|id| {
                let id_str = id.as_str();
                if let Some((side_part, id_part)) = id_str.split_once(':') {
                    (side_part.to_uppercase(), id_part.to_string())
                } else {
                    // No ID, just side
                    (id_str.to_uppercase(), "N/A".to_string())
                }
            })
            .unwrap_or(("UNKNOWN".to_string(), "N/A".to_string()));

        println!(
            "[{}] Trade - {}: {} {} @ {} (ID: {}, MM: {})",
            chrono::Utc::now().format("%H:%M:%S%.3f"),
            trade.pair,
            side,
            trade.quantity,
            trade.price,
            trade_id_str,
            trade.is_market_maker
        );

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

    info!("Starting Bybit trade stream for pairs: {:?}", pairs);

    // Create message handler
    let handler = Box::new(PrintTradeHandler);

    // Create Bybit client
    let mut client = BybitClient::new(
        bybit_config,
        1000, // Buffer size
        Some(handler),
        pairs,
    );

    // Spawn the client in a background task
    // Note: trades() runs forever with reconnection, so it will never return normally
    let _client_handle = {
        tokio::spawn(async move {
            if let Err(e) = client.trades().await {
                error!("Bybit client error: {:?}", e);
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
