use core_logic::data::{DataType, OrderBook, Trade};
use market_data::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{BinanceOrderbookSnapshot, BinanceSymbol, BinanceTradeSnapshot},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let symbol = BinanceSymbol::new("BTC", "USDT");

    let orderbook_handler =
        BinanceExchangeHandler::new(DataType::Orderbook, symbol.clone(), Some(10));
    let binance_orderbook: BinanceOrderbookSnapshot = orderbook_handler.get().await?;
    let _orderbook: OrderBook = binance_orderbook.into();
    println!("{:?}", _orderbook);

    let trade_handler = BinanceExchangeHandler::new(DataType::Trade, symbol, None);
    let binance_trades: Vec<BinanceTradeSnapshot> = trade_handler.get().await?;
    let _trades: Vec<Trade> = binance_trades.into_iter().map(Trade::from).collect();
    // println!("{:?}", trades);

    Ok(())
}
