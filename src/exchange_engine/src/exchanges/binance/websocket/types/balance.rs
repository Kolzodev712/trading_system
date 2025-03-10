use core_logic::data::{
    base_data::AccountType,
    macros::{MillisecondTimestamp, Symbol},
    private_data::{AccountInfo, Balance},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceAccountType {
    Spot,
    Margin,
    Futures,
    Savings,
    Wallet,
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAccountInfo {
    /// The balances of the account
    pub balances: Vec<BinanceBalance>,
    /// The account type
    pub account_type: BinanceAccountType,
    /// Unix millisecond timestamp when an update was received from the exchange
    pub update_time: MillisecondTimestamp,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceBalance {
    /// Currency symbol
    pub asset: Symbol,
    /// The free balance that can be used in new orders
    pub free: Decimal,
    /// The locked balance that is bound to open orders
    pub locked: Decimal,
}

impl From<BinanceAccountType> for AccountType {
    fn from(binance_account_type: BinanceAccountType) -> Self {
        match binance_account_type {
            BinanceAccountType::Spot => AccountType::Spot,
            BinanceAccountType::Futures => AccountType::Futures,
            _ => AccountType::Unknown,
        }
    }
}

impl From<BinanceBalance> for Balance {
    fn from(binance_balance: BinanceBalance) -> Self {
        Balance {
            symbol: binance_balance.asset,
            free: binance_balance.free,
            used: binance_balance.locked,
            total: binance_balance.free + binance_balance.locked,
        }
    }
}

impl From<BinanceAccountInfo> for AccountInfo {
    fn from(binance_account_info: BinanceAccountInfo) -> Self {
        AccountInfo {
            balances: binance_account_info
                .balances
                .into_iter()
                .map(|balance| balance.into())
                .collect(),
            account_type: binance_account_info.account_type.into(),
            update_time: binance_account_info.update_time,
        }
    }
}
