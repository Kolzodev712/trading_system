use super::{
    binance_rest::BinanceExchangeHandler,
    config::BinanceConfig,
    types::{BinancePrivateEndpoint, BinancePublicEndpoint},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]

pub struct BinanceClient {
    pub config: BinanceConfig,
    pub http_client: BinanceExchangeHandler,
}
