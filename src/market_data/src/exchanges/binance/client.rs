use std::vec;

use super::{
    config::BinanceConfig,
    rest::binance_rest::BinanceExchangeHandler,
    rest::types::{
        AccountInfo, BinanceDataType, BinanceOrderbookSnapshot, BinancePrivateEndpoint,
        BinanceTradeSnapshot,
    },
};
use core_logic::{
    data::{OrderBook, Trade},
    error::KolzoError,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]

pub struct BinanceClient {
    pub config: BinanceConfig,
    pub http_client: BinanceExchangeHandler,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, http_client: BinanceExchangeHandler) -> Self {
        Self {
            config,
            http_client,
        }
    }

    pub async fn get_rest_orderbooks(&self) -> Result<Vec<OrderBook>, KolzoError> {
        let orderbook_handler = BinanceExchangeHandler::new_public(
            BinanceDataType::Orderbook,
            self.config.symbol.clone(),
            self.config.limit,
            None,
            None,
            None,
            None,
        );
        let orderbook_snapshots: BinanceOrderbookSnapshot = orderbook_handler.get().await?;
        let orderbook: OrderBook = orderbook_snapshots.into();

        Ok(vec![orderbook])
    }

    pub async fn get_rest_trades(&self) -> Result<Vec<Trade>, KolzoError> {
        let trade_handler = BinanceExchangeHandler::new_public(
            BinanceDataType::Trade,
            self.config.symbol.clone(),
            None,
            None,
            None,
            None,
            None,
        );
        let binance_trades: Vec<BinanceTradeSnapshot> = trade_handler.get().await?;
        let trades: Vec<Trade> = binance_trades.into_iter().map(Trade::from).collect();

        Ok(trades)
    }

    pub async fn get_account_data(&self) -> Result<Vec<AccountInfo>, KolzoError> {
        let account_info_handler = BinanceExchangeHandler::new_private(
            BinancePrivateEndpoint::AccountInfo,
            None,
            None,
            Some(self.config.api_key.clone()),
            Some(self.config.secret_key.clone()),
        );

        let account_info_raw: AccountInfo = account_info_handler.get().await?;

        Ok(vec![account_info_raw])
    }
}
