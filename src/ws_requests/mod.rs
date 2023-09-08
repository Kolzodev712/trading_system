pub mod exchanges_ws;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

/// Constructs a WebSocket URL and makes a WebSocket request to the constructed URL.
///
/// This function builds a WebSocket URL using provided parameters, initiates a WebSocket
/// connection, and then handles incoming messages from the connection.
///
/// # Parameters
/// - `ws_base_url`: The base URL for the WebSocket connection.
/// - `endpoint`: The specific WebSocket endpoint.
/// - `base`: The base trading pair (will be converted to lowercase).
/// - `quote`: The quote trading pair (will be converted to lowercase).
///
/// # Examples
///
/// ```
/// make_ws_request("wss://stream.binance.com:9443", "ws", "BNB", "BTC").await;
/// ```
pub async fn make_ws_request(
    ws_base_url: &str,
    endpoint: &str,
    base: &str,
    quote: &str,
    method: &str,
) {
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

    // Handle incoming messages from the established WebSocket connection.
    handle_messages(ws_stream).await;
}

/// Handles incoming messages from a WebSocket connection.
///
/// This function processes different types of WebSocket messages, including text and ping.
/// Text messages are printed to the console, while ping messages are responded to with pongs.
///
/// # Parameters
/// - `ws_stream`: The established WebSocket connection stream.
pub async fn handle_messages(
    mut ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
) {
    // Continuously listen for messages from the WebSocket stream.
    while let Some(message) = ws_stream.next().await {
        match message {
            // If a text message is received, print it to the console.
            Ok(Message::Text(text)) => {
                println!("Received: {}", text);
            }
            // If a ping message is received, respond with a pong.
            Ok(Message::Ping(ping)) => {
                ws_stream
                    .send(Message::Pong(ping))
                    .await
                    .expect("Failed to send Pong");
            }
            // If there's an error while receiving a message, print an error message to the console.
            Err(e) => {
                eprintln!("Error while receiving a message: {}", e);
            }
            // For all other message types, do nothing.
            _ => {}
        }
    }
}
