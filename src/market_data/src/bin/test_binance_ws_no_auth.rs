use core_logic::error::KolzoError;
use market_data::exchanges::binance::websocket::{
    binance_ws::{BinanceChannelPublic, BinanceWSClient},
    types::ParsedBinanceMessage,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), KolzoError> {
    let (tx, mut rx) = mpsc::channel(100);
    let base = "btc";
    let quote = "usdt";

    let trade_channel_info = BinanceChannelPublic::Trade {
        base: base.to_string(),
        quote: quote.to_string(),
    };

    let client = BinanceWSClient::new_public(trade_channel_info, None);
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
            _ => {}
        }
    }

    Ok(())
}
