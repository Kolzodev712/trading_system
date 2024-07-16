use super::error::KolzoError;
use futures_util::{stream::StreamExt, SinkExt};
use tokio::{net::TcpStream, sync::mpsc::Sender};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::error;

/// Trait for handling WebSocket messages.
///
/// This trait provides methods for handling different types of messages
/// received from a WebSocket stream. Implement this trait for custom
/// message handling logic for different exchanges.
pub trait MessageHandler {
    /// Handles text messages received from the WebSocket stream.
    ///
    /// # Arguments
    ///
    /// * `text` - The text message received from the WebSocket stream.
    fn handle_text_message(&self, text: String);

    /// Handles ping messages received from the WebSocket stream.
    ///
    /// # Arguments
    ///
    /// * `ping` - The ping message received from the WebSocket stream.
    fn handle_ping(&self, ping: Vec<u8>);
}

/// Establishes a WebSocket connection to the given URL.
///
/// This function connects to the WebSocket server at the specified URL and
/// returns the WebSocket stream if the connection is successful.
///
/// # Arguments
///
/// * `ws_url` - The WebSocket URL to connect to.
///
/// # Returns
///
/// * `Ok(WebSocketStream<MaybeTlsStream<TcpStream>>)` - The WebSocket stream if the connection is successful.
/// * `Err(KolzoError)` - An error if the connection fails.
///
/// # Errors
///
/// This function returns an error if the WebSocket connection could not be established.
pub async fn make_ws_request(
    ws_url: &str,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, KolzoError> {
    // Establish a WebSocket connection to the constructed URL.
    let (ws_stream, _) = connect_async(ws_url).await?;
    // Return the WS stream
    Ok(ws_stream)
}

/// Handles messages from the WebSocket stream.
///
/// This function continuously listens for messages from the WebSocket stream
/// and processes them using the provided message handler. Text messages are
/// also sent through the provided channel.
///
/// # Arguments
///
/// * `ws_stream` - The WebSocket stream to listen for messages.
/// * `tx` - The channel to send text messages through.
/// * `handler` - The message handler to process different types of messages.
///
/// # Returns
///
/// * `Ok(())` - If the messages were handled successfully.
/// * `Err(KolzoError)` - An error if there was a problem handling the messages.
///
/// # Errors
///
/// This function returns an error if there was a problem receiving or sending messages.
pub async fn handle_messages<H>(
    mut ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    tx: Sender<String>,
    handler: H,
) -> Result<(), KolzoError>
where
    H: MessageHandler,
{
    // Continuously listen for messages from the WebSocket stream.
    while let Some(message) = ws_stream.next().await {
        match message {
            // If a text message is received, send it through the channel.
            Ok(Message::Text(text)) => {
                handler.handle_text_message(text.clone());
                tx.send(text).await.map_err(KolzoError::ChannelSendError)?;
            }
            // Handle other message types as needed...
            Ok(Message::Ping(ping)) => {
                handler.handle_ping(ping.clone());
                ws_stream.send(Message::Pong(ping)).await?;
            }
            Err(e) => {
                error!("Error while receiving a message: {}", e);
                return Err(KolzoError::WsConnectionError(e));
            }
            _ => {}
        }
    }
    Ok(())
}
