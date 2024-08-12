use core_logic::error::KolzoError;
use market_data::exchanges::binance::websocket::{
    binance_ws::{BinanceChannelPrivate, BinanceWSClient, BinanceWsAuthenticator},
    types::ParsedBinanceMessage,
};
use std::env;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), KolzoError> {
    let api_key = env::var("BINANCE_API_KEY").expect("BINANCE_API_KEY must be set");
    let secret_key = env::var("BINANCE_SECRET_KEY").expect("BINANCE_SECRET_KEY must be set");

    let account_channel_info = BinanceChannelPrivate::AccountStatus;

    let authenticator = BinanceWsAuthenticator::new(&api_key, &secret_key);

    let client = BinanceWSClient::new_private(account_channel_info, Some(Box::new(authenticator)));

    let (tx, mut rx) = mpsc::channel(100);

    if let Err(e) = client.run(tx).await {
        eprintln!("Error running the WebSocket client: {}", e);
        return Err(e);
    }

    while let Some(message) = rx.recv().await {
        match message {
            ParsedBinanceMessage::Account(account) => {
                println!("Received Account: {:?}", account);
            }
            ParsedBinanceMessage::Raw(raw) => {
                println!("Received Raw message: {}", raw);
            }
            _ => {
                println!("Received other message type.");
            }
        }
    }
    Ok(())
}
