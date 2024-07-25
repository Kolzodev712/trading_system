use market_data::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler, types::BinanceOrderbookSnapshot,
};

#[tokio::main]
async fn main() {
    let binance_url = "https://api.binance.com/api/v3/depth?symbol=BTCUSDT&limit=10";
    let binance_handler = BinanceExchangeHandler::new(binance_url.to_string());

    // Example GET request to Binance API for OrderBookSnapshot
    match binance_handler.get::<BinanceOrderbookSnapshot>().await {
        Ok(order_book) => {
            println!("{:?}", order_book);
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
        }
    }

    // Example GET request to Binance API for another type
    // match binance_handler.get::<AnotherType>().await {
    //     Ok(result) => {
    //         println!("{:?}", result);
    //     }
    //     Err(e) => {
    //         eprintln!("Error: {:?}", e);
    //     }
    // }
}
