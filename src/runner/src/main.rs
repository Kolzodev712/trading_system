use core_logic::{data::base_data::Pair, redis::RedisClient, traits::TradingClient};
use exchange_engine::{
    config::{ExchangeConfig, ExchangeEngineConfig},
    exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient},
    Exchange,
};
use market_data_engine::{
    config::{ExchangeConfig as MDExchangeConfig, MarketDataConfig},
    driver::{MarketDataDriver, Method},
};
use rust_decimal::Decimal;
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};
use trading_engine::{executor::StrategyExecutor, strategies::BuyLowSellHigh, strategy::Strategy};

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;
    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Trading System");

    // Load configuration
    let binance_config = load_binance_config("src/exchange_engine/config/binance_config.json")?;
    let symbol = binance_config.symbol.clone();

    // 1. Start Market Data Engine (publishes to Redis)
    info!("Starting Market Data Engine...");
    let market_data_config = MarketDataConfig {
        redis_url: "redis://127.0.0.1/".into(),
        exchange: Exchange::Binance,
        exchange_config: MDExchangeConfig::Binance(binance_config.clone()),
    };
    let mut market_data_driver = MarketDataDriver::new(market_data_config)?;

    // Spawn market data engine in background
    let market_data_handle = tokio::spawn(async move {
        if let Err(e) = market_data_driver.run(Method::Orderbooks).await {
            error!("Market data engine error: {e}");
        }
    });

    // Wait a bit for market data to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 2. Create Trading Client (Exchange Engine)
    info!("Creating Trading Client...");
    let binance_client = BinanceClient::new(binance_config.clone(), 1024);
    
    // Set up execution report channel before wrapping in trait
    let (exec_report_tx, mut exec_report_rx) = tokio::sync::mpsc::channel(100);
    binance_client.set_execution_report_channel(exec_report_tx).await;
    
    let trading_client: Arc<
        RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
    > = Arc::new(RwLock::new(binance_client.clone()));

    // Spawn WebSocket stream for trading client
    let trading_client_clone = binance_client.clone();
    let trading_ws_handle = tokio::spawn(async move {
        if let Err(e) = trading_client_clone.stream().await {
            error!("Trading WebSocket error: {e}");
        }
    });

    // Wait for trading client to be ready
    loop {
        if trading_client
            .read()
            .await
            .get_connection_session()
            .await
            .is_some()
        {
            info!("Trading client ready");
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // 3. Create Strategy
    info!("Creating Buy Low Sell High Strategy...");
    let position_size = Decimal::new(1, 3); // 0.001 BTC per trade
    let max_position = Decimal::new(10, 3); // Max 0.01 BTC total position
    let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(
        trading_client.clone(),
        symbol.clone(),
        position_size,
        max_position,
    )));

    // 4. Create Strategy Executor
    info!("Creating Strategy Executor...");
    let executor = StrategyExecutor::new(
        strategy.clone(),
        trading_client.clone(),
        "redis://127.0.0.1/".into(),
        symbol.clone(),
    );

    // 5. Set up execution report handler
    let strategy_for_reports = Arc::clone(&strategy);
    let execution_report_handle = tokio::spawn(async move {
        while let Some(report) = exec_report_rx.recv().await {
            let order: core_logic::data::private_data::Order = report.into();
            let mut strat = strategy_for_reports.write().await;
            if let Err(e) = strat
                .on_execution_report(&order, strat.get_state_mut())
                .await
            {
                error!("Error handling execution report: {e}");
            }
        }
    });

    info!("All systems ready! Starting strategy execution...");
    info!("Strategy: Buy Low Sell High");
    info!("Position size: {position_size} BTC");
    info!("Max position: {max_position} BTC");
    info!("Symbol: {symbol}");

    // 6. Run strategy executor (this blocks)
    if let Err(e) = executor.run().await {
        error!("Strategy executor error: {e}");
    }

    // Cleanup
    info!("Shutting down...");
    market_data_handle.abort();
    trading_ws_handle.abort();
    execution_report_handle.abort();

    Ok(())
}
