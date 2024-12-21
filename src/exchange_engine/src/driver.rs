use crate::exchanges::binance::{
    client::BinanceClient,
    config::BinanceConfig,
    rest::client::BinanceHttpClient,
    types::{
        cancel_all::BinanceCancelAllOrdersResponse, cancel_order::BinanceCancelOrderByIdResponse,
        common::BinanceSymbol, open_order::BinanceOrderRequest,
    },
};
use core_logic::{
    data::{
        base_data::{OrderId, Pair},
        private_data::Order,
    },
    error::EngineError,
    traits::TradingClient,
    Exchange,
};

use super::config::ExchangeEngineConfig;

use async_trait::async_trait;
use tracing::debug;

#[derive(Debug, Clone)]
pub enum Method {
    TestOpenOrder(BinanceOrderRequest),
    OpenOrder(BinanceOrderRequest),
    CancelOrderById(OrderId),
    CancelAllOrders(Pair),
}

#[derive(Debug, Clone)]
pub struct ExchangeEngineDriver {
    config: ExchangeEngineConfig,
}

impl ExchangeEngineDriver {
    pub fn new(config: ExchangeEngineConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self, method: Method) -> Result<(), EngineError> {
        match method {
            Method::TestOpenOrder(binance_order_request) => {
                let order = self.test_open_order(binance_order_request).await?;
                debug!("Order placed successfully: {:?}", order);
            }

            Method::OpenOrder(binance_order_request) => {
                let order = self.open_order(binance_order_request).await?;
                debug!("Order placed successfully: {:?}", order);
            }

            Method::CancelOrderById(order_id) => {
                self.cancel_order_by_id(order_id).await?;
                debug!("Order canceled successfully.");
            }

            Method::CancelAllOrders(pair) => {
                self.cancel_all_orders(&pair).await?;
                debug!("All orders canceled successfully.");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl TradingClient<BinanceOrderRequest> for ExchangeEngineDriver {
    const EXCHANGE_ID: Exchange = Exchange::Binance;

    async fn test_open_order(&self, req: BinanceOrderRequest) -> Result<(), EngineError> {
        let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
        let http_client = BinanceHttpClient::new(binance_config.clone());
        let client = BinanceClient::new(http_client, binance_config.clone());

        let mut binance_order_request = req;

        let time = client.get_time().await?;
        binance_order_request.timestamp = Some(time.server_time);

        let _response = client.open_order_test(binance_order_request).await?;
        Ok(())
    }

    async fn open_order(&self, req: BinanceOrderRequest) -> Result<Order, EngineError> {
        let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
        let http_client = BinanceHttpClient::new(binance_config.clone());
        let client = BinanceClient::new(http_client, binance_config.clone());

        let mut binance_order_request = req;

        let time = client.get_time().await?;
        binance_order_request.timestamp = Some(time.server_time);

        let response = client.open_order(binance_order_request).await?;
        Ok(response.into())
    }

    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
        let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
        let http_client = BinanceHttpClient::new(binance_config.clone());
        let client = BinanceClient::new(http_client, binance_config.clone());

        let binance_order_id = order_id.as_str().parse::<u64>().map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Failed to parse OrderId into u64: {}", e))
        })?;

        let response: Vec<BinanceCancelOrderByIdResponse> =
            client.cancel_order_by_id(binance_order_id).await?;

        if response.is_empty() {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "No matching order found to cancel."
            )));
        }

        Ok(())
    }

    async fn cancel_all_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
        let http_client = BinanceHttpClient::new(binance_config.clone());
        let client = BinanceClient::new(http_client, binance_config.clone());

        let binance_symbol: BinanceSymbol = pair.clone().into();

        let response: Vec<BinanceCancelAllOrdersResponse> = client.cancel_all().await?;

        if response.is_empty() {
            tracing::warn!(
                "No open orders to cancel for pair: {}",
                binance_symbol.convert_to_smolstr()
            );
        }

        Ok(())
    }
}
