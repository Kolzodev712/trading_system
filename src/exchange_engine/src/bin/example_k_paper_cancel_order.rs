//! Example: Cancel an order by ID on k-paper
//!
//! Connects to k-paper, waits for balance snapshot, places one limit order, then cancels it by order_id.
//! Requires k-paper to send Execution ack then CancelAck.
//!
//! Usage:
//!   cargo run -p exchange_engine --bin example_k_paper_cancel_order
//!   # Loads config from config/bot.json (exchange_engine.exchange = "Paper")
//!
//! Ensure k-paper is running and config/bot.json has paper_exchange_url and pairs.

use exchange_engine::exchanges::k_paper::{
    client::{run_websocket_loop, KPaperClient, KPaperConfig},
    PaperConfig,
};
use rust_decimal::Decimal;
use std::{fs, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};
use trading_core::{
    channels::exchange::cex_venue_update_channel,
    config::ChannelConfig,
    registry::symbol::SymbolRegistry,
    traits::TradingClient,
    types::{
        common::{OrderType, Side},
        exchange::OrderRequest,
    },
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
        run_websocket_loop(
            kpaper_config,
            command_rx,
            state,
            None,
            state_manager,
            shutdown_rx,
            balance_ready,
        )
        .await
    });

    match tokio::time::timeout(tokio::time::Duration::from_secs(15), balance_ready_rx).await {
        Ok(Ok(())) => info!("Balance snapshot received"),
        Ok(Err(_)) => {}
        Err(_) => info!("Timeout waiting for balance snapshot, continuing..."),
    }

    let test_pair = pairs.first().ok_or("No pairs in config")?;
    let test_price = Decimal::new(50000, 0);
    let test_qty = Decimal::new(2, 3); // 0.002

    let order_request = OrderRequest {
        instrument: trading_core::types::common::Instrument::new(
            paper_config.instrument_type.clone(),
            test_pair.clone(),
        ),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: test_price,
        amount: test_qty,
        timestamp: Some(trading_core::utils::current_timestamp()),
        exchange_params: std::collections::HashMap::new(),
    };

    info!(
        "Placing limit order: {} {} @ {} (qty={})",
        test_pair, order_request.side, order_request.price, order_request.amount
    );

    let order_id = {
        let client = trading_client.read().await;
        match client.open_order(order_request).await {
            Ok(order) => {
                info!(
                    "Order placed: id={:?} {} {} @ {}",
                    order.order_id, order.instrument.pair, order.side, order.price
                );
                Some(order.order_id)
            }
            Err(e) => {
                error!("Place order failed: {}", e);
                None
            }
        }
    };

    if let Some(oid) = order_id {
        let client = trading_client.read().await;
        info!("Cancelling order {:?}", oid);
        match client.cancel_order_by_id(oid).await {
            Ok(()) => info!("Cancel order succeeded"),
            Err(e) => error!("Cancel order failed: {}", e),
        }
    }

    drop(loop_handle);
    Ok(())
}
