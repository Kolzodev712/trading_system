//! Bybit unified client (implements TradingClient)
//!
//! This client handles both spot and futures trading through WebSocket and REST.
//! Differentiation is done via InstrumentType in OrderRequest.

use super::{
    config::BybitConfig,
    rest::{
        client::BybitHttpClient,
        types::{
            balance::{BybitWalletBalanceParams, BybitWalletBalanceResponse},
            open_orders::{BybitOpenOrderParams, BybitOpenOrdersResponse},
            trades::{BybitTradesParams, BybitTradesResponse},
        },
    },
    websocket::{
        rpc_client::{BybitRpcClient, RpcRequest, RpcResponse},
        types::rpc::{
            amend::BybitRpcAmendOrderRequest,
            cancel_all::BybitRpcCancelAllOrdersRequest,
            cancel_by_id::BybitRpcCancelOrderRequest,
            open_order::{BybitRpcCreateOrderArgs, BybitRpcCreateOrderRequest},
        },
    },
};
use crate::state::manager::ExchangeStateManager;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::warn;
use trading_core::{
    error::EngineError,
    registry::symbol::SymbolRegistry,
    rest::HttpMethod,
    traits::TradingClient,
    types::{
        common::{InstrumentType, OrderId, OrderStatus, OrderType, Pair},
        exchange::{Balance, Order, OrderRequest, Trade},
    },
};

use super::common_types::{BybitCategory, BybitOrderType, BybitSide};

#[derive(Debug, Clone)]
pub struct BybitClient {
    pub(crate) config: BybitConfig,
    pub(crate) rpc_client: Arc<BybitRpcClient>,
    pub(crate) rest_client: BybitHttpClient,
    pub(crate) symbol_registry: Arc<SymbolRegistry>,
    state_manager: Arc<tokio::sync::Mutex<Option<Arc<ExchangeStateManager>>>>,
}

impl BybitClient {
    pub fn new(
        config: BybitConfig,
        _inbound_capacity: usize,
        _ping_capacity: usize,
        _outbound_rpc_capacity: usize,
        _shutdown_capacity: usize,
    ) -> Self {
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(config.pairs.clone()));
        let rate_limiter = Arc::new(super::rate_limit::BybitRateLimiter::new(
            config.rate_limiter.clone(),
        ));
        let rpc_client = Arc::new(BybitRpcClient::new(config.clone(), rate_limiter.clone()));
        let rest_client = BybitHttpClient::new(config.clone());

        BybitClient {
            config,
            rpc_client,
            rest_client,
            symbol_registry,
            state_manager: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub async fn set_state_manager(&self, state_manager: Arc<ExchangeStateManager>) {
        let mut guard = self.state_manager.lock().await;
        *guard = Some(state_manager);
    }

    fn get_category_from_instrument_type(&self, instrument_type: InstrumentType) -> BybitCategory {
        match instrument_type {
            InstrumentType::Spot => BybitCategory::Spot,
            InstrumentType::Futures => BybitCategory::Linear,
            InstrumentType::Derivitives => BybitCategory::Linear,
        }
    }

    fn order_request_to_bybit_args(
        &self,
        req: &OrderRequest,
    ) -> Result<BybitRpcCreateOrderArgs, EngineError> {
        let category = self.get_category_from_instrument_type(req.instrument.instrument_type());
        let symbol = req.instrument.symbol();

        let side: BybitSide = req.side.clone().into();
        let order_type: BybitOrderType = match req.order_type {
            OrderType::Market => BybitOrderType::Market,
            OrderType::Limit => BybitOrderType::Limit,
            OrderType::PostOnly => BybitOrderType::Limit,
            OrderType::Ioc => BybitOrderType::Limit,
            OrderType::Fok => BybitOrderType::Limit,
        };

        let time_in_force = match req.order_type {
            OrderType::Limit => Some("GTC".into()),
            OrderType::Market => None,
            OrderType::PostOnly => Some("PostOnly".into()),
            OrderType::Ioc => Some("IOC".into()),
            OrderType::Fok => Some("FOK".into()),
        };

        Ok(BybitRpcCreateOrderArgs {
            symbol,
            side,
            order_type,
            qty: req.amount,
            price: match req.order_type {
                OrderType::Limit | OrderType::PostOnly | OrderType::Ioc | OrderType::Fok => {
                    Some(req.price)
                }
                OrderType::Market => None,
            },
            category,
            time_in_force,
            order_link_id: None,
            reduce_only: None,
            position_idx: None,
            take_profit: None,
            stop_loss: None,
            tp_trigger_by: None,
            sl_trigger_by: None,
        })
    }
}

#[async_trait]
impl TradingClient<OrderRequest> for BybitClient {
    fn exchange_id(&self) -> trading_core::types::common::Exchange {
        trading_core::types::common::Exchange::Bybit
    }

    async fn test_open_order(&self, req: OrderRequest) -> Result<(), EngineError> {
        let category = self.get_category_from_instrument_type(req.instrument.instrument_type());

        let params = BybitOpenOrderParams {
            category: category.clone(),
            symbol: Some(req.instrument.symbol()),
            base_coin: None,
            settle_coin: None,
            order_id: None,
            order_link_id: None,
            order_filter: None,
            order_status: None,
            start_time: None,
            end_time: None,
            limit: Some(1),
            cursor: None,
        };

        let response: BybitOpenOrdersResponse = self
            .rest_client
            .call(
                "order/open-order",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from test order"))
            })?;

        if response.ret_code != 0 {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Test order failed: {}",
                response.ret_msg
            )));
        }

        Ok(())
    }

    async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        let args = self.order_request_to_bybit_args(&req)?;

        let request = BybitRpcCreateOrderRequest {
            req_id: Some(trading_core::utils::generate_custom_uuid().into()),
            header: super::websocket::types::rpc::BybitRpcRequestHeader {
                x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                x_bapi_recv_window: Some("5000".into()),
                referer: None,
            },
            op: "order.create".into(),
            args: vec![args],
        };

        match self
            .rpc_client
            .send_request(RpcRequest::CreateOrder(request))
            .await?
        {
            RpcResponse::CreateOrder(resp) => {
                if resp.ret_code != 0 {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Order creation failed: {}",
                        resp.ret_msg
                    )));
                }

                let data = resp.data.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("No order data in response"))
                })?;

                let order_id = OrderId::new(data.order_id);

                Ok(Order {
                    order_id,
                    client_order_id: data.order_link_id,
                    instrument: req.instrument.clone(),
                    side: req.side,
                    price: req.price,
                    amount: req.amount,
                    internal_created_at: chrono::Utc::now().timestamp_millis() as u64,
                    exchange_created_at: None,
                    status: OrderStatus::New,
                    filled_amount: rust_decimal::Decimal::ZERO,
                })
            }
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Unexpected response type"
            ))),
        }
    }

    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        // Bybit API requires symbol for order cancellation
        // Get order from state manager (authoritative source) to get symbol
        let (symbol, category) = {
            if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
                if let Some(order) = state_mgr.get_order_by_id(&order_id).await {
                    // Extract symbol and determine category from order's instrument type
                    let symbol = order.instrument.symbol();
                    let category = self.get_category_from_instrument_type(
                        order.instrument.instrument_type().clone(),
                    );
                    tracing::info!(
                        "Found order {} in state manager: symbol={}, category={:?}",
                        order_id.as_str(),
                        symbol.as_str(),
                        category
                    );
                    (symbol, category)
                } else {
                    // Order not in state - cannot determine symbol, return error
                    tracing::error!("Order {} not found in state manager. Cannot determine symbol for cancellation.", order_id.as_str());
                    return Err(EngineError::new_custom_error(
                        format!(
                            "Order {} not found in state manager. Cannot determine symbol for cancellation.",
                            order_id.as_str()
                        )
                        .into(),
                    ));
                }
            } else {
                // State manager not available - cannot determine symbol, return error
                tracing::error!(
                    "State manager not available. Cannot determine symbol for order cancellation."
                );
                return Err(EngineError::new_custom_error(
                    "State manager not available. Cannot determine symbol for order cancellation."
                        .into(),
                ));
            }
        };

        let request = BybitRpcCancelOrderRequest {
            req_id: Some(trading_core::utils::generate_custom_uuid().into()),
            header: super::websocket::types::rpc::BybitRpcRequestHeader {
                x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                x_bapi_recv_window: Some("5000".into()),
                referer: None,
            },
            op: "order.cancel".into(),
            args: vec![
                super::websocket::types::rpc::cancel_by_id::BybitRpcCancelOrderArgs {
                    category: category.clone(),
                    symbol: symbol.as_str().into(),
                    order_id: Some(order_id.as_str().into()),
                    order_link_id: None,
                },
            ],
        };

        match self
            .rpc_client
            .send_request(RpcRequest::CancelOrder(request))
            .await?
        {
            RpcResponse::CancelOrder(resp) => {
                if resp.ret_code != 0 {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Order cancellation failed: {}",
                        resp.ret_msg
                    )));
                }
                Ok(())
            }
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Unexpected response type"
            ))),
        }
    }

    async fn cancel_all_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        let symbol = self
            .symbol_registry
            .get_symbol(pair)
            .map(|s| s.to_string().into())
            .unwrap_or_else(|| pair.convert_to_smolstr());

        let request = BybitRpcCancelAllOrdersRequest {
            req_id: Some(trading_core::utils::generate_custom_uuid().into()),
            header: super::websocket::types::rpc::BybitRpcRequestHeader {
                x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                x_bapi_recv_window: Some("5000".into()),
                referer: None,
            },
            op: "order.cancelAll".into(),
            args: vec![
                super::websocket::types::rpc::cancel_all::BybitRpcCancelAllOrdersArgs {
                    category: category.clone(),
                    symbol: Some(symbol),
                    base_coin: None,
                    settle_coin: None,
                },
            ],
        };

        match self
            .rpc_client
            .send_request(RpcRequest::CancelAllOrders(request))
            .await?
        {
            RpcResponse::CancelAllOrders(resp) => {
                if resp.ret_code != 0 {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Cancel all orders failed: {}",
                        resp.ret_msg
                    )));
                }
                Ok(())
            }
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Unexpected response type"
            ))),
        }
    }

    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());

        let category_str = match category {
            BybitCategory::Spot => "spot",
            BybitCategory::Linear => "linear",
            BybitCategory::Inverse => "inverse",
            BybitCategory::Option => "option",
        };

        // Bybit v5 unified accounts only support accountType=UNIFIED
        let params = BybitWalletBalanceParams {
            account_type: Some("UNIFIED".into()),
            coin: None,
            category: Some(category_str.into()),
        };

        tracing::debug!(
            "Fetching Bybit wallet balance: category={}, accountType=UNIFIED",
            category_str
        );

        let response: BybitWalletBalanceResponse = self
            .rest_client
            .call(
                "account/wallet-balance",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from balance fetch"))
            })?;

        if response.ret_code != 0 {
            tracing::error!(
                "Bybit balance fetch failed: retCode={}, retMsg={}, category={}, accountType=UNIFIED",
                response.ret_code,
                response.ret_msg,
                category_str
            );
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Failed to fetch balance: retCode={}, retMsg={}",
                response.ret_code,
                response.ret_msg
            )));
        }

        Ok(response.into())
    }

    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());

        let params = BybitOpenOrderParams {
            category: category.clone(),
            symbol: None,
            base_coin: None,
            settle_coin: None,
            order_id: None,
            order_link_id: None,
            order_filter: None,
            order_status: None,
            start_time: None,
            end_time: None,
            limit: Some(50),
            cursor: None,
        };

        let response: BybitOpenOrdersResponse = self
            .rest_client
            .call(
                "order/open-order",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from test order"))
            })?;

        if response.ret_code != 0 {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Failed to fetch open orders: {}",
                response.ret_msg
            )));
        }

        Ok(response.to_orders(self.symbol_registry.clone()))
    }

    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());

        let symbol = pair.and_then(|p| {
            self.symbol_registry
                .get_symbol(p)
                .map(|s| s.to_string().into())
        });

        let params = BybitTradesParams {
            category: category.clone(),
            symbol,
            order_id: None,
            order_link_id: None,
            base_coin: None,
            settle_coin: None,
            exec_type: None,
            start_time: None,
            end_time: None,
            limit: Some(50),
            cursor: None,
        };

        let response: BybitTradesResponse = self
            .rest_client
            .call(
                "execution/list",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from trades fetch"))
            })?;

        if response.ret_code != 0 {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Failed to fetch trades: {}",
                response.ret_msg
            )));
        }

        Ok(response.to_trades(self.symbol_registry.clone()))
    }

    async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
        warn!("Bybit fetch_market_data_snapshot not yet implemented");
        Ok(())
    }

    async fn amend_order(
        &self,
        order_id: OrderId,
        req: OrderRequest,
    ) -> Result<Order, EngineError> {
        let category = self.get_category_from_instrument_type(req.instrument.instrument_type());
        let symbol = req.instrument.symbol();

        let request = BybitRpcAmendOrderRequest {
            req_id: Some(trading_core::utils::generate_custom_uuid().into()),
            header: super::websocket::types::rpc::BybitRpcRequestHeader {
                x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                x_bapi_recv_window: Some("5000".into()),
                referer: None,
            },
            op: "order.amend".into(),
            args: vec![
                super::websocket::types::rpc::amend::BybitRpcAmendOrderArgs {
                    category: category.clone(),
                    symbol,
                    order_id: Some(order_id.as_str().into()),
                    order_link_id: None,
                    qty: Some(req.amount),
                    price: Some(req.price),
                    take_profit: None,
                    stop_loss: None,
                    tp_trigger_by: None,
                    sl_trigger_by: None,
                },
            ],
        };

        match self
            .rpc_client
            .send_request(RpcRequest::AmendOrder(request))
            .await?
        {
            RpcResponse::AmendOrder(resp) => {
                if resp.ret_code != 0 {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Order amendment failed: {}",
                        resp.ret_msg
                    )));
                }

                let data = resp.data.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("No order data in response"))
                })?;

                Ok(Order {
                    order_id,
                    client_order_id: data.order_link_id,
                    instrument: req.instrument.clone(),
                    side: req.side,
                    price: req.price,
                    amount: req.amount,
                    internal_created_at: chrono::Utc::now().timestamp_millis() as u64,
                    exchange_created_at: None,
                    status: OrderStatus::New,
                    filled_amount: rust_decimal::Decimal::ZERO,
                })
            }
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Unexpected response type"
            ))),
        }
    }
}
