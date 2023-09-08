mod parser;
mod rest_requests;
mod ws_requests;

use rest_requests::exchanges::binance::{
    get_binance_active_pairs, get_binance_orderbooks, get_binance_trades,
};
use ws_requests::exchanges_ws::binance_ws::{
    binance_orderbook_ws_stream, binance_trades_ws_stream,
};

#[tokio::main]
async fn main() {
    match get_binance_active_pairs().await {
        Ok(_result) => (),
        Err(error) => println!("Error: {}", error),
    }

    match get_binance_trades("BTC", "USDT").await {
        Ok(_result) => (),
        Err(error) => println!("Error: {}", error),
    }

    match get_binance_orderbooks("BTC", "USDT").await {
        Ok(_result) => (),
        Err(error) => println!("Error: {}", error),
    }

    let _obs = binance_orderbook_ws_stream("BTC", "USDT").await;
    let _trades = binance_trades_ws_stream("BTC", "USDT").await;
}
