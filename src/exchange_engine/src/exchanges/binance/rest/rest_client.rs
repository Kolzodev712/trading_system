// End goal here is to implement the `TradingClient` trait on the binance client
use super::{
    client::BinanceHttpClient,
    types::{
        balance::{BinanceAccountInfo, BinanceBalanceRequest},
        common::BinanceServerTime,
    },
};
use trading_core::{
    error::EngineError,
    rest::HttpMethod,
    types::{common::MillisecondTimestamp, exchange::Balance},
};

const TIME: &str = "time";
const BALANCE: &str = "account";

#[derive(Debug, Clone)]
pub struct BinanceRestClient {
    http_client: BinanceHttpClient,
}

impl BinanceRestClient {
    pub fn new(http_client: BinanceHttpClient) -> Self {
        BinanceRestClient { http_client }
    }

    pub async fn get_time(&self) -> Result<BinanceServerTime, EngineError> {
        let response: Option<Option<BinanceServerTime>> = self
            .http_client
            .call::<(), Option<BinanceServerTime>>(TIME, HttpMethod::GET, None, None, None, false)
            .await?;

        response.flatten().ok_or_else(|| {
            EngineError::CustomError(smol_str::format_smolstr!(
                "Expected BinanceServerTime, but received None"
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
            EngineError::CustomError(smol_str::format_smolstr!(
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
