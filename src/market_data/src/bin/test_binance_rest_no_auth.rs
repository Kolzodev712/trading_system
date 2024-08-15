use core_logic::data::{OrderBook, Trade};
use market_data::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{BinanceDataType, BinanceOrderbookSnapshot, BinanceSymbol, BinanceTradeSnapshot},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let symbol = BinanceSymbol::new("BTC", "USDT");

    // Create a handler for the order book
    let orderbook_handler = BinanceExchangeHandler::new_public(
        BinanceDataType::Orderbook,
        symbol.clone(),
        Some(10),
        None,
        None,
    );
    let binance_orderbook: BinanceOrderbookSnapshot = orderbook_handler.get().await?;
    let _orderbook: OrderBook = binance_orderbook.into();
    println!("{:?}", _orderbook);

    let trade_handler =
        BinanceExchangeHandler::new_public(BinanceDataType::Trade, symbol, None, None, None);
    let binance_trades: Vec<BinanceTradeSnapshot> = trade_handler.get().await?;
    let _trades: Vec<Trade> = binance_trades.into_iter().map(Trade::from).collect();
    // println!("{:?}", _trades);

    Ok(())
}
