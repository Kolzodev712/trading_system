//! Bybit WebSocket API client
//!
//! Unified client for both Spot and Futures WebSocket streams.
//! Trading operations are performed via WebSocket, not REST.

use super::super::config::BybitConfig;
use smol_str::SmolStr;

/// Base URL for Bybit WebSocket API
const BYBIT_WS_URL: &str = "wss://stream.bybit.com/v5/";
const BYBIT_TESTNET_WS_URL: &str = "wss://stream-testnet.bybit.com/v5/";

/// Bybit WebSocket client
///
/// This client handles trading operations via WebSocket.
/// TODO: Implement WebSocket connection and message handling
#[derive(Debug, Clone)]
pub struct BybitWsClient {
    #[allow(dead_code)]
    pub(crate) config: BybitConfig,
    pub(crate) ws_url: SmolStr,
    // TODO: Add WebSocket connection when implementing
    // pub(crate) connection: Option<WebSocketStream>,
}

impl BybitWsClient {
    pub fn new(config: BybitConfig) -> Self {
        let ws_url = if config.testnet {
            BYBIT_TESTNET_WS_URL.into()
        } else {
            BYBIT_WS_URL.into()
        };

        BybitWsClient {
            config,
            ws_url,
            // TODO: Initialize WebSocket connection
        }
    }

    pub fn ws_url(&self) -> &str {
        self.ws_url.as_str()
    }
}
