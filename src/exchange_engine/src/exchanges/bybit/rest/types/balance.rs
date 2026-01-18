//! Bybit wallet balance response types
//! See: <https://bybit-exchange.github.io/docs/v5/account/wallet-balance>

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::{
    types::{common::Symbol, exchange::Balance},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitWalletBalanceResponse {
    pub ret_code: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub ret_msg: SmolStr,
    pub result: BybitWalletBalanceResult,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitWalletBalanceResult {
    pub list: Vec<BybitAccountBalance>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitAccountBalance {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub account_type: SmolStr,
    pub coin: Vec<BybitCoinBalance>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitCoinBalance {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub coin: SmolStr,
    #[serde(with = "rust_decimal::serde::str")]
    pub wallet_balance: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub locked: Decimal,
}

impl From<BybitCoinBalance> for Balance {
    fn from(coin: BybitCoinBalance) -> Self {
        let free = coin.wallet_balance - coin.locked;
        Balance {
            symbol: Symbol::from(&coin.coin),
            free,
            used: coin.locked,
            total: coin.wallet_balance,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitWalletBalanceParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_type: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coin: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<SmolStr>,
}

impl From<BybitWalletBalanceResponse> for Vec<Balance> {
    fn from(response: BybitWalletBalanceResponse) -> Self {
        response
            .result
            .list
            .into_iter()
            .flat_map(|account| account.coin)
            .map(Balance::from)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_bybit_wallet_balance_response_deserialization() {
        let json = r#"
        {
            "retCode": 0,
            "retMsg": "OK",
            "result": {
                "list": [
                    {
                        "accountType": "UNIFIED",
                        "coin": [
                            {
                                "coin": "BTC",
                                "walletBalance": "1.5",
                                "locked": "0.1"
                            },
                            {
                                "coin": "USDT",
                                "walletBalance": "10000.0",
                                "locked": "500.0"
                            }
                        ]
                    }
                ]
            },
            "retExtInfo": {},
            "time": 1690872862481
        }
        "#;

        let response: BybitWalletBalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert_eq!(response.ret_msg.as_str(), "OK");
        assert_eq!(response.result.list.len(), 1);
        assert_eq!(response.result.list[0].account_type.as_str(), "UNIFIED");
        assert_eq!(response.result.list[0].coin.len(), 2);
    }

    #[test]
    fn test_bybit_coin_balance_to_balance() {
        let coin_balance = BybitCoinBalance {
            coin: SmolStr::new("BTC"),
            wallet_balance: Decimal::new(150, 1),
            locked: Decimal::new(10, 1),
        };

        let balance: Balance = coin_balance.into();
        assert_eq!(balance.symbol, "BTC");
        assert_eq!(balance.free, Decimal::new(140, 1));
        assert_eq!(balance.used, Decimal::new(10, 1));
        assert_eq!(balance.total, Decimal::new(150, 1));
    }

    #[test]
    fn test_bybit_wallet_balance_response_to_balance_vec() {
        let response = BybitWalletBalanceResponse {
            ret_code: 0,
            ret_msg: SmolStr::new("OK"),
            result: BybitWalletBalanceResult {
                list: vec![BybitAccountBalance {
                    account_type: SmolStr::new("UNIFIED"),
                    coin: vec![
                        BybitCoinBalance {
                            coin: SmolStr::new("BTC"),
                            wallet_balance: Decimal::new(150, 1),
                            locked: Decimal::new(10, 1),
                        },
                        BybitCoinBalance {
                            coin: SmolStr::new("USDT"),
                            wallet_balance: Decimal::new(100000, 2),
                            locked: Decimal::new(50000, 2),
                        },
                    ],
                }],
            },
            ret_ext_info: None,
            time: 1690872862481,
        };

        let balances: Vec<Balance> = response.into();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].symbol, "BTC");
        assert_eq!(balances[0].total, Decimal::new(150, 1));
        assert_eq!(balances[1].symbol, "USDT");
        assert_eq!(balances[1].total, Decimal::new(100000, 2));
    }

    #[test]
    fn test_bybit_coin_balance_zero_locked() {
        let coin_balance = BybitCoinBalance {
            coin: SmolStr::new("ETH"),
            wallet_balance: Decimal::new(5, 0),
            locked: Decimal::ZERO,
        };

        let balance: Balance = coin_balance.into();
        assert_eq!(balance.free, Decimal::new(5, 0));
        assert_eq!(balance.used, Decimal::ZERO);
        assert_eq!(balance.total, Decimal::new(5, 0));
    }

    #[test]
    fn test_bybit_wallet_balance_empty_coins() {
        let response = BybitWalletBalanceResponse {
            ret_code: 0,
            ret_msg: SmolStr::new("OK"),
            result: BybitWalletBalanceResult {
                list: vec![BybitAccountBalance {
                    account_type: SmolStr::new("UNIFIED"),
                    coin: vec![],
                }],
            },
            ret_ext_info: None,
            time: 1690872862481,
        };

        let balances: Vec<Balance> = response.into();
        assert_eq!(balances.len(), 0);
    }
}
