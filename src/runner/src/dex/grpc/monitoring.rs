//! Stub CEX components for DEX-only gRPC monitoring (orderbook + `MonitoringData.dex`).

use std::sync::Arc;

use async_trait::async_trait;
use exchange_engine::state::manager::ExchangeStateManager;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::{Exchange, OrderId, Pair};
use trading_core::types::exchange::{
    Balance, BulkAmendRequest, BulkAmendResponse, BulkCancelRequest, BulkCancelResponse,
    BulkOrderRequest, BulkOrderResponse, Order, OrderRequest, Trade,
};
use trading_core::venue::cex::channels::cex_venue_update_channel;
use trading_core::venue::cex::traits::TradingClient;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

use super::control::DexGrpcControl;
use crate::config::BotConfig;
use crate::grpc::service::TradingCliServiceImpl;
use crate::SharedServices;

/// No-op trading client — DEX mode does not route orders through CEX APIs.
#[derive(Debug)]
struct DexMonitoringTradingClient;

#[async_trait]
impl TradingClient<OrderRequest> for DexMonitoringTradingClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Paper
    }

    async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
        Ok(())
    }

    async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
        Err(EngineError::NotSupported("DEX monitoring mode".into()))
    }

    async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
        Ok(())
    }

    async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
        Ok(())
    }

    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        Ok(vec![])
    }

    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        Ok(vec![])
    }

    async fn fetch_trades(&self, _pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        Ok(vec![])
    }

    async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
        Ok(())
    }

    async fn amend_order(
        &self,
        _order_id: OrderId,
        _new_request: OrderRequest,
    ) -> Result<Order, EngineError> {
        Err(EngineError::NotSupported("DEX monitoring mode".into()))
    }

    async fn bulk_open_orders(
        &self,
        _req: BulkOrderRequest,
    ) -> Result<BulkOrderResponse, EngineError> {
        Err(EngineError::NotSupported("DEX monitoring mode".into()))
    }

    async fn bulk_cancel_orders(
        &self,
        _req: BulkCancelRequest,
    ) -> Result<BulkCancelResponse, EngineError> {
        Err(EngineError::NotSupported("DEX monitoring mode".into()))
    }

    async fn bulk_amend_orders(
        &self,
        _req: BulkAmendRequest,
    ) -> Result<BulkAmendResponse, EngineError> {
        Err(EngineError::NotSupported("DEX monitoring mode".into()))
    }
}

pub struct DexMonitoringGrpc {
    pub service: TradingCliServiceImpl,
}

pub fn build_dex_monitoring_grpc(
    services: &SharedServices,
    control: DexGrpcControl,
    bot_config: &BotConfig,
) -> Result<DexMonitoringGrpc, String> {
    let (cex_tx, cex_rx) = cex_venue_update_channel(64);
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![]));
    let trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
        Arc::new(RwLock::new(DexMonitoringTradingClient));
    let exchange_state_manager = Arc::new(ExchangeStateManager::new(
        cex_tx,
        symbol_registry,
        trading_client.clone(),
        0,
        false,
    ));
    let trading_state_manager = Arc::new(TradingStateManager::new(
        cex_rx,
        exchange_state_manager.clone(),
    ));
    let level_manager = Arc::new(RwLock::new(LevelManager::new()));
    let trading_control = Arc::new(trading_core::grpc::TradingControl::new(
        trading_client.clone(),
    ));

    let (mode, venue, environment) = crate::config::execution_status_from_bot(bot_config);
    let mut service = TradingCliServiceImpl::new(
        level_manager,
        trading_state_manager,
        trading_client,
        exchange_state_manager,
        services.market_data_state_manager.clone(),
        services.indicator_service.clone(),
        services.indicator_calculator.clone(),
        trading_control,
    )
    .with_exchange("deepbook")
    .with_execution_status(mode, venue, environment);

    if let Some(dex_sm) = services.venue.dex_state_manager() {
        service = service.with_dex_state_manager(dex_sm, "dex".to_string());
    }

    if let Some(qm) = control.quote_manager {
        let bm = control
            .balance_manager_id
            .ok_or("balance_manager_id required when quote_manager is set")?;
        service = service.with_dex_quote_control(qm, control.dex_runtime, bm);
    }

    if let Some(sm) = control.strategy_manager {
        service = service.with_dex_strategy_manager(sm);
    }

    if let Some(engine) = control.indicator_engine {
        service = service.with_dex_indicator_engine(engine);
    }

    Ok(DexMonitoringGrpc { service })
}
