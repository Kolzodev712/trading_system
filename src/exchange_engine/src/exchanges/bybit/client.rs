//! Bybit unified client (implements TradingClient)
//!
//! This client handles both spot and futures trading through WebSocket and REST.
//! Differentiation is done via InstrumentType in OrderRequest.

use super::{
    config::BybitConfig,
    pagination::{
        collect_open_order_pages, linear_list_selector, OpenOrderPageItem, OrderPage,
        BYBIT_OPEN_ORDER_MAX_PAGES, BYBIT_OPEN_ORDER_PAGE_LIMIT,
    },
    rest::{
        client::BybitHttpClient,
        types::{
            balance::{BybitWalletBalanceParams, BybitWalletBalanceResponse},
            collateral::BybitCollateralBalanceResponse,
            open_orders::{BybitOpenOrderParams, BybitOpenOrdersResponse},
            position::{
                position_idx_to_mode, position_mode_to_bybit, BybitAckResponse,
                BybitPositionListParams, BybitPositionResponse, BybitSetLeverageRequest,
                BybitSwitchModeRequest,
            },
            trades::{BybitTradesParams, BybitTradesResponse},
        },
    },
    websocket::{
        rpc_client::{BybitRpcClient, RpcRequest, RpcResponse},
        types::rpc::{
            amend::BybitRpcAmendOrderRequest,
            bulk::{
                BybitRpcBatchAmendOrderRequest, BybitRpcBatchCancelOrderRequest,
                BybitRpcBatchCreateOrderRequest,
            },
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
    types::{
        common::{InstrumentType, OrderId, OrderStatus, OrderType, Pair},
        exchange::{
            Balance, BulkAmendItem, BulkAmendRequest, BulkAmendResponse, BulkCancelRequest,
            BulkCancelResponse, BulkOperationResult, BulkOrderRequest, BulkOrderResponse,
            ClientOrderLookup, Order, OrderRequest, Position, Trade,
        },
        futures::{FuturesBalance, LeverageInfo, PositionMode},
    },
    venue::cex::traits::TradingClient,
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

struct PagedOpenOrder {
    id: String,
    order: Order,
}

impl OpenOrderPageItem for PagedOpenOrder {
    fn page_item_id(&self) -> &str {
        &self.id
    }
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
            order_link_id: req
                .exchange_params
                .get("orderLinkId")
                .cloned()
                .or_else(|| req.exchange_params.get("clientOrderId").cloned()),
            reduce_only: req
                .exchange_params
                .get("reduceOnly")
                .map(|value| value.eq_ignore_ascii_case("true")),
            position_idx: None,
            take_profit: None,
            stop_loss: None,
            tp_trigger_by: None,
            sl_trigger_by: None,
        })
    }

    fn linear_settle_coin(&self) -> smol_str::SmolStr {
        self.config
            .pairs
            .first()
            .map(|pair| pair.quote.as_str().into())
            .unwrap_or_else(|| "USDT".into())
    }

    fn open_order_query_params(&self) -> Result<Vec<BybitOpenOrderParams>, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        match category {
            BybitCategory::Linear | BybitCategory::Inverse => {
                if self.config.pairs.is_empty() {
                    Ok(vec![self.open_order_params(
                        category.clone(),
                        linear_list_selector(category, None, self.linear_settle_coin())
                            .map_err(|err| EngineError::CustomError(anyhow::anyhow!("{err}")))?,
                    )])
                } else {
                    self.config
                        .pairs
                        .iter()
                        .map(|pair| {
                            let selector = linear_list_selector(
                                category.clone(),
                                Some(pair.convert_to_smolstr()),
                                self.linear_settle_coin(),
                            )
                            .map_err(|err| EngineError::CustomError(anyhow::anyhow!("{err}")))?;
                            Ok(self.open_order_params(category.clone(), selector))
                        })
                        .collect()
                }
            }
            BybitCategory::Spot | BybitCategory::Option => Ok(vec![self.open_order_params(
                category,
                super::pagination::LinearListSelector {
                    symbol: None,
                    base_coin: None,
                    settle_coin: None,
                },
            )]),
        }
    }

    fn open_order_params(
        &self,
        category: BybitCategory,
        selector: super::pagination::LinearListSelector,
    ) -> BybitOpenOrderParams {
        BybitOpenOrderParams {
            category,
            symbol: selector.symbol,
            base_coin: selector.base_coin,
            settle_coin: selector.settle_coin,
            order_id: None,
            order_link_id: None,
            order_filter: None,
            order_status: None,
            start_time: None,
            end_time: None,
            limit: Some(BYBIT_OPEN_ORDER_PAGE_LIMIT),
            cursor: None,
        }
    }

    async fn fetch_open_order_page(
        &self,
        mut params: BybitOpenOrderParams,
        cursor: Option<String>,
    ) -> Result<OrderPage<PagedOpenOrder>, EngineError> {
        params.cursor = cursor.map(Into::into);
        params.limit = Some(BYBIT_OPEN_ORDER_PAGE_LIMIT);
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
                EngineError::CustomError(anyhow::anyhow!("No response from open-order"))
            })?;

        if response.ret_code != 0 {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Failed to fetch open orders: {}",
                response.ret_msg
            )));
        }

        let next_page_cursor = if response.result.next_page_cursor.is_empty() {
            None
        } else {
            Some(response.result.next_page_cursor.to_string())
        };
        let orders = response.to_orders(self.symbol_registry.clone());
        Ok(OrderPage {
            items: orders
                .into_iter()
                .map(|order| PagedOpenOrder {
                    id: order.order_id.as_str().to_string(),
                    order,
                })
                .collect(),
            next_page_cursor,
        })
    }

    async fn lookup_orders_by_link_id(
        &self,
        endpoint: &str,
        order_link_id: &str,
    ) -> Result<Option<Vec<Order>>, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        let selector = linear_list_selector(
            category.clone(),
            self.config
                .pairs
                .first()
                .map(|pair| pair.convert_to_smolstr()),
            self.linear_settle_coin(),
        )
        .map_err(|err| EngineError::CustomError(anyhow::anyhow!("{err}")))?;
        let params = BybitOpenOrderParams {
            category,
            symbol: selector.symbol,
            base_coin: selector.base_coin,
            settle_coin: selector.settle_coin,
            order_id: None,
            order_link_id: Some(order_link_id.into()),
            order_filter: None,
            order_status: None,
            start_time: None,
            end_time: None,
            limit: Some(1),
            cursor: None,
        };
        let response: BybitOpenOrdersResponse = match self
            .rest_client
            .call(endpoint, HttpMethod::GET, Some(params), None, None, true)
            .await
        {
            Ok(Some(response)) => response,
            Ok(None) => return Ok(None),
            Err(err) => {
                warn!("Bybit {endpoint} lookup by orderLinkId failed: {err}");
                return Ok(None);
            }
        };
        if response.ret_code != 0 {
            warn!(
                "Bybit {endpoint} lookup by orderLinkId ret_code={}: {}",
                response.ret_code, response.ret_msg
            );
            return Ok(None);
        }
        Ok(Some(response.to_orders(self.symbol_registry.clone())))
    }

    async fn fetch_position_list(
        &self,
        symbol: Option<&Pair>,
    ) -> Result<BybitPositionResponse, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        let params = BybitPositionListParams {
            category,
            symbol: symbol.map(|pair| pair.convert_to_smolstr()),
            settle_coin: if symbol.is_none() {
                Some(self.linear_settle_coin())
            } else {
                None
            },
        };

        self.rest_client
            .call(
                "position/list",
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from position/list"))
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
                    symbol,
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
        let mut all = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for params in self.open_order_query_params()? {
            let page_params = params.clone();
            let paged = collect_open_order_pages(
                |cursor| self.fetch_open_order_page(page_params.clone(), cursor),
                BYBIT_OPEN_ORDER_PAGE_LIMIT,
                BYBIT_OPEN_ORDER_MAX_PAGES,
            )
            .await
            .map_err(|err| EngineError::CustomError(anyhow::anyhow!("{err}")))?;
            for item in paged {
                if seen.insert(item.id.clone()) {
                    all.push(item.order);
                }
            }
        }
        Ok(all)
    }

    async fn fetch_order_by_client_id(
        &self,
        client_order_id: &str,
    ) -> Result<ClientOrderLookup, EngineError> {
        if let Some(orders) = self
            .lookup_orders_by_link_id("order/open-order", client_order_id)
            .await?
        {
            if let Some(order) = orders.into_iter().next() {
                return Ok(ClientOrderLookup::from_order(order));
            }
        }
        if let Some(orders) = self
            .lookup_orders_by_link_id("order/history", client_order_id)
            .await?
        {
            if let Some(order) = orders.into_iter().next() {
                return Ok(ClientOrderLookup::from_order(order));
            }
        }
        Ok(ClientOrderLookup::Inconclusive)
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

    async fn bulk_open_orders(
        &self,
        req: BulkOrderRequest,
    ) -> Result<BulkOrderResponse, EngineError> {
        if req.requests.is_empty() {
            return Ok(BulkOrderResponse {
                results: vec![],
                success_count: 0,
                failure_count: 0,
            });
        }

        // Group requests by category (required by Bybit API structure)
        // But process them uniformly - category is just metadata
        let mut requests_by_category: Vec<(BybitCategory, Vec<(usize, BybitRpcCreateOrderArgs)>)> =
            Vec::new();

        for (idx, order_req) in req.requests.iter().enumerate() {
            let category =
                self.get_category_from_instrument_type(order_req.instrument.instrument_type());
            let args = self.order_request_to_bybit_args(order_req)?;

            // Find or create category group
            if let Some((_, requests)) = requests_by_category
                .iter_mut()
                .find(|(cat, _)| *cat == category)
            {
                requests.push((idx, args));
            } else {
                requests_by_category.push((category, vec![(idx, args)]));
            }
        }

        let mut results = Vec::new();
        let mut success_count = 0;
        let mut failure_count = 0;

        // Process all categories uniformly
        for (category, requests_with_indices) in requests_by_category {
            let batch_items: Vec<_> = requests_with_indices
                .iter()
                .map(
                    |(_, args)| super::websocket::types::rpc::bulk::BybitRpcBatchCreateOrderItem {
                        symbol: args.symbol.clone(),
                        side: args.side.clone(),
                        order_type: args.order_type.clone(),
                        qty: args.qty,
                        price: args.price.unwrap_or_default(),
                        time_in_force: args.time_in_force.clone().unwrap_or_else(|| "GTC".into()),
                        order_link_id: args.order_link_id.clone(),
                    },
                )
                .collect();

            let batch_request = BybitRpcBatchCreateOrderRequest {
                req_id: Some(trading_core::utils::generate_custom_uuid().into()),
                header: super::websocket::types::rpc::BybitRpcRequestHeader {
                    x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                    x_bapi_recv_window: Some("5000".into()),
                    referer: None,
                },
                op: "order.create-batch".into(),
                args: vec![
                    super::websocket::types::rpc::bulk::BybitRpcBatchCreateOrderArgs {
                        category,
                        request: batch_items,
                    },
                ],
            };

            match self
                .rpc_client
                .send_request(RpcRequest::BatchCreateOrder(batch_request))
                .await?
            {
                RpcResponse::BatchCreateOrder(resp) => {
                    if resp.ret_code != 0 {
                        // All failed for this category
                        for (orig_idx, _) in &requests_with_indices {
                            results.push(BulkOperationResult {
                                success: false,
                                data: None,
                                error: Some(resp.ret_msg.to_string()),
                                index: *orig_idx,
                            });
                            failure_count += 1;
                        }
                    } else if let Some(data) = resp.data {
                        // Map response back to original indices
                        for (response_idx, (orig_idx, _)) in
                            requests_with_indices.iter().enumerate()
                        {
                            if response_idx < data.list.len() {
                                let item = &data.list[response_idx];
                                let order_req = &req.requests[*orig_idx];
                                results.push(BulkOperationResult {
                                    success: true,
                                    data: Some(Order {
                                        order_id: OrderId::new(item.order_id.clone()),
                                        client_order_id: item.order_link_id.clone(),
                                        instrument: order_req.instrument.clone(),
                                        side: order_req.side.clone(),
                                        price: order_req.price,
                                        amount: order_req.amount,
                                        internal_created_at: chrono::Utc::now().timestamp_millis()
                                            as u64,
                                        exchange_created_at: None,
                                        status: OrderStatus::New,
                                        filled_amount: rust_decimal::Decimal::ZERO,
                                    }),
                                    error: None,
                                    index: *orig_idx,
                                });
                                success_count += 1;
                            } else {
                                results.push(BulkOperationResult {
                                    success: false,
                                    data: None,
                                    error: Some("Response missing order data".to_string()),
                                    index: *orig_idx,
                                });
                                failure_count += 1;
                            }
                        }
                    }
                }
                _ => {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Unexpected response type for batch create"
                    )));
                }
            }
        }

        // Sort results by original index
        results.sort_by_key(|r| r.index);

        Ok(BulkOrderResponse {
            results,
            success_count,
            failure_count,
        })
    }

    async fn bulk_cancel_orders(
        &self,
        req: BulkCancelRequest,
    ) -> Result<BulkCancelResponse, EngineError> {
        if req.order_ids.is_empty() {
            return Ok(BulkCancelResponse {
                results: vec![],
                cancelled_ids: vec![],
                failed_ids: vec![],
            });
        }

        // Get orders from state manager to determine category and symbol
        let mut cancel_items = Vec::new();
        let mut order_info = Vec::new(); // Track (order_id, category, index, found)
        let mut not_found_results = Vec::new(); // Track orders not found for immediate failure

        if let Some(state_mgr) = self.state_manager.lock().await.as_ref() {
            for (idx, order_id) in req.order_ids.iter().enumerate() {
                if let Some(order) = state_mgr.get_order_by_id(order_id).await {
                    let category = self.get_category_from_instrument_type(
                        order.instrument.instrument_type().clone(),
                    );
                    let symbol = order.instrument.symbol();

                    cancel_items.push(
                        super::websocket::types::rpc::bulk::BybitRpcBatchCancelOrderItem {
                            symbol,
                            order_id: Some(order_id.as_str().into()),
                            order_link_id: None,
                        },
                    );

                    order_info.push((order_id.clone(), category, idx, true));
                } else {
                    // Order not found - mark as failed immediately
                    not_found_results.push((order_id.clone(), idx));
                    order_info.push((order_id.clone(), BybitCategory::Spot, idx, false));
                }
            }
        } else {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "State manager not available for bulk cancel"
            )));
        }

        if cancel_items.is_empty() {
            // All orders not found
            return Ok(BulkCancelResponse {
                results: req
                    .order_ids
                    .iter()
                    .enumerate()
                    .map(|(idx, _order_id)| BulkOperationResult {
                        success: false,
                        data: None,
                        error: Some("Order not found in state manager".to_string()),
                        index: idx,
                    })
                    .collect(),
                cancelled_ids: vec![],
                failed_ids: req
                    .order_ids
                    .iter()
                    .map(|id| (id.clone(), "Order not found".to_string()))
                    .collect(),
            });
        }

        // Group by category (for now, assume all same category - could be improved)
        // Find first valid category from found orders
        let category = order_info
            .iter()
            .find_map(|(_, cat, _, found)| if *found { Some(cat.clone()) } else { None })
            .unwrap_or(BybitCategory::Spot);
        let batch_request = BybitRpcBatchCancelOrderRequest {
            req_id: Some(trading_core::utils::generate_custom_uuid().into()),
            header: super::websocket::types::rpc::BybitRpcRequestHeader {
                x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                x_bapi_recv_window: Some("5000".into()),
                referer: None,
            },
            op: "order.cancel-batch".into(),
            args: vec![
                super::websocket::types::rpc::bulk::BybitRpcBatchCancelOrderArgs {
                    category,
                    request: cancel_items,
                },
            ],
        };

        let mut results = Vec::new();
        let mut cancelled_ids = Vec::new();
        let mut failed_ids = Vec::new();

        // First, add results for orders not found
        for (order_id, idx) in not_found_results {
            results.push(BulkOperationResult {
                success: false,
                data: None,
                error: Some("Order not found in state manager".to_string()),
                index: idx,
            });
            failed_ids.push((order_id, "Order not found".to_string()));
        }

        match self
            .rpc_client
            .send_request(RpcRequest::BatchCancelOrder(batch_request))
            .await?
        {
            RpcResponse::BatchCancelOrder(resp) => {
                if resp.ret_code != 0 {
                    // All failed - only process orders that were found
                    for (order_id, _, idx, found) in order_info {
                        if found {
                            results.push(BulkOperationResult {
                                success: false,
                                data: None,
                                error: Some(resp.ret_msg.to_string()),
                                index: idx,
                            });
                            failed_ids.push((order_id, resp.ret_msg.to_string()));
                        }
                    }
                } else if let Some(data) = resp.data {
                    // Map response - only process orders that were found
                    let mut response_idx = 0;
                    for (order_id, _, orig_idx, found) in order_info {
                        if found {
                            if response_idx < data.list.len() {
                                let item = &data.list[response_idx];
                                if !item.order_id.is_empty() {
                                    results.push(BulkOperationResult {
                                        success: true,
                                        data: Some(order_id.clone()),
                                        error: None,
                                        index: orig_idx,
                                    });
                                    cancelled_ids.push(order_id);
                                } else {
                                    results.push(BulkOperationResult {
                                        success: false,
                                        data: None,
                                        error: Some("Empty order ID in response".to_string()),
                                        index: orig_idx,
                                    });
                                    failed_ids.push((order_id, "Empty order ID".to_string()));
                                }
                                response_idx += 1;
                            } else {
                                results.push(BulkOperationResult {
                                    success: false,
                                    data: None,
                                    error: Some("Response missing cancellation data".to_string()),
                                    index: orig_idx,
                                });
                                failed_ids.push((order_id, "Response missing data".to_string()));
                            }
                        }
                    }
                }
            }
            _ => {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "Unexpected response type for batch cancel"
                )));
            }
        }

        // Sort results by original index
        results.sort_by_key(|r| r.index);

        Ok(BulkCancelResponse {
            results,
            cancelled_ids,
            failed_ids,
        })
    }

    async fn bulk_amend_orders(
        &self,
        req: BulkAmendRequest,
    ) -> Result<BulkAmendResponse, EngineError> {
        if req.amendments.is_empty() {
            return Ok(BulkAmendResponse {
                results: vec![],
                amended_orders: vec![],
                failed_amendments: vec![],
            });
        }

        // Group by category (required by API) but process uniformly
        let mut amendments_by_category: Vec<(BybitCategory, Vec<(usize, BulkAmendItem)>)> =
            Vec::new();

        for (idx, amendment) in req.amendments.iter().enumerate() {
            let category = self.get_category_from_instrument_type(
                amendment.new_request.instrument.instrument_type(),
            );

            // Find or create category group
            if let Some((_, amendments)) = amendments_by_category
                .iter_mut()
                .find(|(cat, _)| *cat == category)
            {
                amendments.push((idx, amendment.clone()));
            } else {
                amendments_by_category.push((category, vec![(idx, amendment.clone())]));
            }
        }

        let mut results = Vec::new();
        let mut amended_orders = Vec::new();
        let mut failed_amendments = Vec::new();

        // Process all categories uniformly
        for (category, amendments_with_indices) in amendments_by_category {
            let amend_items: Vec<_> = amendments_with_indices
                .iter()
                .map(|(_, amendment)| {
                    let symbol = amendment.new_request.instrument.symbol();
                    super::websocket::types::rpc::bulk::BybitRpcBatchAmendOrderItem {
                        symbol: symbol.clone(),
                        order_id: Some(amendment.order_id.as_str().into()),
                        order_link_id: None,
                        price: Some(amendment.new_request.price),
                        qty: Some(amendment.new_request.amount),
                    }
                })
                .collect();

            let batch_request = BybitRpcBatchAmendOrderRequest {
                req_id: Some(trading_core::utils::generate_custom_uuid().into()),
                header: super::websocket::types::rpc::BybitRpcRequestHeader {
                    x_bapi_timestamp: chrono::Utc::now().timestamp_millis().to_string().into(),
                    x_bapi_recv_window: Some("5000".into()),
                    referer: None,
                },
                op: "order.amend-batch".into(),
                args: vec![
                    super::websocket::types::rpc::bulk::BybitRpcBatchAmendOrderArgs {
                        category,
                        request: amend_items,
                    },
                ],
            };

            match self
                .rpc_client
                .send_request(RpcRequest::BatchAmendOrder(batch_request))
                .await?
            {
                RpcResponse::BatchAmendOrder(resp) => {
                    if resp.ret_code != 0 {
                        // All failed for this category
                        for (orig_idx, amendment) in &amendments_with_indices {
                            results.push(BulkOperationResult {
                                success: false,
                                data: None,
                                error: Some(resp.ret_msg.to_string()),
                                index: *orig_idx,
                            });
                            failed_amendments
                                .push((amendment.order_id.clone(), resp.ret_msg.to_string()));
                        }
                    } else if let Some(data) = resp.data {
                        // Map response back to original indices
                        for (response_idx, (orig_idx, amendment)) in
                            amendments_with_indices.iter().enumerate()
                        {
                            if response_idx < data.list.len() {
                                let item = &data.list[response_idx];
                                if !item.order_id.is_empty() {
                                    let amended_order = Order {
                                        order_id: amendment.order_id.clone(),
                                        client_order_id: item.order_link_id.clone(),
                                        instrument: amendment.new_request.instrument.clone(),
                                        side: amendment.new_request.side.clone(),
                                        price: amendment.new_request.price,
                                        amount: amendment.new_request.amount,
                                        internal_created_at: chrono::Utc::now().timestamp_millis()
                                            as u64,
                                        exchange_created_at: None,
                                        status: OrderStatus::New,
                                        filled_amount: rust_decimal::Decimal::ZERO,
                                    };
                                    results.push(BulkOperationResult {
                                        success: true,
                                        data: Some(amended_order.clone()),
                                        error: None,
                                        index: *orig_idx,
                                    });
                                    amended_orders.push(amended_order);
                                } else {
                                    results.push(BulkOperationResult {
                                        success: false,
                                        data: None,
                                        error: Some("Empty order ID in response".to_string()),
                                        index: *orig_idx,
                                    });
                                    failed_amendments.push((
                                        amendment.order_id.clone(),
                                        "Empty order ID".to_string(),
                                    ));
                                }
                            } else {
                                results.push(BulkOperationResult {
                                    success: false,
                                    data: None,
                                    error: Some("Response missing amendment data".to_string()),
                                    index: *orig_idx,
                                });
                                failed_amendments.push((
                                    amendment.order_id.clone(),
                                    "Response missing data".to_string(),
                                ));
                            }
                        }
                    }
                }
                _ => {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Unexpected response type for batch amend"
                    )));
                }
            }
        }

        // Sort results by original index
        results.sort_by_key(|r| r.index);

        Ok(BulkAmendResponse {
            results,
            amended_orders,
            failed_amendments,
        })
    }

    async fn fetch_positions(&self) -> Result<Vec<Position>, EngineError> {
        if matches!(self.config.instrument_type, InstrumentType::Spot) {
            return Ok(Vec::new());
        }
        let response = self.fetch_position_list(None).await?;
        Ok(response.to_positions(self.symbol_registry.clone()))
    }

    async fn fetch_position(&self, pair: &Pair) -> Result<Option<Position>, EngineError> {
        if matches!(self.config.instrument_type, InstrumentType::Spot) {
            return Ok(None);
        }
        let response = self.fetch_position_list(Some(pair)).await?;
        Ok(response
            .to_positions(self.symbol_registry.clone())
            .into_iter()
            .find(|position| position.pair() == pair))
    }

    async fn fetch_collateral_balance(&self) -> Result<Vec<FuturesBalance>, EngineError> {
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        let category_str = match category {
            BybitCategory::Linear => "linear",
            BybitCategory::Inverse => "inverse",
            BybitCategory::Spot => "spot",
            BybitCategory::Option => "option",
        };
        let params = BybitWalletBalanceParams {
            account_type: Some("UNIFIED".into()),
            coin: None,
            category: Some(category_str.into()),
        };
        let response: BybitCollateralBalanceResponse = self
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
                EngineError::CustomError(anyhow::anyhow!("No response from wallet-balance"))
            })?;
        Ok(response.into())
    }

    async fn set_leverage(&self, pair: &Pair, leverage: u32) -> Result<LeverageInfo, EngineError> {
        if matches!(self.config.instrument_type, InstrumentType::Spot) {
            return Err(EngineError::NotSupported(
                "Leverage setting not supported for spot".into(),
            ));
        }
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        let leverage_str: smol_str::SmolStr = leverage.to_string().into();
        let request = BybitSetLeverageRequest {
            category,
            symbol: pair.convert_to_smolstr(),
            buy_leverage: leverage_str.clone(),
            sell_leverage: leverage_str,
        };
        let ack: BybitAckResponse = self
            .rest_client
            .call(
                "position/set-leverage",
                HttpMethod::POST,
                None,
                Some(request),
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from set-leverage"))
            })?;
        if ack.ret_code != 0 {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "set-leverage failed: {}",
                ack.ret_msg
            )));
        }
        Ok(LeverageInfo {
            pair: pair.clone(),
            leverage,
            max_leverage: leverage,
            last_update: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    async fn get_leverage(&self, pair: &Pair) -> Result<LeverageInfo, EngineError> {
        if matches!(self.config.instrument_type, InstrumentType::Spot) {
            return Err(EngineError::NotSupported(
                "Leverage query not supported for spot".into(),
            ));
        }
        let response = self.fetch_position_list(Some(pair)).await?;
        let row = response
            .result
            .list
            .iter()
            .find(|pos| {
                pos.symbol == pair.convert_to_smolstr()
                    || (!pos.base_coin.is_empty()
                        && !pos.settle_coin.is_empty()
                        && pos.base_coin == pair.base
                        && pos.settle_coin == pair.quote)
            })
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No leverage row for {}", pair))
            })?;
        let leverage = row.leverage.parse::<u32>().unwrap_or(0);
        Ok(LeverageInfo {
            pair: pair.clone(),
            leverage,
            max_leverage: leverage,
            last_update: row.updated_time.parse().unwrap_or(0),
        })
    }

    async fn set_position_mode(&self, mode: PositionMode) -> Result<(), EngineError> {
        if matches!(self.config.instrument_type, InstrumentType::Spot) {
            return Err(EngineError::NotSupported(
                "Position mode setting not supported for spot".into(),
            ));
        }
        let category = self.get_category_from_instrument_type(self.config.instrument_type.clone());
        let request = BybitSwitchModeRequest {
            category,
            symbol: self
                .config
                .pairs
                .first()
                .map(|pair| pair.convert_to_smolstr()),
            mode: position_mode_to_bybit(mode),
        };
        let ack: BybitAckResponse = self
            .rest_client
            .call(
                "position/switch-mode",
                HttpMethod::POST,
                None,
                Some(request),
                None,
                true,
            )
            .await?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("No response from switch-mode"))
            })?;
        if ack.ret_code != 0 {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "switch-mode failed: {}",
                ack.ret_msg
            )));
        }
        Ok(())
    }

    async fn get_position_mode(&self) -> Result<PositionMode, EngineError> {
        if matches!(self.config.instrument_type, InstrumentType::Spot) {
            return Err(EngineError::NotSupported(
                "Position mode query not supported for spot".into(),
            ));
        }
        let response = self.fetch_position_list(None).await?;
        let idx = response
            .result
            .list
            .first()
            .map(|pos| pos.position_idx)
            .unwrap_or(0);
        Ok(position_idx_to_mode(idx))
    }
}
