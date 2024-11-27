// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    config::BinanceConfig, rest::http_client::BinanceHttpClient,
    types::open_order::BinanceOrderRequest,
};
use core_logic::error::EngineError;
use reqwest::Method;
use smol_str::SmolStr;

const OPEN_ORDER: &str = "order/test";

#[derive(Clone)]
pub struct BinanceDriver {
    client: BinanceHttpClient,
    _config: BinanceConfig,
}

impl BinanceDriver {
    pub fn new(client: BinanceHttpClient, _config: BinanceConfig) -> Self {
        BinanceDriver { client, _config }
    }

    pub async fn open_order(&self, order: BinanceOrderRequest) -> Result<SmolStr, EngineError> {
        let body = Some(&order);

        self.client.call(OPEN_ORDER, Method::POST, None, body).await
    }
}
