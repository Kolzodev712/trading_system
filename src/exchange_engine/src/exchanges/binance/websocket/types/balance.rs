use core_logic::data::private_data::Balance;
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// See: <https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/account-requests>
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAccountInfo {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    pub status: i16,
    pub result: BinanceAccountData,
}

/// See: <https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/account-requests>
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAccountData {
    pub update_time: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub account_type: SmolStr,
    pub balances: Vec<Balances>,
}
/// See: <https://developers.binance.com/docs/binance-spot-api-docs/web-socket-api/authentication-requests>
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balances {
    #[serde(rename = "a")]
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub asset: SmolStr,
    #[serde(rename = "f")]
    pub free: Decimal,
    #[serde(rename = "l")]
    pub locked: Decimal,
}

impl From<Balances> for Balance {
    fn from(balance_update: Balances) -> Self {
        Balance {
            symbol: balance_update.asset.into(),
            free: balance_update.free,
            used: balance_update.locked,
            total: balance_update.free + balance_update.locked,
        }
    }
}

impl From<BinanceAccountInfo> for Vec<Balance> {
    fn from(binance_account_info: BinanceAccountInfo) -> Self {
        binance_account_info
            .result
            .balances
            .into_iter()
            .map(|balance| Balance {
                symbol: balance.asset.into(),
                free: balance.free,
                used: balance.locked,
                total: balance.free + balance.locked,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_deserialization_binance_account_info() {
        let json = r#"
        {
            "id": "12345",
            "status": 1,
            "result": {
                "updateTime": 1628000000000,
                "accountType": "SPOT",
                "balances": [
                    {
                        "a": "BTC",
                        "f": "10.00",
                        "l": "0.00"
                    },
                    {
                        "a": "USDT",
                        "f": "1000.00",
                        "l": "0.00"
                    }
                ]
            }
        }
        "#;

        let account_info: BinanceAccountInfo = serde_json::from_str(json).unwrap();

        assert_eq!(account_info.id, "12345");
        assert_eq!(account_info.status, 1);
        assert_eq!(account_info.result.update_time, 1628000000000);
        assert_eq!(account_info.result.account_type, "SPOT");
        assert_eq!(account_info.result.balances.len(), 2);
        assert_eq!(account_info.result.balances[0].asset, "BTC");
        assert_eq!(account_info.result.balances[1].asset, "USDT");
    }

    #[test]
    fn test_binance_account_info_to_balance_vec_conversion() {
        let account_info = BinanceAccountInfo {
            id: "12345".into(),
            status: 1,
            result: BinanceAccountData {
                update_time: 1628000000000,
                account_type: "SPOT".into(),
                balances: vec![
                    Balances {
                        asset: "ETH".into(),
                        free: Decimal::new(500, 2),
                        locked: Decimal::new(100, 2),
                    },
                    Balances {
                        asset: "BNB".into(),
                        free: Decimal::new(0, 2),
                        locked: Decimal::new(0, 2),
                    },
                ],
            },
        };

        let balances: Vec<Balance> = account_info.into();

        assert_eq!(balances.len(), 2);

        let eth = &balances[0];
        assert_eq!(eth.symbol, "ETH");
        assert_eq!(eth.free, Decimal::new(500, 2));
        assert_eq!(eth.used, Decimal::new(100, 2));
        assert_eq!(eth.total, Decimal::new(600, 2));

        let bnb = &balances[1];
        assert_eq!(bnb.symbol, "BNB");
        assert_eq!(bnb.free, Decimal::new(0, 2));
        assert_eq!(bnb.used, Decimal::new(0, 2));
        assert_eq!(bnb.total, Decimal::new(0, 2));
    }

    #[test]
    fn test_empty_balances_conversion() {
        let account_info = BinanceAccountInfo {
            id: "empty".into(),
            status: 0,
            result: BinanceAccountData {
                update_time: 0,
                account_type: "SPOT".into(),
                balances: vec![],
            },
        };

        let balances: Vec<Balance> = account_info.into();
        assert!(balances.is_empty());
    }
}
