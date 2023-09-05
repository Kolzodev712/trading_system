mod parser;
mod rest_requests;

use rest_requests::exchanges::binance::{
    get_binance_active_pairs, get_binance_orderbooks, get_binance_trades,
};

#[tokio::main]
async fn main() {
    match get_binance_active_pairs().await {
        Ok(_result) => println!("OK"),
        Err(error) => println!("Error: {}", error),
    }

    match get_binance_trades("BTC", "USDT").await {
        Ok(_result) => println!("OK"),
        Err(error) => println!("Error: {}", error),
    }

    match get_binance_orderbooks("BTC", "USDT").await {
        Ok(result) => println!("{:?}", result),
        Err(error) => println!("Error: {}", error),
    }
}
