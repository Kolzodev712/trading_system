use core_logic::traits::PrivateDataClient;
use exchange_engine::exchanges::binance::{
    common_types::{BinanceOrderType, BinanceSide, BinanceSymbol, BinanceTimeInForce},
    config::BinanceConfig,
    rest::client::BinanceHttpClient,
    rest_client::BinanceRestClient,
    websocket::types::rpc::open_order::BinanceWsOrderReqParams,
    ws_client::BinanceClient,
};
use rust_decimal::Decimal;
use std::{fs, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info};

fn load_binance_config(file_path: &str) -> Result<BinanceConfig, Box<dyn std::error::Error>> {
    let config_data = fs::read_to_string(file_path)?;
    let config: BinanceConfig = serde_json::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cfg = load_binance_config("config/binance_config.json")?;
    let cfg_for_rest = cfg.clone();
    let binance_client = Arc::new(RwLock::new(BinanceClient::new(cfg, 100)));

    // -- spawn the WS loop on a *clone* of the client, not on the locked instance --
    let mut ws_client_clone = {
        let r = binance_client.read().await;
        r.clone()
    };
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = ws_client_clone.stream().await {
            error!("WebSocket streaming error: {e}");
        }
    });

    // -- wait until the subscription handshake finished and we have a session --
    loop {
        if binance_client
            .read()
            .await
            .get_connection_session()
            .await
            .is_some()
        {
            info!("WebSocket session ready, proceeding...");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // -- Fetch current market price to use a valid price --
    let rest_client =
        BinanceRestClient::new(BinanceHttpClient::new(cfg_for_rest.clone()), cfg_for_rest);

    let orderbook = match rest_client.fetch_orderbook().await {
        Ok(ob) => ob,
        Err(e) => {
            error!("Failed to fetch orderbook: {e}");
            return Err(format!("Failed to fetch orderbook: {e}").into());
        }
    };

    // Use best bid price (slightly below for buy order) or fallback to 60000
    let market_price = orderbook
        .bids
        .first()
        .map(|bid| bid[0])
        .unwrap_or(Decimal::new(6000000, 2)); // 60,000 USDT fallback

    // Use 1% below market price for buy order to ensure it's valid
    // Round to 2 decimal places for BTC/USDT price filter
    let test_price_raw = market_price * Decimal::new(99, 2) / Decimal::new(100, 2);
    let test_price = (test_price_raw * Decimal::new(100, 0)).round_dp(0) / Decimal::new(100, 0);
    let test_quantity = Decimal::new(1, 3); // 0.001 BTC (small quantity for testing)

    info!(
        "Market price: {market_price}, Using test price: {test_price}, Quantity: {test_quantity}"
    );

    // -- now we can send RPCs freely --
    let params = BinanceWsOrderReqParams::new(
        BinanceSymbol::new("BTC", "USDT").convert_to_smolstr(),
        BinanceSide::Buy,
        BinanceOrderType::Limit,
        BinanceTimeInForce::Gtc,
        test_price,
        test_quantity,
    );

    {
        let client = binance_client.read().await;
        match client.test_order(params).await {
            Ok(_) => info!("Test order sent successfully!"),
            Err(e) => error!("Failed to send test order: {e}"),
        }
    }

    // wait for CTRL-C or whatever
    tokio::signal::ctrl_c().await?;
    // optionally signal the client to shut down cleanly:
    {
        let client = binance_client.read().await;
        client.shutdown();
    }
    // wait for the websocket task to finish
    ws_handle.await?;

    Ok(())
}
