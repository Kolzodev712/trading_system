// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    config::BinanceConfig,
    rest::http_client::BinanceHttpClient,
    types::{
        cancel_all::{BinanceCancelAllOrdersRequest, BinanceCancelAllOrdersResponse},
        common::BinanceServerTime,
        open_order::{BinanceOrderRequest, BinanceOrderResponse},
        orderbook::{BinanceOrderbookSnapshotRequest, BinanceOrderbookSnapshotResponse},
    },
};
use core_logic::{error::EngineError, rest_client::HttpMethod};
use tracing::{error, info};

const TIME: &str = "time";
const ORDERBOOK: &str = "depth";
const ORDER: &str = "order/test";
const CANCEL_ALL: &str = "openOrders";

#[derive(Debug)]
pub struct BinanceDriver {
    client: BinanceHttpClient,
    config: BinanceConfig,
}

impl BinanceDriver {
    pub fn new(client: BinanceHttpClient, config: BinanceConfig) -> Self {
        BinanceDriver { client, config }
    }

    pub async fn get_time(&self) -> Result<BinanceServerTime, EngineError> {
        let response: Option<Option<BinanceServerTime>> = self
            .client
            .call::<(), Option<BinanceServerTime>>(TIME, HttpMethod::GET, None, None, false)
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected BinanceServerTime, but received None"
            ))
        })
    }
    pub async fn get_orderbook(&self) -> Result<BinanceOrderbookSnapshotResponse, EngineError> {
        let params = BinanceOrderbookSnapshotRequest {
            symbol: self.config.symbol.convert_to_smolstr(),
            limit: None,
        };

        let response = self
            .client
            .call::<BinanceOrderbookSnapshotRequest, Option<BinanceOrderbookSnapshotResponse>>(
                ORDERBOOK,
                HttpMethod::GET,
                Some(params),
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
            .client
            .call::<BinanceOrderRequest, Option<BinanceOrderResponse>>(
                ORDER,
                HttpMethod::POST,
                params,
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
        let params = Some(order);

        match self
            .client
            .call::<BinanceOrderRequest, Option<BinanceOrderResponse>>(
                ORDER,
                HttpMethod::POST,
                params,
                None,
                true,
            )
            .await
        {
            Ok(Some(_)) => Ok(()),
            Ok(None) => {
                info!("Binance order test call returned no content as expected.");
                Ok(())
            }
            Err(e) => {
                error!("Error during Binance order test call: {}", e);
                Err(e)
            }
        }
    }

    pub async fn cancel_all(&self) -> Result<Vec<BinanceCancelAllOrdersResponse>, EngineError> {
        let time = self.get_time().await?.server_time;

        let params = BinanceCancelAllOrdersRequest {
            symbol: self.config.symbol.convert_to_smolstr(),
            timestamp: time,
        };

        let response = self
            .client
            .call::<BinanceCancelAllOrdersRequest, Option<Vec<BinanceCancelAllOrdersResponse>>>(
                CANCEL_ALL,
                HttpMethod::DELETE,
                Some(params),
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
}
