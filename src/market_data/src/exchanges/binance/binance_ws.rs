use super::types::BinanceWSMessage;
use core_logic::{
    error::KolzoError,
    ws_client::{
        handle_messages, make_ws_request, subscribe_to_channels, MessageHandler,
        SubscriptionHandler,
    },
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug, Clone)]
pub struct BinanceSubscriptionHandler;
#[derive(Debug, Clone)]
struct DataHandler;

impl MessageHandler for DataHandler {
    fn handle_text_message(&self, text: String) {
        println!("Data message: {}", text);
    }

    fn handle_ping(&self, _ping: Vec<u8>) {
        println!("Ping received");
    }
}

impl SubscriptionHandler for BinanceSubscriptionHandler {
    fn create_subscription_request(&self) -> Message {
        let subscription_msg = BinanceWSMessage {
            method: "SUBSCRIBE".into(),
            params: vec!["btcusdt@depth".into()],
            id: 1,
        };
        let json_msg = serde_json::to_string(&subscription_msg).unwrap();
        Message::Text(json_msg)
    }
}

pub struct BinanceWSClient {
    url: String,
    subscription_handler: BinanceSubscriptionHandler,
}

impl BinanceWSClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            subscription_handler: BinanceSubscriptionHandler,
        }
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        KolzoError,
    > {
        make_ws_request(&self.url).await
    }

    pub async fn run(&self, tx: mpsc::Sender<String>) -> Result<(), KolzoError> {
        let mut ws_stream = self.create_ws_stream().await?;
        subscribe_to_channels(&mut ws_stream, self.subscription_handler.clone()).await?;
        tokio::spawn(handle_messages(ws_stream, tx, DataHandler));
        Ok(())
    }
}
