use core_logic::data::private_data::Balance;
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// See: <https://developers.binance.com/docs/binance-spot-api-docs/user-data-stream>

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundAccountPosition {
    #[serde(rename = "u")]
    pub last_update: u64,
    #[serde(rename = "B")]
    pub updated_balances: Vec<Balances>,
}

/// See: <https://developers.binance.com/docs/binance-spot-api-docs/user-data-stream>
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

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceUpdate {
    #[serde(rename = "a")]
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub asset: SmolStr,
    #[serde(rename = "d")]
    pub delta: Decimal,
    #[serde(rename = "T", default)]
    pub clear_time: Option<u64>,
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

impl From<OutboundAccountPosition> for Vec<Balance> {
    fn from(binance_account_info: OutboundAccountPosition) -> Self {
        binance_account_info
            .updated_balances
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
    use core_logic::data::private_data::Balance;
    use rust_decimal::Decimal;

    #[test]
    fn test_deserialization_outbound_account_position() {
        let json = r#"
        {
            "u": 1628000000000,
            "B": [
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
        "#;

        let outbound_position: OutboundAccountPosition = serde_json::from_str(json).unwrap();

        assert_eq!(outbound_position.last_update, 1628000000000);
        assert_eq!(outbound_position.updated_balances.len(), 2);
        assert_eq!(outbound_position.updated_balances[0].asset, "BTC");
        assert_eq!(outbound_position.updated_balances[1].asset, "USDT");
    }

    #[test]
    fn test_deserialization_balance_update() {
        let json = r#"
        {
            "e": "balanceUpdate",
            "E": 1573200697110,
            "a": "BTC",
            "d": "100.000000",
            "T": 1573200697068
        }
        "#;

        let balance_update: BalanceUpdate = serde_json::from_str(json).unwrap();

        assert_eq!(balance_update.asset, "BTC");
        assert_eq!(balance_update.delta, Decimal::new(10_000_000_000, 8));
        assert_eq!(balance_update.clear_time, Some(1573200697068));
    }

    // Test case for the conversion of OutboundAccountPosition to Vec<Balance>
    #[test]
    fn test_outbound_account_position_to_balance_vec_conversion() {
        let outbound_position = OutboundAccountPosition {
            last_update: 1628000000000,
            updated_balances: vec![
                Balances {
                    asset: "BTC".into(),
                    free: Decimal::new(10, 2),
                    locked: Decimal::new(0, 0),
                },
                Balances {
                    asset: "USDT".into(),
                    free: Decimal::new(1000, 0),
                    locked: Decimal::new(0, 0),
                },
            ],
        };

        let balances: Vec<Balance> = outbound_position.into();

        assert_eq!(balances.len(), 2);

        let btc_balance = &balances[0];
        assert_eq!(btc_balance.symbol, "BTC");
        assert_eq!(btc_balance.free, Decimal::new(10, 2));
        assert_eq!(btc_balance.used, Decimal::new(0, 0));
        assert_eq!(btc_balance.total, Decimal::new(10, 2));

        let usdt_balance = &balances[1];
        assert_eq!(usdt_balance.symbol, "USDT");
        assert_eq!(usdt_balance.free, Decimal::new(1000, 0));
        assert_eq!(usdt_balance.used, Decimal::new(0, 0));
        assert_eq!(usdt_balance.total, Decimal::new(1000, 0));
    }

    // Test case with an empty OutboundAccountPosition (no balances)
    #[test]
    fn test_outbound_account_position_empty_balances() {
        let outbound_position = OutboundAccountPosition {
            last_update: 1628000000000,
            updated_balances: vec![],
        };

        let balances: Vec<Balance> = outbound_position.into();

        assert_eq!(balances.len(), 0);
    }

    // Test case with balances having some values (testing non-zero balances)
    #[test]
    fn test_outbound_account_position_with_non_zero_balances() {
        let outbound_position = OutboundAccountPosition {
            last_update: 1628000000000,
            updated_balances: vec![
                Balances {
                    asset: "ETH".into(),
                    free: Decimal::new(5, 2),
                    locked: Decimal::new(1, 2),
                },
                Balances {
                    asset: "BNB".into(),
                    free: Decimal::new(0, 2),
                    locked: Decimal::new(0, 2),
                },
            ],
        };

        let balances: Vec<Balance> = outbound_position.into();

        let eth_balance = &balances[0];
        assert_eq!(eth_balance.symbol, "ETH");
        assert_eq!(eth_balance.free, Decimal::new(5, 2));
        assert_eq!(eth_balance.used, Decimal::new(1, 2));
        assert_eq!(eth_balance.total, Decimal::new(6, 2));

        let bnb_balance = &balances[1];
        assert_eq!(bnb_balance.symbol, "BNB");
        assert_eq!(bnb_balance.free, Decimal::new(0, 2));
        assert_eq!(bnb_balance.used, Decimal::new(0, 2));
        assert_eq!(bnb_balance.total, Decimal::new(0, 2));
    }
}
