//! Bybit unified WebSocket client (implements TradingClient)
//!
//! This client handles both spot and futures trading through WebSocket.
//! Differentiation is done via InstrumentType in OrderRequest.

use super::super::config::BybitConfig;
use super::client::BybitWsClient;
use async_trait::async_trait;
use std::fmt::Debug;
use tracing::warn;
use trading_core::{error::EngineError, traits::TradingClient, types::exchange::OrderRequest};

/// Bybit unified client (implements TradingClient)
///
/// This client handles both spot and futures trading through WebSocket.
/// Differentiation is done via InstrumentType in OrderRequest.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BybitClient {
    pub(crate) config: BybitConfig,
    pub(crate) ws_client: BybitWsClient,
    // TODO: Add RPC client, streaming client, etc. when implementing
}

impl BybitClient {
    pub fn new(
        config: BybitConfig,
        _inbound_capacity: usize,
        _ping_capacity: usize,
        _outbound_rpc_capacity: usize,
        _shutdown_capacity: usize,
    ) -> Self {
        let ws_client = BybitWsClient::new(config.clone());

        BybitClient {
            config,
            ws_client,
            // TODO: Initialize RPC client, streaming client, etc.
        }
    }
}

#[async_trait]
impl TradingClient<OrderRequest> for BybitClient {
    fn exchange_id(&self) -> trading_core::types::common::Exchange {
        trading_core::types::common::Exchange::Bybit
    }

    async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
        warn!("Bybit test_open_order not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn open_order(
        &self,
        _req: OrderRequest,
    ) -> Result<trading_core::types::exchange::Order, EngineError> {
        warn!("Bybit open_order not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn cancel_order_by_id(
        &self,
        _order_id: trading_core::types::common::OrderId,
    ) -> Result<(), EngineError> {
        warn!("Bybit cancel_order_by_id not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn cancel_all_orders(
        &self,
        _pair: &trading_core::types::common::Pair,
    ) -> Result<(), EngineError> {
        warn!("Bybit cancel_all_orders not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn fetch_balance(
        &self,
    ) -> Result<Vec<trading_core::types::exchange::Balance>, EngineError> {
        warn!("Bybit fetch_balance not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<trading_core::types::exchange::Order>, EngineError> {
        warn!("Bybit fetch_open_orders not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn fetch_trades(
        &self,
        _pair: Option<&trading_core::types::common::Pair>,
    ) -> Result<Vec<trading_core::types::exchange::Trade>, EngineError> {
        warn!("Bybit fetch_trades not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn fetch_market_data_snapshot(
        &self,
        _pair: &trading_core::types::common::Pair,
    ) -> Result<(), EngineError> {
        warn!("Bybit fetch_market_data_snapshot not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }

    async fn amend_order(
        &self,
        _order_id: trading_core::types::common::OrderId,
        _req: OrderRequest,
    ) -> Result<trading_core::types::exchange::Order, EngineError> {
        warn!("Bybit amend_order not yet implemented");
        Err(EngineError::NotSupported(
            "Bybit trading not yet implemented".into(),
        ))
    }
}
