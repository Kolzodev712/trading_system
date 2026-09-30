//! Example: Cancel all orders for a pair on k-paper
//!
//! Connects to k-paper, waits for balance snapshot, places one or two limit orders for the first
//! configured pair, then calls cancel_all_orders for that pair. Requires k-paper to send
//! Execution acks and CancelAcks.
//!
//! Usage:
//!   cargo run -p exchange_engine --bin example_k_paper_cancel_all
//!   # Loads config from config/bot.json (exchange_engine.exchange = "Paper")
//!
//! Ensure k-paper is running and config/bot.json has paper_exchange_url and pairs.

use exchange_engine::exchanges::k_paper::{
    client::{run_websocket_loop, KPaperClient, KPaperConfig, KPaperWebsocketLoop},
    PaperConfig,
};
use rust_decimal::Decimal;
use std::{fs, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};
use trading_core::{
    config::ChannelConfig,
    registry::symbol::SymbolRegistry,
    types::{
        common::{OrderType, Side},
        exchange::OrderRequest,
    },
    venue::cex::channels::cex_venue_update_channel,
    venue::cex::traits::TradingClient,
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
        Ok(Ok(())) => info!("Balance snapshot received"),
        Ok(Err(_)) => {}
        Err(_) => info!("Timeout waiting for balance snapshot, continuing..."),
    }

    let test_pair = pairs.first().ok_or("No pairs in config")?;
    let price1 = Decimal::new(50000, 0);
    let price2 = Decimal::new(50100, 0);
    let test_qty = Decimal::new(2, 3); // 0.002

    let req1 = OrderRequest {
        instrument: trading_core::types::common::Instrument::new(
            paper_config.instrument_type.clone(),
            test_pair.clone(),
        ),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: price1,
        amount: test_qty,
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    let req2 = OrderRequest {
        instrument: trading_core::types::common::Instrument::new(
            paper_config.instrument_type.clone(),
            test_pair.clone(),
        ),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: price2,
        amount: test_qty,
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    info!(
        "Placing two limit orders: {} Buy @ {} and @ {}",
        test_pair, price1, price2
    );

    {
        let client = trading_client.read().await;
        if let Err(e) = client.open_order(req1).await {
            error!("Place first order failed: {}", e);
        } else {
            info!("First order placed");
        }
    }
    {
        let client = trading_client.read().await;
        if let Err(e) = client.open_order(req2).await {
            error!("Place second order failed: {}", e);
        } else {
            info!("Second order placed");
        }
    }

    info!("Cancelling all orders for pair {}", test_pair);
    {
        let client = trading_client.read().await;
        match client.cancel_all_orders(test_pair).await {
            Ok(()) => info!("Cancel-all completed"),
            Err(e) => error!("Cancel-all failed: {}", e),
        }
    }

    drop(loop_handle);
    Ok(())
}
