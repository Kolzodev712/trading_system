use crate::error::KolzoError;
use async_trait::async_trait;
use tokio::{net::TcpStream, sync::mpsc::Sender};
use tokio_tungstenite::{tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream};

/// Trait for handling WebSocket messages.
pub trait MessageHandler<T> {
    /// Handles a text message received from the WebSocket stream.
    fn handle_text_message(&self, text: String) -> Result<T, KolzoError>;

    /// Handles a ping message received from the WebSocket stream.
    fn handle_ping(&self, ping: Vec<u8>);
}

/// Trait for creating WebSocket subscription requests.
pub trait SubscriptionHandler {
    /// Creates a WebSocket subscription request message.
    fn create_subscription_request(&self) -> Message;
}

#[async_trait]
/// Trait representing a WebSocket client for different exchanges.
pub trait WebSocketClient<T> {
    /// Establishes a WebSocket connection to the given URL.
    async fn make_ws_request(
        &self,
        ws_url: &str,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, KolzoError>;

    /// Handles messages from the WebSocket stream.
    async fn handle_messages<H>(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        tx: Sender<T>,
        handler: H,
    ) -> Result<(), KolzoError>
    where
        H: MessageHandler<T> + Send + Sync + 'static;

    /// Subscribes to WebSocket channels using the provided subscription handler.
    async fn subscribe_to_channels<S>(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        subscription_handler: S,
    ) -> Result<(), KolzoError>
    where
        S: SubscriptionHandler + Send + Sync + 'static;
}
