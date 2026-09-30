//! Example: Fetch account balances from k-paper
//!
//! Connects to k-paper WebSocket, waits for initial balance snapshot, then prints balances.
//!
//! Usage:
//!   cargo run -p exchange_engine --bin example_k_paper_balances
//!   # Loads config from config/bot.json (exchange_engine.exchange = "Paper")
//!
//! Ensure k-paper server is running (e.g. docker-compose up) and config/bot.json has
//! paper_exchange_url (e.g. ws://127.0.0.1:19999/ws).

use exchange_engine::exchanges::k_paper::{
    client::{run_websocket_loop, KPaperClient, KPaperConfig, KPaperWebsocketLoop},
    PaperConfig,
};
use std::{fs, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};
use trading_core::{
    config::ChannelConfig, registry::symbol::SymbolRegistry, types::exchange::OrderRequest,
    venue::cex::channels::cex_venue_update_channel, venue::cex::traits::TradingClient,
};

fn load_paper_config() -> Result<PaperConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string("config/bot.json")?;
    let bot_config: serde_json::Value = serde_json::from_str(&config_data)?;

    let exchange_engine = bot_config
        .get("exchange_engine")
        .ok_or("Missing exchange_engine in config")?;

    let exchange = exchange_engine
        .get("exchange")
        .and_then(|v| v.as_str())
        .ok_or("Missing exchange field")?;

    if exchange != "Paper" {
        return Err("Exchange must be Paper".into());
    }

    let config_json = exchange_engine
        .get("config")
        .ok_or("Missing config in exchange_engine")?;

    let paper_config: PaperConfig = serde_json::from_value(config_json.clone())?;
    Ok(paper_config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let paper_config = load_paper_config()?;
    let kpaper_config = KPaperConfig::from(paper_config.clone());
    let pairs = paper_config.pairs.clone();

    let (client, command_rx) = KPaperClient::new(kpaper_config.clone());
    let state = client.state();
    let trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
        Arc::new(RwLock::new(client));

    let channels = ChannelConfig::default();
    let (exchange_update_tx, _rx) = cex_venue_update_channel(channels.cex_venue_update_capacity);
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(pairs.clone()));

    let state_manager = Arc::new(exchange_engine::state::manager::ExchangeStateManager::new(
        exchange_update_tx,
        symbol_registry,
        trading_client.clone(),
        0,
        false,
    ));

    let (balance_ready_tx, balance_ready_rx) = tokio::sync::oneshot::channel();
    let balance_ready = Some(Arc::new(tokio::sync::Mutex::new(Some(balance_ready_tx))));

    let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);

    info!("Connecting to k-paper at {}...", kpaper_config.url);

    let loop_handle = tokio::spawn(async move {
        run_websocket_loop(KPaperWebsocketLoop {
            config: kpaper_config,
            command_rx,
            state,
            market_data_tx: None,
            exchange_state_manager: state_manager,
            shutdown_rx,
            balance_ready,
            connection_state: None,
        })
        .await
    });

    match tokio::time::timeout(tokio::time::Duration::from_secs(15), balance_ready_rx).await {
        Ok(Ok(())) => info!("Received initial balance snapshot"),
        Ok(Err(_)) => {}
        Err(_) => {
            info!("Timeout waiting for balance snapshot, fetching anyway...");
        }
    }

    {
        let client = trading_client.read().await;
        match client.fetch_balance().await {
            Ok(balances) => {
                info!("Balances ({}):", balances.len());
                for b in balances {
                    info!(
                        "  {}: free={} used={} total={}",
                        b.symbol, b.free, b.used, b.total
                    );
                }
            }
            Err(e) => error!("Failed to fetch balances: {}", e),
        }
    }

    drop(loop_handle);
    Ok(())
}
