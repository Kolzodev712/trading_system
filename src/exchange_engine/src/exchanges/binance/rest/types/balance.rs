use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trading_core::types::{
    common::{AccountType, MillisecondTimestamp, Symbol},
    exchange::{AccountInfo, Balance},
};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceBalanceRequest {
    /// optional param to remove 0 balances
    pub omit_zero_balances: Option<bool>,
    /// timestamp in milliseconds
    pub timestamp: MillisecondTimestamp,
}

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
    #[serde(with = "rust_decimal::serde::str")]
    pub free: Decimal,
    /// The locked balance that is bound to open orders
    #[serde(with = "rust_decimal::serde::str")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json;

    // Test case for deserializing a `BinanceAccountInfo` struct from JSON
    #[test]
    fn test_binance_account_info_deserialization() {
        let json = r#"
        {
            "balances": [
                {
                    "asset": "BTC",
                    "free": "4723846.89208129",
                    "locked": "0.00000000"
                },
                {
                    "asset": "LTC",
                    "free": "4763368.68006011",
                    "locked": "0.00000000"
                }
            ],
            "accountType": "SPOT",
            "updateTime": 123456789
        }
        "#;

        let binance_account_info: BinanceAccountInfo = serde_json::from_str(json).unwrap();

        assert_eq!(binance_account_info.balances.len(), 2);
        assert_eq!(binance_account_info.balances[0].asset, "BTC");
        assert_eq!(
            binance_account_info.balances[0].free,
            Decimal::new(472384689208129, 8)
        );
        assert_eq!(binance_account_info.account_type, BinanceAccountType::Spot);
        assert_eq!(
            binance_account_info.update_time,
            MillisecondTimestamp(123456789)
        );
    }

    // Test case for serializing a `BinanceAccountInfo` struct to JSON
    #[test]
    fn test_binance_account_info_serialization() {
        let account_info = BinanceAccountInfo {
            balances: vec![
                BinanceBalance {
                    asset: "BTC".to_string().into(),
                    free: Decimal::new(472384689208129, 8),
                    locked: Decimal::new(0, 8),
                },
                BinanceBalance {
                    asset: "LTC".to_string().into(),
                    free: Decimal::new(476336868006011, 8),
                    locked: Decimal::new(0, 8),
                },
            ],
            account_type: BinanceAccountType::Spot,
            update_time: MillisecondTimestamp(123456789),
        };

        let json = serde_json::to_string(&account_info).unwrap();
        assert!(json.contains("\"accountType\":\"SPOT\""));
        assert!(json.contains("\"balances\""));
        assert!(json.contains("\"free\":\"4723846.89208129\""));
    }

    // Test case for converting a `BinanceAccountType` to `AccountType`
    #[test]
    fn test_binance_account_type_conversion() {
        let binance_account_type = BinanceAccountType::Spot;
        let account_type: AccountType = binance_account_type.into();
        assert_eq!(account_type, AccountType::Spot);

        let binance_account_type = BinanceAccountType::Futures;
        let account_type: AccountType = binance_account_type.into();
        assert_eq!(account_type, AccountType::Futures);

        let binance_account_type = BinanceAccountType::Unknown;
        let account_type: AccountType = binance_account_type.into();
        assert_eq!(account_type, AccountType::Unknown);
    }

    // Test case for converting a `BinanceBalance` to `Balance`
    #[test]
    fn test_binance_balance_conversion() {
        let binance_balance = BinanceBalance {
            asset: "BTC".to_string().into(),
            free: Decimal::new(472384689208129, 8),
            locked: Decimal::new(0, 8),
        };

        let balance: Balance = binance_balance.into();
        assert_eq!(balance.symbol, "BTC");
        assert_eq!(balance.free, Decimal::new(472384689208129, 8));
        assert_eq!(balance.used, Decimal::new(0, 8));
        assert_eq!(balance.total, Decimal::new(472384689208129, 8));
    }

    // Test case for converting a `BinanceAccountInfo` to `AccountInfo`
    #[test]
    fn test_binance_account_info_conversion() {
        let binance_account_info = BinanceAccountInfo {
            balances: vec![
                BinanceBalance {
                    asset: "BTC".to_string().into(),
                    free: Decimal::new(472384689208129, 8),
                    locked: Decimal::new(0, 8),
                },
                BinanceBalance {
                    asset: "LTC".to_string().into(),
                    free: Decimal::new(476336868006011, 8),
                    locked: Decimal::new(0, 8),
                },
            ],
            account_type: BinanceAccountType::Spot,
            update_time: MillisecondTimestamp(123456789),
        };

        let account_info: AccountInfo = binance_account_info.into();

        assert_eq!(account_info.balances.len(), 2);
        assert_eq!(account_info.balances[0].symbol, "BTC");
        assert_eq!(
            account_info.balances[0].free,
            Decimal::new(472384689208129, 8)
        );
        assert_eq!(account_info.account_type, AccountType::Spot);
        assert_eq!(account_info.update_time, MillisecondTimestamp(123456789));
    }

    // Test case for serialization and deserialization of `BinanceBalanceRequest`
    #[test]
    fn test_binance_balance_request_serialization() {
        let binance_balance_request = BinanceBalanceRequest {
            omit_zero_balances: Some(true),
            timestamp: MillisecondTimestamp(1627849182375),
        };

        let json = serde_json::to_string(&binance_balance_request).unwrap();
        assert!(json.contains("\"omitZeroBalances\":true"));
        assert!(json.contains("\"timestamp\":1627849182375"));

        let deserialized: BinanceBalanceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.omit_zero_balances, Some(true));
        assert_eq!(deserialized.timestamp, MillisecondTimestamp(1627849182375));
    }

    // Test case for handling empty balances
    #[test]
    fn test_empty_balances() {
        let binance_account_info = BinanceAccountInfo {
            balances: vec![],
            account_type: BinanceAccountType::Spot,
            update_time: MillisecondTimestamp(123456789),
        };

        let account_info: AccountInfo = binance_account_info.into();
        assert_eq!(account_info.balances.len(), 0);
    }

    // Test case for handling omitted balances (zero balance)
    #[test]
    fn test_omit_zero_balances() {
        let binance_balance_request = BinanceBalanceRequest {
            omit_zero_balances: Some(true),
            timestamp: MillisecondTimestamp(1627849182375),
        };

        let json = serde_json::to_string(&binance_balance_request).unwrap();
        assert!(json.contains("\"omitZeroBalances\":true"));
    }
}
