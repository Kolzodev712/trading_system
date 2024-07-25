use core_logic::data::DataType;
use core_logic::error::KolzoError;
use market_data::exchanges::binance::websocket::{
    binance_ws::{BinanceChannel, BinanceWSClient},
    types::ParsedBinanceMessage,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), KolzoError> {
    let (tx, mut rx) = mpsc::channel(100);
    let ws_url = "wss://stream.binance.com:9443/ws";
    let base = "btc";
    let quote = "usdt";

    let trade_channel_info = BinanceChannel {
        channel: DataType::Trade,
        base: base.to_string(),
        quote: quote.to_string(),
    };

    let client = BinanceWSClient::new(ws_url, trade_channel_info);
    client.run(tx).await?;

    while let Some(message) = rx.recv().await {
        match message {
            ParsedBinanceMessage::OrderBook(orderbook) => {
                println!("Received OrderBook: {:?}", orderbook)
            }
            ParsedBinanceMessage::Trade(trade) => println!("Received Trade: {:?}", trade),
            ParsedBinanceMessage::Raw(raw) => println!("Received Raw message: {}", raw),
        }
    }

    Ok(())
}
