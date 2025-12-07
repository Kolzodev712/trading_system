//! Generic WebSocket client infrastructure
use crate::error::EngineError;
use async_trait::async_trait;
use smol_str::SmolStr;
use tokio::{net::TcpStream, sync::mpsc::Sender};
use tokio_tungstenite::{
    tungstenite::protocol::{CloseFrame, Message},
    MaybeTlsStream, WebSocketStream,
};

/// Trait for handling WebSocket messages
pub trait MessageHandler<T> {
    /// Handles a text message received from the WebSocket stream
    fn handle_text_message(&self, text: SmolStr) -> Result<T, EngineError>;

    /// Handles a binary message received from the WebSocket stream
    fn handle_binary(&self, binary: Vec<u8>);

    /// Handles a ping message received from the WebSocket stream
    fn handle_ping(&self, ping: Vec<u8>);

    /// Handles a pong message sent to the WebSocket stream
    fn handle_pong(&self, pong: Vec<u8>);

    /// Handles a close message received from the WebSocket stream
    fn handle_close(&self, close: Option<CloseFrame>);
}

/// Trait for creating WebSocket subscription requests
pub trait SubscriptionHandler {
    /// Creates a WebSocket subscription request message
    fn create_subscription_request(&self) -> Option<Message>;
}

/// Trait representing a WebSocket client for different exchanges
#[async_trait]
pub trait WebSocketClient<T> {
    /// Establishes a WebSocket connection to the given URL
    async fn make_ws_request(
        &self,
        ws_url: &str,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError>;

    /// Handles messages from the WebSocket stream
    async fn handle_messages<H>(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        tx: Sender<T>,
        handler: H,
    ) -> Result<(), EngineError>
    where
        H: MessageHandler<T> + Send + Sync + 'static;

    /// Subscribes to WebSocket channels using the provided subscription handler
    async fn subscribe_to_channels<S>(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        subscription_handler: S,
    ) -> Result<(), EngineError>
    where
        S: SubscriptionHandler + Send + Sync + 'static;
}
