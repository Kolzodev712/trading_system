pub mod events;
pub mod execution_report;
pub mod outbound_account_position;

use super::user_data::events::ParsedBinanceMessage;
use core_logic::{error::EngineError, ws_client::MessageHandler};
use smol_str::SmolStr;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct DataHandler;

impl MessageHandler<ParsedBinanceMessage> for DataHandler {
    fn handle_text_message(&self, text: SmolStr) -> Result<ParsedBinanceMessage, EngineError> {
        serde_json::from_str::<ParsedBinanceMessage>(&text).map_err(EngineError::JsonError)
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        debug!("Received binary message: {:?}", binary);
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        debug!("Ping received: {:?}", ping);
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        debug!("Pong received: {:?}", pong);
    }

    fn handle_close(&self, _close: Option<CloseFrame>) {
        // Optional: implement graceful shutdown logic
    }
}
