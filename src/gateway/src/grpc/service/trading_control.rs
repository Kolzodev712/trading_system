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

pub struct TradingControlService {
    pub trading_control: Arc<TradingControl>,
    pub trading_state_manager: Arc<TradingStateManager>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub operator_results: Arc<Mutex<HashMap<String, trading_cli::OperatorCommandResponse>>>,
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
        let response = match self.trading_control.cancel_working(order_ids, pairs).await {
            Ok(cancelled) => trading_cli::OperatorCommandResponse {
                accepted: true,
                state: "completed".into(),
                message: format!(
                    "CancelWorking: requested cancel of {} orders; remain paused; reconcile fills before treating as flat",
                    cancelled.len()
                ),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: cancelled,
            },
            Err(e) => trading_cli::OperatorCommandResponse {
                accepted: false,
                state: "failed".into(),
                message: format!("CancelWorking failed: {e}"),
                venue,
                environment,
                scope: scope_label(&req.symbol),
                order_ids: vec![],
            },
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
        let (order_ids, pairs) = self.collect_working().await;
        let _ = self.trading_control.cancel_working(order_ids, pairs).await;

        let flatten_result = self
            .trading_control
            .submit_reduce_only_flatten()
            .await
            .map_err(|err| err.to_string());
        let mut flatten_ids = match flatten_result {
            Ok(ids) => ids,
            Err(message) => {
                let response = trading_cli::OperatorCommandResponse {
                    accepted: false,
                    state: "failed".into(),
                    message: format!("FlattenAndPause reduce-only failed: {message}"),
                    venue,
                    environment,
                    scope: scope_label(&req.symbol),
                    order_ids: vec![],
                };
                self.remember(&req.request_id, response.clone()).await;
                return Ok(Response::new(response));
            }
        };

        if flatten_ids.is_empty() {
            let to_close: Vec<(trading_core::types::common::Pair, rust_decimal::Decimal)> = {
                let state = self.trading_state_manager.get_state();
                let guard = state.read().await;
                guard
                    .positions
                    .values()
                    .filter(|p| p.quantity.abs() > rust_decimal::Decimal::new(1, 6))
                    .map(|p| (p.symbol.clone(), p.quantity))
                    .collect()
            };
            for (pair, qty) in to_close {
                let side = if qty > rust_decimal::Decimal::ZERO {
                    trading_core::types::common::Side::Sell
                } else {
                    trading_core::types::common::Side::Buy
                };
                let mut exchange_params = std::collections::HashMap::new();
                exchange_params.insert(
                    smol_str::SmolStr::new("reduceOnly"),
                    smol_str::SmolStr::new("true"),
                );
                let request = trading_core::types::exchange::OrderRequest {
                    instrument: trading_core::types::common::Instrument::new(
                        instrument_type.clone(),
                        pair,
                    ),
                    side,
                    order_type: trading_core::types::common::OrderType::Market,
                    price: rust_decimal::Decimal::ZERO,
                    amount: qty.abs(),
                    timestamp: None,
                    exchange_params,
                };
                match self
                    .trading_control
                    .trading_client()
                    .write()
                    .await
                    .open_order(request)
                    .await
                {
                    Ok(order) => flatten_ids.push(order.order_id.as_str().to_string()),
                    Err(e) => {
                        let message = format!("FlattenAndPause TSM exit failed: {e}");
                        let response = trading_cli::OperatorCommandResponse {
                            accepted: false,
                            state: "failed".into(),
                            message,
                            venue,
                            environment,
                            scope: scope_label(&req.symbol),
                            order_ids: flatten_ids,
                        };
                        self.remember(&req.request_id, response.clone()).await;
                        return Ok(Response::new(response));
                    }
                }
            }
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
                    "FlattenAndPause: latch held until Resume; {} reduce-only orders; TSM is flat",
                    flatten_ids.len()
                ),
            )
        } else {
            (
                "in_progress",
                format!(
                    "FlattenAndPause: latch held; RPC success is not flat (open: {})",
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
