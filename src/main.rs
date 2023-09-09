mod parser;
mod rest_requests;
mod threads;
mod ws_requests;

use async_trait::async_trait;
use rest_requests::exchanges::binance::{
    get_binance_active_pairs, get_binance_orderbooks, get_binance_trades,
};

use threads::{AsyncConsume, AsyncProduce, AsyncProducerConsumer};
use ws_requests::exchanges_ws::binance_ws::binance_ws_stream;
use ws_requests::handle_messages;

struct Producer;
struct Consumer;

#[async_trait]
impl AsyncProduce<String> for Producer {
    async fn produce(&self, tx: tokio::sync::mpsc::Sender<String>) {
        let ob_stream = binance_ws_stream("BTC", "USDT", "kline_1h").await;
        match ob_stream {
            Ok(ob_stream) => {
                if let Err(e) = handle_messages(ob_stream, tx.clone()).await {
                    eprintln!("Error while handling messages: {}", e);
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            Err(error) => {
                eprintln!("Failed to establish WebSocket connection: {}", error);
                // Handle the error as needed...
            }
        }
    }
}

#[async_trait]
impl AsyncConsume<String> for Consumer {
    async fn consume(&self, mut rx: tokio::sync::mpsc::Receiver<String>) {
        while let Some(val) = rx.recv().await {
            println!("Receiving WS data: {:?}", val);
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
