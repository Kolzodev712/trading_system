// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    config::BinanceConfig,
    rest::{
        client::BinanceHttpClient,
        types::{
            balance::{BinanceAccountInfo, BinanceBalanceRequest},
            cancel_all::{BinanceCancelAllOrdersRequest, BinanceCancelAllOrdersResponse},
            cancel_order::{BinanceCancelOrderByIdRequest, BinanceCancelOrderByIdResponse},
            common::{BinanceListenKey, BinanceServerTime},
            open_order::{BinanceOrderRequest, BinanceOrderResponse},
            orderbook::{BinanceOrderbookSnapshotRequest, BinanceOrderbookSnapshotResponse},
        },
    },
};
use anyhow::anyhow;
use async_trait::async_trait;
use core_logic::{
    data::{
        base_data::{OrderId, Pair},
        macros::MillisecondTimestamp,
        private_data::{Balance, Order, OrderRequest},
    },
    error::EngineError,
    rest_client::HttpMethod,
    traits::TradingClient,
    Exchange,
};
use std::collections::HashMap;
use tracing::{error, info};

const TIME: &str = "time";
const ORDERBOOK: &str = "depth";
const ORDER: &str = "order/test";
const CANCEL_ALL: &str = "openOrders";
const BALANCE: &str = "account";
const LISTEN_KEY: &str = "userDataStream";

#[derive(Debug, Clone)]
pub struct BinanceClient {
    http_client: BinanceHttpClient,
    config: BinanceConfig,
}

impl BinanceClient {
    pub fn new(http_client: BinanceHttpClient, config: BinanceConfig) -> Self {
        BinanceClient {
            http_client,
            config,
        }
    }

    pub async fn get_time(&self) -> Result<BinanceServerTime, EngineError> {
        let response: Option<Option<BinanceServerTime>> = self
            .http_client
            .call::<(), Option<BinanceServerTime>>(TIME, HttpMethod::GET, None, None, None, false)
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected BinanceServerTime, but received None"
            ))
        })
    }

    pub async fn create_listen_key(&self, api_key: &str) -> Result<String, EngineError> {
        let mut custom_headers = HashMap::new();
        custom_headers.insert("X-MBX-APIKEY".to_string(), api_key.into());

        let response = self
            .http_client
            .call::<(), BinanceListenKey>(
                LISTEN_KEY,
                HttpMethod::POST,
                None,
                None,
                Some(custom_headers),
                false,
            )
            .await?;

        match response {
            Some(resp) => Ok(resp.listen_key),
            None => Err(EngineError::AuthenticationError(anyhow!(
                "ListenKey generation error"
            ))),
        }
    }

    pub async fn get_orderbook(&self) -> Result<BinanceOrderbookSnapshotResponse, EngineError> {
        let params = BinanceOrderbookSnapshotRequest {
            symbol: self.config.symbol.convert_to_smolstr(),
            limit: None,
        };

        let response = self
            .http_client
            .call::<BinanceOrderbookSnapshotRequest, Option<BinanceOrderbookSnapshotResponse>>(
                ORDERBOOK,
                HttpMethod::GET,
                Some(params),
                None,
                None,
                false,
            )
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected orderbook snapshot, but received None"
            ))
        })
    }

    pub async fn open_order(
        &self,
        order: BinanceOrderRequest,
    ) -> Result<BinanceOrderResponse, EngineError> {
        let params = Some(order);

        let response = self
            .http_client
            .call::<BinanceOrderRequest, Option<BinanceOrderResponse>>(
                ORDER,
                HttpMethod::POST,
                params,
                None,
                None,
                true,
            )
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected order response, but received None"
            ))
        })
    }

    pub async fn open_order_test(&self, order: BinanceOrderRequest) -> Result<(), EngineError> {
        let params = Some(order.clone());

        match self
            .http_client
            .call::<BinanceOrderRequest, Option<BinanceOrderResponse>>(
                ORDER,
                HttpMethod::POST,
                params,
                None,
                None,
                true,
            )
            .await
        {
            Ok(Some(_)) => Ok(()),
            Ok(None) => {
                info!("✅ Binance order test call returned no content as expected.");
                Ok(())
            }
            Err(e) => {
                error!("❌ Error during Binance order test call: {}", e);
                Err(e)
            }
        }
    }

    pub async fn cancel_all(&self) -> Result<Vec<BinanceCancelAllOrdersResponse>, EngineError> {
        let time = self.get_time().await?;

        let params = BinanceCancelAllOrdersRequest {
            symbol: self.config.symbol.convert_to_smolstr(),
            timestamp: time.server_time,
        };

        let response = self
            .http_client
            .call::<BinanceCancelAllOrdersRequest, Option<Vec<BinanceCancelAllOrdersResponse>>>(
                CANCEL_ALL,
                HttpMethod::DELETE,
                Some(params),
                None,
                None,
                true,
            )
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected order response, but received None"
            ))
        })
    }

    pub async fn cancel_order_by_id(
        &self,
        order_id: u64,
    ) -> Result<Vec<BinanceCancelOrderByIdResponse>, EngineError> {
        let time = self.get_time().await?;

        let params = BinanceCancelOrderByIdRequest {
            symbol: self.config.symbol.convert_to_smolstr(),
            order_id,
            timestamp: time.server_time,
        };

        let response = self
            .http_client
            .call::<BinanceCancelOrderByIdRequest, Option<Vec<BinanceCancelOrderByIdResponse>>>(
                CANCEL_ALL,
                HttpMethod::DELETE,
                Some(params),
                None,
                None,
                true,
            )
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected order response, but received None"
            ))
        })
    }

    pub async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        let time = self.get_time().await?;

        let params = BinanceBalanceRequest {
            omit_zero_balances: Some(true),
            timestamp: MillisecondTimestamp(time.server_time),
        };

        let response = self
            .http_client
            .call::<BinanceBalanceRequest, Option<BinanceAccountInfo>>(
                BALANCE,
                HttpMethod::GET,
                Some(params),
                None,
                None,
                true,
            )
            .await?;

        let binance_account_info = response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected order response, but received None"
            ))
        })?;

        let balances: Vec<Balance> = binance_account_info
            .balances
            .into_iter()
            .map(|binance_balance| Balance {
                symbol: binance_balance.asset,
                free: binance_balance.free,
                used: binance_balance.locked,
                total: binance_balance.free + binance_balance.locked,
            })
            .collect();

        Ok(balances)
    }
}

#[async_trait]
impl TradingClient<OrderRequest, Pair> for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    /// ✅ Test an order before actually submitting it
    async fn test_open_order(&self, req: OrderRequest) -> Result<(), EngineError> {
        let binance_order: BinanceOrderRequest = req.into();

        let time = self.get_time().await?;
        let mut order_request = binance_order;
        order_request.timestamp = Some(time.server_time);

        self.open_order_test(order_request).await?;
        Ok(())
    }

    /// ✅ Submit an actual order to Binance
    async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        let binance_order: BinanceOrderRequest = req.into();

        let time = self.get_time().await?;
        let mut order_request = binance_order;
        order_request.timestamp = Some(time.server_time);

        let response = self.open_order(order_request).await?;
        Ok(response.into())
    }

    /// ✅ Cancel a specific order by order ID
    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        let binance_order_id = order_id.as_str().parse::<u64>().map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to parse OrderId into u64: {}", e))
        })?;

        let response = self.cancel_order_by_id(binance_order_id).await?;

        if response.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "No matching order found to cancel."
            )));
        }

        Ok(())
    }

    /// ✅ Cancel all open orders for a trading pair
    async fn cancel_all_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        let response = self.cancel_all().await?;

        if response.is_empty() {
            tracing::warn!(
                "No open orders to cancel for pair: {}",
                pair.convert_to_smolstr()
            );
        }

        Ok(())
    }

    /// ✅ Fetch the account balances
    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        let response = self.fetch_balance().await?;

        if response.is_empty() {
            tracing::warn!("No balances found");
        }

        Ok(response)
    }
}
