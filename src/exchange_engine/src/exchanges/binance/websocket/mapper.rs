//! WebSocket message mapping and parsing
//!
//! Handles parsing raw WebSocket messages into typed Binance structures

use super::types::{
    common::BinanceInbound,
    rpc::{
        balance::BinanceAccountStatusResponse, cancel_all::BinanceWsCancelAllOrdersResponse,
        cancel_order::BinanceWsCancelOrderResponse, open_order::BinanceWsOrderResponse,
        BinanceRpcResponse,
    },
    user_data::events::ParsedBinanceMessage,
};
use smol_str::SmolStr;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tracing::debug;
use trading_core::{error::EngineError, websocket::client::MessageHandler};

#[derive(Debug, Clone)]
pub struct BinanceMessageMapper;

impl MessageHandler<BinanceInbound> for BinanceMessageMapper {
    fn handle_text_message(&self, text: SmolStr) -> Result<BinanceInbound, EngineError> {
        // Try parsing as RPC responses first
        if let Ok(resp) = serde_json::from_str::<BinanceAccountStatusResponse>(&text) {
            tracing::debug!("[BinanceMessageMapper] Parsed as AccountStatus RPC response");
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::AccountStatus(resp),
            )));
        }

        // Try CancelAll first (has array result) before OpenOrder (has object result)
        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelAllOrdersResponse>(&text) {
            tracing::info!(
                "[BinanceMessageMapper] Successfully parsed as CancelAll RPC response (id={}, status={}, cancelled {} orders)",
                resp.id,
                resp.status,
                resp.result.len()
            );
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::CancelAll(resp),
            )));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelOrderResponse>(&text) {
            if resp.status == 200 {
                tracing::debug!(
                    "[BinanceMessageMapper] Parsed as CancelOrder RPC response (id={}, status={})",
                    resp.id,
                    resp.status
                );
            } else {
                tracing::info!(
                    "[BinanceMessageMapper] Parsed as CancelOrder RPC error response (id={}, status={}, error={:?})",
                    resp.id,
                    resp.status,
                    resp.error
                );
            }
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::CancelOrder(resp),
            )));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsOrderResponse>(&text) {
            tracing::info!(
                "[BinanceMessageMapper] Successfully parsed as OpenOrder RPC response (id={}, status={})",
                resp.id,
                resp.status
            );
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::OpenOrder(resp),
            )));
        } else if text.contains("\"id\"")
            && text.contains("\"status\"")
            && text.contains("\"result\"")
        {
            // This looks like an RPC response but failed to parse - log the error
            if let Err(e) = serde_json::from_str::<BinanceWsOrderResponse>(&text) {
                tracing::warn!(
                    "[BinanceMessageMapper] Failed to parse as BinanceWsOrderResponse: {}, text: {}",
                    e,
                    text
                );
            }
        }

        // Fall back to parsing as event
        let event = serde_json::from_str::<ParsedBinanceMessage>(&text)
            .map(BinanceInbound::Event)
            .unwrap_or(BinanceInbound::Event(ParsedBinanceMessage::Raw(
                text.clone(),
            )));
        Ok(event)
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        debug!(?binary, "ws binary message received");
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        debug!(?ping, "ws ping received");
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        debug!(?pong, "ws pong received");
    }

    fn handle_close(&self, close: Option<CloseFrame>) {
        debug!(?close, "ws close received");
    }
}
