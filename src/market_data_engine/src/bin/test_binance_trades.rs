use core_logic::{data::base_data::DataType, error::EngineError};
use market_data_engine::exchanges::binance::websocket::{
    binance_ws::{BinanceChannel, BinanceWSClient},
    types::ParsedBinanceMessage,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), EngineError> {
    let (tx, mut rx) = mpsc::channel(100);
    let base = "btc";
    let quote = "usdt";

    let trade_channel_info = BinanceChannel {
        channel: DataType::Trade,
        base: base.into(),
        quote: quote.into(),
    };

    let client = BinanceWSClient::new(trade_channel_info);
    client.run(tx).await?;

    while let Some(message) = rx.recv().await {
        match message {
            ParsedBinanceMessage::Trade(trade) => {
                println!("Received Trade: {:?}", trade);
            }
            ParsedBinanceMessage::OrderBook(orderbook) => {
                println!("Received OrderBook: {:?}", orderbook);
            }
            ParsedBinanceMessage::Raw(raw) => {
                println!("Received Raw message: {}", raw);
            }
        }
    }

    Ok(())
}
