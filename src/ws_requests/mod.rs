pub mod exchanges_ws;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use std::error::Error;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

/// Constructs a WebSocket URL and initiates a WebSocket connection.
///
/// This function builds a WebSocket URL using the provided parameters and initiates a connection
/// to the constructed URL. It returns the established WebSocket stream for further handling.
///
/// # Parameters
/// - `ws_base_url`: The base URL for the WebSocket connection.
/// - `endpoint`: The specific WebSocket endpoint.
/// - `base`: The base trading pair. This will be converted to lowercase in the URL.
/// - `quote`: The quote trading pair. This will be converted to lowercase in the URL.
/// - `method`: The specific method or resource to request (e.g., "depth").
///
/// # Returns
/// A `Result` containing the established WebSocket stream. If the connection fails, it returns
/// an error wrapped in a `Box<dyn Error>`.
///
/// # Examples
///
/// ```no_run
/// let stream = make_ws_request("wss://stream.binance.com:9443", "ws", "BNB", "BTC", "depth").await
///     .expect("Failed to establish WebSocket connection");
/// ```
pub async fn make_ws_request(
    ws_base_url: &str,
    endpoint: &str,
    base: &str,
    quote: &str,
    method: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    Box<dyn Error>,
> {
    // Construct the WebSocket URL using the provided parameters.
    let ws_url = format!(
        "{}/{}/{}{}@{}",
        ws_base_url,
        endpoint,
        base.to_lowercase(),
        quote.to_lowercase(),
        method,
    );

    // Establish a WebSocket connection to the constructed URL.
    let (ws_stream, _) = connect_async(ws_url).await.expect("Error connecting");

    // Return the WS stream
    Ok(ws_stream)
}

/// Continuously processes incoming messages from a WebSocket stream.
///
/// This function listens to messages on the provided `WebSocketStream` and performs actions based on the message type:
/// - For text messages, it forwards them to the provided sender channel.
/// - For ping messages, it responds with a pong.
/// - Errors encountered during message reception are logged and then propagated.
///
/// # Arguments
///
/// * `ws_stream` - The WebSocket stream from which messages are read.
/// * `tx` - The sender channel through which processed text messages are sent.
///
/// # Returns
///
/// This function returns a `Result` that indicates whether the message handling completed successfully or
/// encountered an error. In case of an error, it returns a boxed error.
pub async fn handle_messages(
    mut ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    tx: Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Continuously listen for messages from the WebSocket stream.
    while let Some(message) = ws_stream.next().await {
        match message {
            // If a text message is received, send it through the channel.
            Ok(Message::Text(text)) => {
                tx.send(text).await?;
            }
            // Handle other message types as needed...
            Ok(Message::Ping(ping)) => {
                ws_stream.send(Message::Pong(ping)).await?;
            }
            Err(e) => {
                eprintln!("Error while receiving a message: {}", e);
                return Err(Box::new(e));
            }
            _ => {}
        }
    }
    Ok(())
}
