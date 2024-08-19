use std::vec;

use super::{
    config::BinanceConfig,
    rest::{
        binance_rest::BinanceExchangeHandler,
        types::{
            AccountInfo, BinanceDataType, BinanceOrderRequest, BinanceOrderbookSnapshot,
            BinancePrivateEndpoint, BinanceTradeSnapshot,
        },
    },
};
use core_logic::{
    data::public_data::{OrderBook, Trade},
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
            self.config.api_key.clone().into(),
            self.config.secret_key.clone().into(),
        );

        let account_info_raw: AccountInfo = account_info_handler.get().await?;

        Ok(vec![account_info_raw])
    }

    pub async fn place_order(&self, order_request: BinanceOrderRequest) -> Result<(), KolzoError> {
        let private_endpoint = BinancePrivateEndpoint::PlaceOrder {
            symbol: order_request.symbol.into(),
            side: order_request.side.into(),
            order_type: order_request.order_type.into(),
            quantity: order_request.quantity.unwrap_or_default(),
            price: order_request.price,
            time_in_force: order_request.time_in_force.map(|tif| tif.into()),
        };

        let order_handler = BinanceExchangeHandler::new_private(
            private_endpoint,
            None,
            Some(order_request.timestamp),
            self.config.api_key.clone().into(),
            self.config.secret_key.clone().into(),
        );

        order_handler.post::<()>(None).await
    }
}
