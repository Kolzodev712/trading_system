// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    config::BinanceConfig,
    rest::http_client::BinanceHttpClient,
    types::{
        common::BinanceServerTime,
        open_order::{BinanceOrderRequest, BinanceOrderResponse},
        orderbook::{BinanceOrderbookSnapshotRequest, BinanceOrderbookSnapshotResponse},
    },
};
use core_logic::{error::EngineError, rest_client::HttpMethod};

const TIME: &str = "time";
const ORDERBOOK: &str = "depth";
const ORDER: &str = "order/test";

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
        let response: BinanceServerTime = self
            .client
            .call::<(), BinanceServerTime>(TIME, HttpMethod::GET, None, None, false)
            .await?;
        Ok(response)
    }

    pub async fn get_orderbook(&self) -> Result<BinanceOrderbookSnapshotResponse, EngineError> {
        let params = BinanceOrderbookSnapshotRequest {
            symbol: self.config.symbol.convert_to_smolstr(),
            limit: None,
        };

        let response: Option<BinanceOrderbookSnapshotResponse> = self
            .client
            .call::<BinanceOrderbookSnapshotRequest, Option<BinanceOrderbookSnapshotResponse>>(
                ORDERBOOK,
                HttpMethod::GET,
                Some(params),
                None,
                false,
            )
            .await?;

        response.ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Expected orderbook snapshot, but received None"
            ))
        })
    }

    pub async fn open_order(
        &self,
        order: BinanceOrderRequest,
    ) -> Result<BinanceOrderResponse, EngineError> {
        let params = Some(&order);

        self.client
            .call(ORDER, HttpMethod::POST, params, None, true)
            .await
    }

    pub async fn open_order_test(&self, order: BinanceOrderRequest) -> Result<(), EngineError> {
        let params = Some(&order).unwrap().clone();

        let _: Option<BinanceOrderResponse> = self
            .client
            .call::<BinanceOrderRequest, Option<BinanceOrderResponse>>(
                ORDER,
                HttpMethod::POST,
                Some(params),
                None,
                true,
            )
            .await?;
        Ok(())
    }
}
