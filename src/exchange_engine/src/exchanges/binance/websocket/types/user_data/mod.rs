pub mod events;
pub mod execution_report;
pub mod outbound_account_position;

use super::user_data::{
    events::{BinanceWsEventType, ParsedBinanceMessage},
    execution_report::ExecutionReport,
    outbound_account_position::{BalanceUpdate, OutboundAccountPosition},
};
use core_logic::{error::EngineError, ws_client::MessageHandler};
use serde_json::Value;
use smol_str::SmolStr;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tracing::{debug, error, trace};

#[derive(Debug, Clone)]
pub struct DataHandler;

impl DataHandler {
    fn handle_outbound_account_position(
        &self,
        v: Value,
    ) -> Result<ParsedBinanceMessage, EngineError> {
        let outbound_account_position: OutboundAccountPosition = serde_json::from_value(v)
            .map_err(|e| {
                error!("Error deserializing account info: {}", e);
                EngineError::JsonError(e)
            })?;

        Ok(ParsedBinanceMessage::OutboundAccountPosition(
            outbound_account_position,
        ))
    }

    fn handle_balance_update(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let balance_update: BalanceUpdate =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::BalanceUpdate(balance_update))
    }

    fn handle_execution_report(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let execution_report: ExecutionReport =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::ExecutionReport(execution_report))
    }

    fn convert_method_to_event_type(&self, method: &str) -> BinanceWsEventType {
        match method {
            "outboundAccountPosition" => BinanceWsEventType::OutboundAccountPosition,
            "balanceUpdate" => BinanceWsEventType::BalanceUpdate,
            "executionReport" => BinanceWsEventType::ExecutionReport,
            _ => BinanceWsEventType::Unknown,
        }
    }

    fn handle_event(&self, method: &str, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let event_type = self.convert_method_to_event_type(method);

        match event_type {
            BinanceWsEventType::OutboundAccountPosition => self.handle_outbound_account_position(v),
            BinanceWsEventType::BalanceUpdate => self.handle_balance_update(v),
            BinanceWsEventType::ExecutionReport => self.handle_execution_report(v),
            BinanceWsEventType::Unknown => {
                debug!("Unknown event type: {:?}", v);
                Err(EngineError::new_custom_error(
                    "Could not find relevant Binance WS message type".into(),
                ))
            }
        }
    }
}

impl MessageHandler<ParsedBinanceMessage> for DataHandler {
    fn handle_text_message(&self, text: SmolStr) -> Result<ParsedBinanceMessage, EngineError> {
        let v: Value = serde_json::from_str(&text).map_err(EngineError::JsonError)?;

        if let Some(event_type) = v.clone().get("e").and_then(|e| e.as_str()) {
            self.handle_event(event_type, v)
        } else {
            trace!("Unknown or unsupported message format: {}", text);
            Ok(ParsedBinanceMessage::Raw(text))
        }
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        debug!("Received binary message: {:?}", binary)
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        debug!("Ping received: {:?}", ping);
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        debug!("Seding Pong: {:?}", pong);
    }

    fn handle_close(&self, _close: Option<CloseFrame>) {
        unimplemented!()
    }
}
