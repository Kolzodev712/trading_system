// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    config::BinanceConfig,
    rest::{
        client::BinanceHttpClient,
        types::{
            balance::{BinanceAccountInfo, BinanceBalanceRequest},
            common::BinanceServerTime,
            orderbook::{BinanceOrderbookSnapshotRequest, BinanceOrderbookSnapshotResponse},
        },
    },
};
use core_logic::{
    data::{macros::MillisecondTimestamp, private_data::Balance},
    error::EngineError,
    rest_client::HttpMethod,
};

const TIME: &str = "time";
const ORDERBOOK: &str = "depth";
const BALANCE: &str = "account";

#[derive(Debug, Clone)]
pub struct BinanceRestClient {
    http_client: BinanceHttpClient,
    config: BinanceConfig,
}

impl BinanceRestClient {
    pub fn new(http_client: BinanceHttpClient, config: BinanceConfig) -> Self {
        BinanceRestClient {
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

    pub async fn fetch_orderbook(&self) -> Result<BinanceOrderbookSnapshotResponse, EngineError> {
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
