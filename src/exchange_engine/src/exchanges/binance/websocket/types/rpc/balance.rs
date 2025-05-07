use super::super::common::SessionLogonParams;
use core_logic::data::private_data::Balance;
use core_logic::utils::{
    deserialize_option_smol_str, deserialize_smol_str, serialize_option_smol_str,
    serialize_smol_str,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// See: <https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/account-requests>
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAccountStatusRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub method: SmolStr,
    pub params: BinanceAccountStatusParams,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAccountStatusParams {
    #[serde(flatten)]
    pub session_logon_params: SessionLogonParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recv_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omit_zero_balances: Option<bool>,
}

/// See: <https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/account-requests>
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAccountStatusResponse {
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
    pub update_time: Option<i64>,
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    pub account_type: Option<SmolStr>,
    pub balances: Option<Vec<Balances>>,
}
/// See: <https://developers.binance.com/docs/binance-spot-api-docs/web-socket-api/authentication-requests>
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balances {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub asset: SmolStr,
    pub free: Decimal,
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

impl From<BinanceAccountStatusResponse> for Vec<Balance> {
    fn from(binance_account_info: BinanceAccountStatusResponse) -> Self {
        binance_account_info
            .result
            .balances
            .unwrap_or_default()
            .into_iter()
            .map(Balance::from)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json::json;

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
                        "asset": "BTC",
                        "free": "10.00",
                        "locked": "0.00"
                    },
                    {
                        "asset": "USDT",
                        "free": "1000.00",
                        "locked": "0.00"
                    }
                ]
            }
        }
        "#;

        let account_info: BinanceAccountStatusResponse = serde_json::from_str(json).unwrap();

        assert_eq!(account_info.id, "12345");
        assert_eq!(account_info.status, 1);
        assert_eq!(account_info.result.update_time, Some(1628000000000));
        assert_eq!(account_info.result.account_type.as_deref(), Some("SPOT"));
        let balances = account_info.result.balances.as_ref().unwrap();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].asset, "BTC");
        assert_eq!(balances[1].asset, "USDT");
    }

    #[test]
    fn test_binance_account_info_to_balance_vec_conversion() {
        let account_info = BinanceAccountStatusResponse {
            id: "12345".into(),
            status: 1,
            result: BinanceAccountData {
                update_time: Some(1628000000000),
                account_type: Some("SPOT".into()),
                balances: Some(vec![
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
                ]),
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
        let account_info = BinanceAccountStatusResponse {
            id: "empty".into(),
            status: 0,
            result: BinanceAccountData {
                update_time: None,
                account_type: None,
                balances: Some(vec![]),
            },
        };

        let balances: Vec<Balance> = account_info.into();
        assert!(balances.is_empty());
    }

    #[test]
    fn test_missing_balances_field_conversion() {
        let account_info = BinanceAccountStatusResponse {
            id: "missing".into(),
            status: 0,
            result: BinanceAccountData {
                update_time: None,
                account_type: None,
                balances: None,
            },
        };

        let balances: Vec<Balance> = account_info.into();
        assert!(balances.is_empty());
    }

    #[test]
    fn test_account_status_params_serialization_minimal() {
        let params = BinanceAccountStatusParams {
            session_logon_params: SessionLogonParams {
                api_key: "my_key".into(),
                timestamp: 1234567890,
                signature: "sig".into(),
            },
            recv_window: None,
            omit_zero_balances: None,
        };

        let serialized = serde_json::to_value(&params).unwrap();

        assert_eq!(serialized["apiKey"], "my_key");
        assert_eq!(serialized["timestamp"], 1234567890);
        assert_eq!(serialized["signature"], "sig");
        assert!(serialized.get("recvWindow").is_none());
        assert!(serialized.get("omitZeroBalances").is_none());
    }

    #[test]
    fn test_account_status_params_serialization_with_options() {
        let params = BinanceAccountStatusParams {
            session_logon_params: SessionLogonParams {
                api_key: "another_key".into(),
                timestamp: 98765432,
                signature: "signature123".into(),
            },
            recv_window: Some(5000),
            omit_zero_balances: Some(true),
        };

        let json = serde_json::to_value(&params).unwrap();

        assert_eq!(json["apiKey"], "another_key");
        assert_eq!(json["timestamp"], 98765432);
        assert_eq!(json["signature"], "signature123");
        assert_eq!(json["recvWindow"], 5000);
        assert_eq!(json["omitZeroBalances"], true);
    }

    #[test]
    fn test_account_status_params_full_json_string() {
        let params = BinanceAccountStatusParams {
            session_logon_params: SessionLogonParams {
                api_key: "abc123".into(),
                timestamp: 1111111111,
                signature: "xyz456".into(),
            },
            recv_window: Some(60000),
            omit_zero_balances: Some(false),
        };

        let expected = json!({
            "apiKey": "abc123",
            "timestamp": 1111111111,
            "signature": "xyz456",
            "recvWindow": 60000,
            "omitZeroBalances": false
        });

        let actual = serde_json::to_value(&params).unwrap();

        assert_eq!(actual, expected);
    }
}
