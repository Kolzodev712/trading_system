//! Channel configuration for internal communication buffer sizes
//! This is exchange-agnostic and applies to all exchanges
use serde::{Deserialize, Serialize};

/// Channel configuration - centralizes buffer sizes for all internal communication channels
/// This is exchange-agnostic and applies to all exchanges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Market data update channel (broadcast) - used for orderbooks and trades
    /// Default: 1000 (high-frequency updates)
    #[serde(default = "default_market_data_capacity")]
    pub market_data_capacity: usize,

    /// CEX venue update channel (broadcast) - orders, balances, positions, trades
    /// Default: 1000 (high-frequency updates)
    #[serde(
        default = "default_cex_venue_update_capacity",
        alias = "cex_venue_update_capacity"
    )]
    pub cex_venue_update_capacity: usize,

    /// DEX venue update channel (broadcast) - wallet, pending txs, execution results
    /// Default: 256
    #[serde(default = "default_dex_venue_update_capacity")]
    pub dex_venue_update_capacity: usize,

    /// Indicator update channel (broadcast) - used for indicator value updates
    /// Default: 1000 (high-frequency updates)
    #[serde(default = "default_indicator_update_capacity")]
    pub indicator_update_capacity: usize,

    /// WebSocket inbound channel (mpsc) - internal message handling for exchange WebSocket connections
    /// Default: 1024
    #[serde(default = "default_websocket_inbound_capacity")]
    pub websocket_inbound_capacity: usize,

    /// WebSocket ping channel (mpsc) - ping/pong messages for exchange WebSocket connections
    /// Default: 16 (low volume)
    #[serde(default = "default_websocket_ping_capacity")]
    pub websocket_ping_capacity: usize,

    /// WebSocket outbound RPC channel (mpsc) - RPC requests for exchange WebSocket connections
    /// Default: 1024
    #[serde(default = "default_websocket_outbound_rpc_capacity")]
    pub websocket_outbound_rpc_capacity: usize,

    /// LiveConfig request channel (mpsc) - configuration update requests
    /// Default: 100
    #[serde(default = "default_live_config_request_capacity")]
    pub live_config_request_capacity: usize,

    /// Shutdown channel (broadcast) - shutdown signals
    /// Default: 1 (single message)
    #[serde(default = "default_shutdown_capacity")]
    pub shutdown_capacity: usize,

    /// Market data engine internal buffer - for exchange-specific market data clients
    /// Default: 1000
    #[serde(default = "default_market_data_engine_buffer")]
    pub market_data_engine_buffer: usize,
}

fn default_market_data_capacity() -> usize {
    1000
}

fn default_cex_venue_update_capacity() -> usize {
    1000
}

fn default_dex_venue_update_capacity() -> usize {
    256
}

fn default_indicator_update_capacity() -> usize {
    1000
}

fn default_websocket_inbound_capacity() -> usize {
    4096
}

fn default_websocket_ping_capacity() -> usize {
    16
}

fn default_websocket_outbound_rpc_capacity() -> usize {
    4096
}

fn default_live_config_request_capacity() -> usize {
    100
}

fn default_shutdown_capacity() -> usize {
    1
}

fn default_market_data_engine_buffer() -> usize {
    1000
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            market_data_capacity: default_market_data_capacity(),
            cex_venue_update_capacity: default_cex_venue_update_capacity(),
            dex_venue_update_capacity: default_dex_venue_update_capacity(),
            indicator_update_capacity: default_indicator_update_capacity(),
            websocket_inbound_capacity: default_websocket_inbound_capacity(),
            websocket_ping_capacity: default_websocket_ping_capacity(),
            websocket_outbound_rpc_capacity: default_websocket_outbound_rpc_capacity(),
            live_config_request_capacity: default_live_config_request_capacity(),
            shutdown_capacity: default_shutdown_capacity(),
            market_data_engine_buffer: default_market_data_engine_buffer(),
        }
    }
}
