pub mod exchanges_ws;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use std::error::Error;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

/// Establishes a WebSocket connection to the Binance API for a given method.
///
/// Constructs the WebSocket URL using provided parameters, and if `base` and `quote`
/// are given, they are included in the constructed URL. If either of `base` or `quote`
/// are not provided, the constructed URL does not include them.
///
/// # Parameters
/// - `ws_base_url`: The base URL for the WebSocket connection.
/// - `endpoint`: The endpoint to connect to.
/// - `base`: The base cryptocurrency symbol (e.g., "BTC"). This is optional.
/// - `quote`: The quote cryptocurrency symbol (e.g., "USDT"). This is optional.
/// - `method`: The method for the WebSocket stream (e.g., "depth", "trades").
///
/// # Returns
/// Returns a WebSocket stream on success or an error if the connection could not be established.
///
/// # Examples
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ws_stream = make_ws_request(
///     "wss://stream.binance.com:9443",
///     "ws",
///     Some("BTC"),
///     Some("USDT"),
///     "depth"
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn make_ws_request(
    ws_base_url: &str,
    endpoint: &str,
    base: Option<&str>,
    quote: Option<&str>,
    method: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    Box<dyn Error>,
> {
    // Construct the WebSocket URL using the provided parameters.
    let ws_url = if let (Some(base), Some(quote)) = (base, quote) {
        format!(
            "{}/{}/{}{}@{}",
            ws_base_url,
            endpoint,
            base.to_lowercase(),
            quote.to_lowercase(),
            method
        )
    } else {
        format!("{}/{}/@{}", ws_base_url, endpoint, method)
    };

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
