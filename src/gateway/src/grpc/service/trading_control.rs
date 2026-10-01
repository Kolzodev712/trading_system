//! Trading control RPC handlers
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;
use trading_engine::state::{
    cancel_working_unverified, flatten_cancel_working_failed, flatten_position_unverified,
    merge_rest_working_orders, submit_operator_reduce_only_exits, tsm_working_absent_from_rest,
    verify_rest_exposure_flat, verify_rest_working_orders_cleared, SubmissionJournal,
};

pub struct TradingControlService {
    pub trading_control: Arc<TradingControl>,
    pub trading_state_manager: Arc<TradingStateManager>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub operator_results: Arc<Mutex<HashMap<String, trading_cli::OperatorCommandResponse>>>,
    pub submission_journal: Option<Arc<SubmissionJournal>>,
}

impl TradingControlService {
    pub async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "[gRPC] set_trading_enabled called with enabled={}",
            req.enabled
        );

        let mut active_order_ids_set = HashSet::new();
        let mut pairs_with_orders_set = HashSet::new();

        let mut trading_state_count = 0;
        {
            let state_arc = self.trading_state_manager.get_state();
            let state = state_arc.read().await;
            for order in state.active_orders.values() {
                active_order_ids_set.insert(order.order_id.clone());
                trading_state_count += 1;
            }
        }

        let mut exchange_state_count = 0;
        {
            let exchange_state_arc = self.exchange_state_manager.get_state();
            let exchange_state = exchange_state_arc.read().await;
            for order in exchange_state.get_open_orders() {
                active_order_ids_set.insert(order.order_id.clone());
                if let Some(pair) = exchange_state
                    .symbol_registry()
                    .get_pair(&order.instrument.symbol().into())
                {
                    pairs_with_orders_set.insert(pair.clone());
                }
                exchange_state_count += 1;
            }
        }

        let mut level_manager_count = 0;
        {
            let level_manager = self.level_manager.read().await;
            for level in level_manager.get_all_levels() {
                for order_id in &level.active_orders {
                    active_order_ids_set.insert(order_id.clone());
                    pairs_with_orders_set.insert(level.pair.clone());
                }
                level_manager_count += 1;
            }
        }

        tracing::info!(
            "[gRPC] Collected {} unique order IDs and {} unique pairs for cancellation (TradingState: {}, ExchangeState: {}, LevelManager: {})",
            active_order_ids_set.len(),
            pairs_with_orders_set.len(),
            trading_state_count,
            exchange_state_count,
            level_manager_count
        );

        let active_order_ids: Vec<_> = active_order_ids_set.into_iter().collect();
        let pairs_with_orders: Vec<_> = pairs_with_orders_set.into_iter().collect();

        match self
            .trading_control
            .set_enabled(req.enabled, active_order_ids, pairs_with_orders)
            .await
        {
            Ok(cancelled_orders) => {
                tracing::info!(
                    "[gRPC] set_trading_enabled succeeded, cancelled {} orders",
                    cancelled_orders.len()
                );
                Ok(Response::new(trading_cli::SetTradingEnabledResponse {
                    success: true,
                    message: if req.enabled {
                        "Trading enabled".to_string()
                    } else {
                        format!(
                            "Trading disabled. Cancelled {} orders.",
                            cancelled_orders.len()
                        )
                    },
                    cancelled_orders,
                }))
            }
            Err(e) => {
                tracing::error!("[gRPC] set_trading_enabled failed: {}", e);
                Err(Status::internal(format!(
                    "Failed to set trading status: {}",
                    e
                )))
            }
        }
    }

    pub async fn pause_entries(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
        venue: String,
        environment: String,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        let req = request.into_inner();
        if let Some(cached) = self.cached(&req.request_id).await {
            return Ok(Response::new(cached));
        }
        self.trading_control.pause_entries().await;
        let response = trading_cli::OperatorCommandResponse {
            accepted: true,
            state: "completed".into(),
            message: "PauseEntries: evaluation continues; exposure increases blocked".into(),
            venue,
            environment,
            scope: scope_label(&req.symbol),
            order_ids: vec![],
        };
        self.remember(&req.request_id, response.clone()).await;
        Ok(Response::new(response))
    }

    pub async fn resume(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
        venue: String,
        environment: String,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        let req = request.into_inner();
        if let Some(cached) = self.cached(&req.request_id).await {
            return Ok(Response::new(cached));
        }
        self.trading_control.resume_entries().await;
        let response = trading_cli::OperatorCommandResponse {
            accepted: true,
            state: "completed".into(),
            message: "Resume: PauseEntries and flatten latch cleared; global toggle unchanged"
                .into(),
            venue,
            environment,
            scope: scope_label(&req.symbol),
            order_ids: vec![],
        };
        self.remember(&req.request_id, response.clone()).await;
        Ok(Response::new(response))
    }

    pub async fn cancel_working(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
        venue: String,
        environment: String,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        let req = request.into_inner();
        if let Some(cached) = self.cached(&req.request_id).await {
            return Ok(Response::new(cached));
        }
        self.trading_control.pause_entries().await;
        let (order_ids, pairs) = self.collect_working().await;
        let cancelled = match self.trading_control.cancel_working(order_ids, pairs).await {
            Ok(cancelled) => Ok(cancelled),
            Err(e) => Err(e.to_string()),
        };
        let cancelled = match cancelled {
            Ok(cancelled) => cancelled,
            Err(err) => {
                let response = trading_cli::OperatorCommandResponse {
                    accepted: false,
                    state: "failed".into(),
                    message: cancel_working_unverified(&err),
                    venue,
                    environment,
                    scope: scope_label(&req.symbol),
                    order_ids: vec![],
                };
                self.remember(&req.request_id, response.clone()).await;
                return Ok(Response::new(response));
            }
        };
        let rest_err = {
            let client = self.trading_control.trading_client();
            let client = client.read().await;
            match verify_rest_working_orders_cleared(&*client).await {
                Ok(()) => None,
                Err(err) => Some(err.to_string()),
            }
        };
        let response = if let Some(err) = rest_err {
            trading_cli::OperatorCommandResponse {
                accepted: false,
                state: "failed".into(),
                message: cancel_working_unverified(&err),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: cancelled,
            }
        } else {
            let stale = {
                let state = self.trading_state_manager.get_state();
                let guard = state.read().await;
                tsm_working_absent_from_rest(&guard, &[])
            };
            if !stale.is_empty() {
                tracing::warn!(
                    tsm_working_ids = ?stale,
                    "CancelWorking: TSM working IDs absent from REST after cancel; reconciliation mismatch"
                );
            }
            let recon = if stale.is_empty() {
                String::new()
            } else {
                format!(
                    " TSM still lists working IDs absent from REST ({}); reconciliation mismatch",
                    stale.join(",")
                )
            };
            trading_cli::OperatorCommandResponse {
                accepted: true,
                state: "completed".into(),
                message: format!(
                    "CancelWorking: REST confirmed {} orders gone; remain paused.{recon}",
                    cancelled.len()
                ),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: cancelled,
            }
        };
        self.remember(&req.request_id, response.clone()).await;
        Ok(Response::new(response))
    }

    pub async fn flatten_and_pause(
        &self,
        request: Request<trading_cli::OperatorCommandRequest>,
        venue: String,
        environment: String,
        instrument_type: trading_core::types::common::InstrumentType,
    ) -> Result<Response<trading_cli::OperatorCommandResponse>, Status> {
        let req = request.into_inner();
        if let Some(cached) = self.cached(&req.request_id).await {
            return Ok(Response::new(cached));
        }
        self.trading_control.latch_flatten_and_pause().await;
        let rest_before = {
            let client = self.trading_control.trading_client();
            let client = client.read().await;
            match client.fetch_open_orders().await {
                Ok(orders) => orders,
                Err(err) => {
                    drop(client);
                    let response = trading_cli::OperatorCommandResponse {
                        accepted: false,
                        state: "failed".into(),
                        message: flatten_cancel_working_failed(&format!(
                            "REST GET /open_orders failed: {err}"
                        )),
                        venue,
                        environment,
                        scope: scope_label(&req.symbol),
                        order_ids: vec![],
                    };
                    self.remember(&req.request_id, response.clone()).await;
                    return Ok(Response::new(response));
                }
            }
        };
        let (mut order_ids, mut pairs) = self.collect_working().await;
        merge_rest_working_orders(&mut order_ids, &mut pairs, &rest_before);
        let cancel_err = match self.trading_control.cancel_working(order_ids, pairs).await {
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };
        if let Some(err) = cancel_err {
            let response = trading_cli::OperatorCommandResponse {
                accepted: false,
                state: "failed".into(),
                message: flatten_cancel_working_failed(&err),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: vec![],
            };
            self.remember(&req.request_id, response.clone()).await;
            return Ok(Response::new(response));
        }
        let rest_after_err = {
            let client = self.trading_control.trading_client();
            let client = client.read().await;
            match verify_rest_working_orders_cleared(&*client).await {
                Ok(()) => None,
                Err(err) => Some(err.to_string()),
            }
        };
        if let Some(err) = rest_after_err {
            let response = trading_cli::OperatorCommandResponse {
                accepted: false,
                state: "failed".into(),
                message: flatten_cancel_working_failed(&err),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: vec![],
            };
            self.remember(&req.request_id, response.clone()).await;
            return Ok(Response::new(response));
        }
        {
            let state = self.trading_state_manager.get_state();
            let guard = state.read().await;
            let stale = tsm_working_absent_from_rest(&guard, &[]);
            if !stale.is_empty() {
                tracing::warn!(
                    tsm_working_ids = ?stale,
                    "FlattenAndPause CancelWorking: TSM working IDs absent from REST; reconciliation mismatch, not a live operator exit"
                );
            }
        }

        let Some(journal) = self.submission_journal.as_ref() else {
            let response = trading_cli::OperatorCommandResponse {
                accepted: false,
                state: "failed".into(),
                message:
                    "FlattenAndPause: submission journal required; latch held; reduce-only not sent"
                        .into(),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: vec![],
            };
            self.remember(&req.request_id, response.clone()).await;
            return Ok(Response::new(response));
        };

        let tsm_positions: Vec<(trading_core::types::common::Pair, rust_decimal::Decimal)> = {
            let state = self.trading_state_manager.get_state();
            let guard = state.read().await;
            guard
                .positions
                .values()
                .map(|p| (p.symbol.clone(), p.quantity))
                .collect()
        };
        let flatten_ids = {
            let client = self.trading_control.trading_client();
            let client = client.read().await;
            let state_lock = self.trading_state_manager.get_state();
            let mut state = state_lock.write().await;
            match submit_operator_reduce_only_exits(
                journal.as_ref(),
                &*client,
                &mut state,
                instrument_type.clone(),
                &tsm_positions,
            )
            .await
            {
                Ok(report) => report.exchange_order_ids,
                Err(err) => {
                    drop(state);
                    drop(client);
                    let response = trading_cli::OperatorCommandResponse {
                        accepted: false,
                        state: "failed".into(),
                        message: format!("FlattenAndPause reduce-only failed: {err}"),
                        venue,
                        environment,
                        scope: scope_label(&req.symbol),
                        order_ids: vec![],
                    };
                    self.remember(&req.request_id, response.clone()).await;
                    return Ok(Response::new(response));
                }
            }
        };

        let verify_pairs: Vec<trading_core::types::common::Pair> = {
            let mut pairs: Vec<_> = tsm_positions.iter().map(|(p, _)| p.clone()).collect();
            pairs.sort_by(|a, b| a.base.cmp(&b.base).then(a.quote.cmp(&b.quote)));
            pairs.dedup();
            pairs
        };
        let rest_position_err = {
            let client = self.trading_control.trading_client();
            let client = client.read().await;
            match verify_rest_exposure_flat(&*client, instrument_type, &verify_pairs).await {
                Ok(()) => None,
                Err(err) => Some(err.to_string()),
            }
        };
        if let Some(err) = rest_position_err {
            let response = trading_cli::OperatorCommandResponse {
                accepted: false,
                state: "failed".into(),
                message: flatten_position_unverified(&err),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: flatten_ids,
            };
            self.remember(&req.request_id, response.clone()).await;
            return Ok(Response::new(response));
        }

        let still_open: Vec<_> = {
            let state = self.trading_state_manager.get_state();
            let guard = state.read().await;
            guard
                .positions
                .values()
                .filter(|p| p.quantity.abs() > rust_decimal::Decimal::new(1, 6))
                .map(|p| p.symbol.to_string())
                .collect()
        };
        let (state_label, message) = if still_open.is_empty() {
            (
                "completed",
                format!(
                    "FlattenAndPause: latch held until Resume; {} reduce-only orders; REST and TSM are flat",
                    flatten_ids.len()
                ),
            )
        } else {
            (
                "in_progress",
                format!(
                    "FlattenAndPause: latch held; REST working book cleared; TSM still open: {}",
                    still_open.join(",")
                ),
            )
        };
        let response = trading_cli::OperatorCommandResponse {
            accepted: true,
            state: state_label.into(),
            message,
            venue,
            environment,
            scope: scope_label(&req.symbol),
            order_ids: flatten_ids,
        };
        self.remember(&req.request_id, response.clone()).await;
        Ok(Response::new(response))
    }

    async fn cached(&self, request_id: &str) -> Option<trading_cli::OperatorCommandResponse> {
        if request_id.is_empty() {
            return None;
        }
        self.operator_results.lock().await.get(request_id).cloned()
    }

    async fn remember(&self, request_id: &str, response: trading_cli::OperatorCommandResponse) {
        if request_id.is_empty() {
            return;
        }
        self.operator_results
            .lock()
            .await
            .insert(request_id.to_string(), response);
    }

    async fn collect_working(
        &self,
    ) -> (
        Vec<trading_core::types::common::OrderId>,
        Vec<trading_core::types::common::Pair>,
    ) {
        let mut order_ids = HashSet::new();
        let mut pairs = HashSet::new();
        {
            let state = self.trading_state_manager.get_state();
            let guard = state.read().await;
            for order in guard.active_orders.values() {
                order_ids.insert(order.order_id.clone());
                pairs.insert(order.instrument.pair.clone());
            }
        }
        {
            let exchange_state = self.exchange_state_manager.get_state();
            let guard = exchange_state.read().await;
            for order in guard.get_open_orders() {
                order_ids.insert(order.order_id.clone());
                if let Some(pair) = guard
                    .symbol_registry()
                    .get_pair(&order.instrument.symbol().into())
                {
                    pairs.insert(pair.clone());
                }
                pairs.insert(order.instrument.pair.clone());
            }
        }
        {
            let state = self.trading_state_manager.get_state();
            let guard = state.read().await;
            for position in guard.positions.values() {
                pairs.insert(position.symbol.clone());
            }
        }
        {
            let level_manager = self.level_manager.read().await;
            for level in level_manager.get_all_levels() {
                for order_id in &level.active_orders {
                    order_ids.insert(order_id.clone());
                    pairs.insert(level.pair.clone());
                }
            }
        }
        (order_ids.into_iter().collect(), pairs.into_iter().collect())
    }
}

fn scope_label(symbol: &str) -> String {
    if symbol.is_empty() {
        "account".into()
    } else {
        symbol.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rust_decimal::Decimal;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use trading_core::error::EngineError;
    use trading_core::registry::symbol::SymbolRegistry;
    use trading_core::types::common::{Exchange, OrderId, Pair};
    use trading_core::types::exchange::{Balance, Order, OrderRequest, Trade};
    use trading_core::venue::cex::channels::cex_venue_update_channel;
    use trading_core::venue::cex::traits::TradingClient;
    use trading_engine::state::Position;

    #[derive(Debug)]
    struct UnverifiedOpenOrdersClient {
        placed: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for UnverifiedOpenOrdersClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            self.placed.fetch_add(1, Ordering::SeqCst);
            Err(EngineError::NotSupported("test".into()))
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /open_orders: 503"
            )))
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /open_orders: 503"
            )))
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
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_open_orders(
            &self,
            _req: trading_core::types::exchange::BulkOrderRequest,
        ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    fn flatten_service(
        client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    ) -> TradingControlService {
        let (tx, rx) = cex_venue_update_channel(8);
        let exchange_state_manager = Arc::new(ExchangeStateManager::new(
            tx,
            Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")])),
            client.clone(),
            0,
            false,
        ));
        let trading_state_manager =
            Arc::new(TradingStateManager::new(rx, exchange_state_manager.clone()));
        TradingControlService {
            trading_control: Arc::new(TradingControl::new(client)),
            trading_state_manager,
            exchange_state_manager,
            level_manager: Arc::new(RwLock::new(LevelManager::new())),
            operator_results: Arc::new(Mutex::new(HashMap::new())),
            submission_journal: None,
        }
    }

    #[tokio::test]
    async fn flatten_and_pause_reports_unverified_cancel_working() {
        let placed = Arc::new(AtomicUsize::new(0));
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(UnverifiedOpenOrdersClient {
                placed: placed.clone(),
            }));
        let service = flatten_service(client);
        {
            let state = service.trading_state_manager.get_state();
            let mut guard = state.write().await;
            guard.positions.insert(
                Pair::new("BTC", "USDT"),
                Position {
                    symbol: Pair::new("BTC", "USDT"),
                    quantity: Decimal::new(1, 3),
                    avg_price: Decimal::ONE,
                },
            );
        }
        let response = service
            .flatten_and_pause(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "cw-unverified".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
                trading_core::types::common::InstrumentType::Futures,
            )
            .await
            .expect("rpc")
            .into_inner();
        assert!(!response.accepted);
        assert_eq!(response.state, "failed");
        assert!(
            response.message.contains("CancelWorking unverified"),
            "unexpected message: {}",
            response.message
        );
        assert!(response.message.contains("not treated as cancelled"));
        assert!(response.message.contains("Reduce-only exit was not sent"));
        assert!(service.trading_control.flatten_latched().await);
        assert!(service.trading_control.entries_paused().await);
        assert_eq!(placed.load(Ordering::SeqCst), 0);
    }

    #[derive(Debug)]
    struct RemainingWorkingAfterCancelClient {
        placed: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for RemainingWorkingAfterCancelClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            self.placed.fetch_add(1, Ordering::SeqCst);
            Err(EngineError::NotSupported("test".into()))
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
            Ok(vec![Order {
                order_id: OrderId::new("cache-miss".into()),
                client_order_id: None,
                instrument: trading_core::types::common::Instrument::new(
                    trading_core::types::common::InstrumentType::Futures,
                    Pair::new("BTC", "USDT"),
                ),
                side: trading_core::types::common::Side::Buy,
                price: Decimal::ONE,
                amount: Decimal::new(1, 3),
                internal_created_at: 1,
                exchange_created_at: Some(1),
                status: trading_core::types::common::OrderStatus::New,
                filled_amount: Decimal::ZERO,
            }])
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
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_open_orders(
            &self,
            _req: trading_core::types::exchange::BulkOrderRequest,
        ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    #[tokio::test]
    async fn flatten_and_pause_does_not_send_when_rest_still_working() {
        let placed = Arc::new(AtomicUsize::new(0));
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(RemainingWorkingAfterCancelClient {
                placed: placed.clone(),
            }));
        let service = flatten_service(client);
        {
            let state = service.trading_state_manager.get_state();
            let mut guard = state.write().await;
            guard.positions.insert(
                Pair::new("BTC", "USDT"),
                Position {
                    symbol: Pair::new("BTC", "USDT"),
                    quantity: Decimal::new(1, 3),
                    avg_price: Decimal::ONE,
                },
            );
        }
        let response = service
            .flatten_and_pause(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "rest-still-working".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
                trading_core::types::common::InstrumentType::Futures,
            )
            .await
            .expect("rpc")
            .into_inner();
        assert!(!response.accepted);
        assert!(response.message.contains("working orders after cancel"));
        assert!(service.trading_control.flatten_latched().await);
        assert_eq!(placed.load(Ordering::SeqCst), 0);
    }

    #[derive(Debug)]
    struct TsmFlatRestOpenClient {
        leftover: rust_decimal::Decimal,
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for TsmFlatRestOpenClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
            Ok(Order {
                order_id: OrderId::new("flatten-rest".into()),
                client_order_id: req.exchange_params.get("orderLinkId").cloned(),
                instrument: req.instrument,
                side: req.side,
                price: Decimal::ONE,
                amount: req.amount,
                internal_created_at: 1,
                exchange_created_at: Some(1),
                status: trading_core::types::common::OrderStatus::New,
                filled_amount: Decimal::ZERO,
            })
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
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_open_orders(
            &self,
            _req: trading_core::types::exchange::BulkOrderRequest,
        ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn fetch_positions(
            &self,
        ) -> Result<Vec<trading_core::types::exchange::Position>, EngineError> {
            Ok(vec![trading_core::types::exchange::Position {
                instrument: trading_core::types::common::Instrument::new(
                    trading_core::types::common::InstrumentType::Futures,
                    Pair::new("BTC", "USDT"),
                ),
                quantity: self.leftover,
                avg_price: Decimal::ONE,
                current_price: Decimal::ONE,
                unrealized_pnl: Decimal::ZERO,
                last_update: 1,
                mark_price: None,
                index_price: None,
                realized_pnl: None,
                initial_margin: None,
                maintenance_margin: None,
                leverage: None,
                position_mode: None,
                margin_type: None,
                liquidation_price: None,
            }])
        }
    }

    #[tokio::test]
    async fn flatten_and_pause_tsm_flat_rest_position_is_unverified() {
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(TsmFlatRestOpenClient {
                leftover: Decimal::new(1, 3),
            }));
        let journal_path = std::env::temp_dir().join(format!(
            "gw-flat-tsm-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let journal = Arc::new(SubmissionJournal::open(&journal_path).unwrap());
        let mut service = flatten_service(client);
        service.submission_journal = Some(journal);
        let response = service
            .flatten_and_pause(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "tsm-flat-rest-open".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
                trading_core::types::common::InstrumentType::Futures,
            )
            .await
            .expect("rpc")
            .into_inner();
        let _ = std::fs::remove_file(&journal_path);
        assert!(!response.accepted);
        assert_eq!(response.state, "failed");
        assert!(
            response.message.contains("venue position unverified")
                || response.message.contains("TSM flat is not venue flat"),
            "unexpected message: {}",
            response.message
        );
        assert!(service.trading_control.flatten_latched().await);
        assert!(service.trading_control.entries_paused().await);
    }

    #[tokio::test]
    async fn cancel_working_ack_with_remaining_order_is_unverified() {
        let placed = Arc::new(AtomicUsize::new(0));
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(RemainingWorkingAfterCancelClient {
                placed: placed.clone(),
            }));
        let service = flatten_service(client);
        let response = service
            .cancel_working(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "cancel-leftover".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
            )
            .await
            .expect("rpc")
            .into_inner();
        assert!(!response.accepted);
        assert_eq!(response.state, "failed");
        assert!(
            response.message.contains("CancelWorking unverified"),
            "unexpected message: {}",
            response.message
        );
        assert!(service.trading_control.entries_paused().await);
    }

    #[derive(Debug)]
    struct RecoveringOpenOrdersClient {
        leftover: Arc<AtomicBool>,
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for RecoveringOpenOrdersClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
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
            if self.leftover.load(Ordering::SeqCst) {
                Ok(vec![Order {
                    order_id: OrderId::new("still-open".into()),
                    client_order_id: None,
                    instrument: trading_core::types::common::Instrument::new(
                        trading_core::types::common::InstrumentType::Futures,
                        Pair::new("BTC", "USDT"),
                    ),
                    side: trading_core::types::common::Side::Buy,
                    price: Decimal::ONE,
                    amount: Decimal::new(1, 3),
                    internal_created_at: 1,
                    exchange_created_at: Some(1),
                    status: trading_core::types::common::OrderStatus::New,
                    filled_amount: Decimal::ZERO,
                }])
            } else {
                Ok(vec![])
            }
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
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_open_orders(
            &self,
            _req: trading_core::types::exchange::BulkOrderRequest,
        ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    #[tokio::test]
    async fn cancel_working_retry_requires_new_request_id() {
        let leftover = Arc::new(AtomicBool::new(true));
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(RecoveringOpenOrdersClient {
                leftover: leftover.clone(),
            }));
        let service = flatten_service(client);
        let first = service
            .cancel_working(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "cleanup-cancel-1".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
            )
            .await
            .expect("rpc")
            .into_inner();
        assert!(!first.accepted);
        leftover.store(false, Ordering::SeqCst);
        let cached = service
            .cancel_working(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "cleanup-cancel-1".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
            )
            .await
            .expect("rpc")
            .into_inner();
        assert!(!cached.accepted, "same request id must replay the failure");
        assert_eq!(cached.message, first.message);
        let retry = service
            .cancel_working(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "cleanup-cancel-2".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
            )
            .await
            .expect("rpc")
            .into_inner();
        assert!(
            retry.accepted,
            "new request id after REST recovery must verify: {}",
            retry.message
        );
    }

    #[derive(Debug)]
    struct TsmEmptySpotWalletClient;

    #[async_trait]
    impl TradingClient<OrderRequest> for TsmEmptySpotWalletClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }
        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }
        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![Balance {
                symbol: "BTC".into(),
                free: Decimal::new(1, 3),
                used: Decimal::ZERO,
                total: Decimal::new(1, 3),
            }])
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
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_open_orders(
            &self,
            _req: trading_core::types::exchange::BulkOrderRequest,
        ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    #[tokio::test]
    async fn flatten_and_pause_tsm_empty_spot_wallet_is_unverified() {
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(TsmEmptySpotWalletClient));
        let journal_path = std::env::temp_dir().join(format!(
            "gw-flat-spot-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let journal = Arc::new(SubmissionJournal::open(&journal_path).unwrap());
        let mut service = flatten_service(client);
        service.submission_journal = Some(journal);
        let response = service
            .flatten_and_pause(
                Request::new(trading_cli::OperatorCommandRequest {
                    request_id: "tsm-empty-spot-btc".into(),
                    symbol: String::new(),
                }),
                "PAPER".into(),
                "LOCAL".into(),
                trading_core::types::common::InstrumentType::Spot,
            )
            .await
            .expect("rpc")
            .into_inner();
        let _ = std::fs::remove_file(&journal_path);
        assert!(!response.accepted);
        assert_eq!(response.state, "failed");
        assert!(
            response.message.contains("wallet still shows")
                || response.message.contains("venue position unverified"),
            "unexpected message: {}",
            response.message
        );
        assert!(service.trading_control.flatten_latched().await);
        assert!(service.trading_control.entries_paused().await);
    }
}
