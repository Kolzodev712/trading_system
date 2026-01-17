//! Bybit collateral balance response types
//! See: <https://bybit-exchange.github.io/docs/v5/account/collateral-info>

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::{
    types::{common::Symbol, futures::FuturesBalance},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitCollateralBalanceResponse {
    pub ret_code: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub ret_msg: SmolStr,
    pub result: BybitCollateralBalanceResult,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitCollateralBalanceResult {
    pub list: Vec<BybitCollateralAccountBalance>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitCollateralAccountBalance {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub account_type: SmolStr,
    pub coin: Vec<BybitCollateralCoinBalance>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitCollateralCoinBalance {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub coin: SmolStr,
    #[serde(with = "rust_decimal::serde::str")]
    pub wallet_balance: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub available_to_withdraw: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub unrealised_pnl: Decimal,
}

impl From<BybitCollateralCoinBalance> for FuturesBalance {
    fn from(coin: BybitCollateralCoinBalance) -> Self {
        FuturesBalance {
            asset: Symbol::from(&coin.coin),
            available_balance: coin.wallet_balance - coin.unrealised_pnl,
            wallet_balance: coin.wallet_balance,
            cross_wallet_balance: coin.wallet_balance,
            cross_unrealized_pnl: coin.unrealised_pnl,
            available_withdraw: coin.available_to_withdraw,
            last_update: 0,
        }
    }
}

impl From<BybitCollateralBalanceResponse> for Vec<FuturesBalance> {
    fn from(response: BybitCollateralBalanceResponse) -> Self {
        response
            .result
            .list
            .into_iter()
            .flat_map(|account| account.coin)
            .map(FuturesBalance::from)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_bybit_collateral_balance_response_deserialization() {
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
                                "coin": "USDT",
                                "walletBalance": "10000.0",
                                "availableToWithdraw": "9500.0",
                                "unrealisedPnl": "100.0"
                            },
                            {
                                "coin": "USDC",
                                "walletBalance": "5000.0",
                                "availableToWithdraw": "4800.0",
                                "unrealisedPnl": "-50.0"
                            }
                        ]
                    }
                ]
            },
            "retExtInfo": {},
            "time": 1690872862481
        }
        "#;

        let response: BybitCollateralBalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert_eq!(response.ret_msg.as_str(), "OK");
        assert_eq!(response.result.list.len(), 1);
        assert_eq!(response.result.list[0].account_type.as_str(), "UNIFIED");
        assert_eq!(response.result.list[0].coin.len(), 2);
    }

    #[test]
    fn test_bybit_collateral_coin_balance_to_futures_balance() {
        let coin_balance = BybitCollateralCoinBalance {
            coin: SmolStr::new("USDT"),
            wallet_balance: Decimal::new(100000, 2),
            available_to_withdraw: Decimal::new(95000, 2),
            unrealised_pnl: Decimal::new(10000, 2),
        };

        let futures_balance: FuturesBalance = coin_balance.into();
        assert_eq!(futures_balance.asset, "USDT");
        assert_eq!(futures_balance.wallet_balance, Decimal::new(100000, 2));
        assert_eq!(futures_balance.available_balance, Decimal::new(90000, 2));
        assert_eq!(
            futures_balance.cross_wallet_balance,
            Decimal::new(100000, 2)
        );
        assert_eq!(futures_balance.cross_unrealized_pnl, Decimal::new(10000, 2));
        assert_eq!(futures_balance.available_withdraw, Decimal::new(95000, 2));
    }

    #[test]
    fn test_bybit_collateral_balance_response_to_futures_balance_vec() {
        let response = BybitCollateralBalanceResponse {
            ret_code: 0,
            ret_msg: SmolStr::new("OK"),
            result: BybitCollateralBalanceResult {
                list: vec![BybitCollateralAccountBalance {
                    account_type: SmolStr::new("UNIFIED"),
                    coin: vec![
                        BybitCollateralCoinBalance {
                            coin: SmolStr::new("USDT"),
                            wallet_balance: Decimal::new(100000, 2),
                            available_to_withdraw: Decimal::new(95000, 2),
                            unrealised_pnl: Decimal::new(10000, 2),
                        },
                        BybitCollateralCoinBalance {
                            coin: SmolStr::new("USDC"),
                            wallet_balance: Decimal::new(50000, 2),
                            available_to_withdraw: Decimal::new(48000, 2),
                            unrealised_pnl: Decimal::new(-5000, 2),
                        },
                    ],
                }],
            },
            ret_ext_info: None,
            time: 1690872862481,
        };

        let balances: Vec<FuturesBalance> = response.into();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].asset, "USDT");
        assert_eq!(balances[1].asset, "USDC");
    }

    #[test]
    fn test_bybit_collateral_coin_balance_zero_unrealised_pnl() {
        let coin_balance = BybitCollateralCoinBalance {
            coin: SmolStr::new("USDT"),
            wallet_balance: Decimal::new(10000, 0),
            available_to_withdraw: Decimal::new(10000, 0),
            unrealised_pnl: Decimal::ZERO,
        };

        let futures_balance: FuturesBalance = coin_balance.into();
        assert_eq!(futures_balance.available_balance, Decimal::new(10000, 0));
        assert_eq!(futures_balance.cross_unrealized_pnl, Decimal::ZERO);
    }

    #[test]
    fn test_bybit_collateral_balance_default_fields() {
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
                                "walletBalance": "1.5"
                            }
                        ]
                    }
                ]
            },
            "retExtInfo": {},
            "time": 1690872862481
        }
        "#;

        let response: BybitCollateralBalanceResponse = serde_json::from_str(json).unwrap();
        let balances: Vec<FuturesBalance> = response.into();
        assert_eq!(balances[0].cross_unrealized_pnl, Decimal::ZERO);
        assert_eq!(balances[0].available_withdraw, Decimal::ZERO);
    }

    #[test]
    fn test_bybit_collateral_balance_empty_coins() {
        let response = BybitCollateralBalanceResponse {
            ret_code: 0,
            ret_msg: SmolStr::new("OK"),
            result: BybitCollateralBalanceResult {
                list: vec![BybitCollateralAccountBalance {
                    account_type: SmolStr::new("UNIFIED"),
                    coin: vec![],
                }],
            },
            ret_ext_info: None,
            time: 1690872862481,
        };

        let balances: Vec<FuturesBalance> = response.into();
        assert_eq!(balances.len(), 0);
    }
}
