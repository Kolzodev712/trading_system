use core_logic::data::{DataType, OrderBook, Trade};
use market_data::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{BinanceOrderbookSnapshot, BinanceSymbol, BinanceTradeSnapshot},
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("BINANCE_API_KEY").ok();
    let secret_key = env::var("BINANCE_SECRET_KEY").ok();

    let symbol = BinanceSymbol::new("BTC", "USDT");

    // Create a handler for the order book
    let orderbook_handler = BinanceExchangeHandler::new_public(
        DataType::Orderbook,
        symbol.clone(),
        Some(10),
        None,
        None,
        api_key.clone(),
        secret_key.clone(),
    );
    let binance_orderbook: BinanceOrderbookSnapshot = orderbook_handler.get().await?;
    let _orderbook: OrderBook = binance_orderbook.into();
    println!("{:?}", _orderbook);

    let trade_handler = BinanceExchangeHandler::new_public(
        DataType::Trade,
        symbol,
        None,
        None,
        None,
        api_key,
        secret_key,
    );
    let binance_trades: Vec<BinanceTradeSnapshot> = trade_handler.get().await?;
    let _trades: Vec<Trade> = binance_trades.into_iter().map(Trade::from).collect();
    // println!("{:?}", _trades);

    Ok(())
}
