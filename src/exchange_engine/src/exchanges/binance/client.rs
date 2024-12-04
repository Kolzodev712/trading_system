// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    config::BinanceConfig,
    rest::http_client::BinanceHttpClient,
    types::{common::BinanceServerTime, open_order::BinanceOrderRequest},
};
use core_logic::error::EngineError;
use reqwest::Method;
use smol_str::SmolStr;

const TIME: &str = "time";
const OPEN_ORDER: &str = "order/test";

#[derive(Debug)]
pub struct BinanceDriver {
    client: BinanceHttpClient,
    _config: BinanceConfig,
}

impl BinanceDriver {
    pub fn new(client: BinanceHttpClient, _config: BinanceConfig) -> Self {
        BinanceDriver { client, _config }
    }

    pub async fn get_time(&self) -> Result<BinanceServerTime, EngineError> {
        let response: BinanceServerTime = self
            .client
            .unsigned_call::<(), BinanceServerTime>(TIME, None)
            .await?;
        Ok(response)
    }

    pub async fn open_order(&self, order: BinanceOrderRequest) -> Result<SmolStr, EngineError> {
        let body = Some(&order);

        self.client
            .signed_call(OPEN_ORDER, Method::POST, None, body)
            .await
    }
}
