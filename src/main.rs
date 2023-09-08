mod parser;
mod rest_requests;
mod threads;
mod ws_requests;

use async_trait::async_trait;
use rest_requests::exchanges::binance::{
    get_binance_active_pairs, get_binance_orderbooks, get_binance_trades,
};

use threads::{AsyncConsume, AsyncProduce, AsyncProducerConsumer};
use ws_requests::exchanges_ws::binance_ws::{
    binance_orderbook_ws_stream, binance_trades_ws_stream,
};

struct Producer;
struct Consumer;

#[async_trait]
impl AsyncProduce<()> for Producer {
    async fn produce(&self, tx: tokio::sync::mpsc::Sender<()>) {
        let _obs = binance_orderbook_ws_stream("BTC", "USDT").await;
        if let Err(e) = tx.send(_obs).await {
            println!("Failed to send {:?}: {}", _obs, e);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

#[async_trait]
impl AsyncConsume<()> for Consumer {
    async fn consume(&self, mut rx: tokio::sync::mpsc::Receiver<()>) {
        while let Some(val) = rx.recv().await {
            println!("Received: {:?}", val); // investigate why this is not printing. Maybe the data does not go into the channel as expected.
        }
    }
}

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

    let ob_channel = AsyncProducerConsumer::new(Producer, Consumer);
    ob_channel.join_tasks().await;
}
