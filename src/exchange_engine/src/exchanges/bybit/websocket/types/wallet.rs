//! Bybit WebSocket wallet (balance) message type
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/private/wallet>

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::{
    types::{common::Symbol, exchange::Balance},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitWalletMessage {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub topic: SmolStr,
    pub creation_time: u64,
    pub data: Vec<BybitWalletAccountData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitWalletAccountData {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub account_type: SmolStr,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub account_im_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub account_mm_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_equity: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_wallet_balance: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_margin_balance: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_available_balance: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_perp_upl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_initial_margin: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_maintenance_margin: Decimal,
    pub coin: Vec<BybitWalletCoinBalance>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitWalletCoinBalance {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub coin: SmolStr,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub equity: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub usd_value: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub wallet_balance: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub locked: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub spot_hedging_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub borrow_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub accrued_interest: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_order_im: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_position_im: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub total_position_mm: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub unrealised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub cum_realised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub bonus: Decimal,
    pub collateral_switch: bool,
    pub margin_collateral: bool,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub spot_borrow: Decimal,
}

impl From<BybitWalletCoinBalance> for Balance {
    fn from(coin: BybitWalletCoinBalance) -> Self {
        let free = coin.wallet_balance - coin.locked;
        Balance {
            symbol: Symbol::from(&coin.coin),
            free,
            used: coin.locked,
            total: coin.wallet_balance,
        }
    }
}

impl BybitWalletMessage {
    pub fn to_balances(self) -> Vec<Balance> {
        self.data
            .into_iter()
            .flat_map(|account| account.coin)
            .map(Balance::from)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bybit_wallet_message_deserialization() {
        let json = r#"
        {
            "id": "wallet-msg-123",
            "topic": "wallet",
            "creationTime": 1697682317044,
            "data": [
                {
                    "accountType": "UNIFIED",
                    "accountIMRate": "0.1",
                    "accountMMRate": "0.05",
                    "totalEquity": "10000.0",
                    "totalWalletBalance": "9500.0",
                    "totalMarginBalance": "10000.0",
                    "totalAvailableBalance": "9000.0",
                    "totalPerpUPL": "500.0",
                    "totalInitialMargin": "500.0",
                    "totalMaintenanceMargin": "250.0",
                    "coin": [
                        {
                            "coin": "BTC",
                            "equity": "0.5",
                            "usdValue": "20000.0",
                            "walletBalance": "0.5",
                            "locked": "0.1",
                            "spotHedgingQty": "0",
                            "borrowAmount": "0",
                            "accruedInterest": "0",
                            "totalOrderIM": "0",
                            "totalPositionIM": "0",
                            "totalPositionMM": "0",
                            "unrealisedPnl": "0",
                            "cumRealisedPnl": "0",
                            "bonus": "0",
                            "collateralSwitch": true,
                            "marginCollateral": true,
                            "spotBorrow": "0"
                        },
                        {
                            "coin": "USDT",
                            "equity": "1000.0",
                            "usdValue": "1000.0",
                            "walletBalance": "1000.0",
                            "locked": "50.0",
                            "spotHedgingQty": "0",
                            "borrowAmount": "0",
                            "accruedInterest": "0",
                            "totalOrderIM": "0",
                            "totalPositionIM": "0",
                            "totalPositionMM": "0",
                            "unrealisedPnl": "0",
                            "cumRealisedPnl": "0",
                            "bonus": "0",
                            "collateralSwitch": true,
                            "marginCollateral": true,
                            "spotBorrow": "0"
                        }
                    ]
                }
            ]
        }
        "#;

        let message: BybitWalletMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.topic, "wallet");
        assert_eq!(message.data.len(), 1);
        assert_eq!(message.data[0].account_type, "UNIFIED");
        assert_eq!(message.data[0].coin.len(), 2);
        assert_eq!(message.data[0].coin[0].coin, "BTC");
        assert_eq!(message.data[0].coin[1].coin, "USDT");
    }

    #[test]
    fn test_bybit_wallet_coin_balance_to_balance() {
        let coin_balance = BybitWalletCoinBalance {
            coin: SmolStr::new("USDT"),
            equity: Decimal::new(100000, 2),
            usd_value: Decimal::new(100000, 2),
            wallet_balance: Decimal::new(100000, 2),
            locked: Decimal::new(5000, 2),
            spot_hedging_qty: Decimal::ZERO,
            borrow_amount: Decimal::ZERO,
            accrued_interest: Decimal::ZERO,
            total_order_im: Decimal::ZERO,
            total_position_im: Decimal::ZERO,
            total_position_mm: Decimal::ZERO,
            unrealised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            bonus: Decimal::ZERO,
            collateral_switch: true,
            margin_collateral: true,
            spot_borrow: Decimal::ZERO,
        };

        let balance: Balance = coin_balance.into();
        assert_eq!(balance.symbol, Symbol::from("USDT"));
        assert_eq!(balance.free, Decimal::new(95000, 2));
        assert_eq!(balance.used, Decimal::new(5000, 2));
        assert_eq!(balance.total, Decimal::new(100000, 2));
    }

    #[test]
    fn test_bybit_wallet_coin_balance_zero_locked() {
        let coin_balance = BybitWalletCoinBalance {
            coin: SmolStr::new("BTC"),
            equity: Decimal::new(100000000, 8),
            usd_value: Decimal::new(200000000, 8),
            wallet_balance: Decimal::new(100000000, 8),
            locked: Decimal::ZERO,
            spot_hedging_qty: Decimal::ZERO,
            borrow_amount: Decimal::ZERO,
            accrued_interest: Decimal::ZERO,
            total_order_im: Decimal::ZERO,
            total_position_im: Decimal::ZERO,
            total_position_mm: Decimal::ZERO,
            unrealised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            bonus: Decimal::ZERO,
            collateral_switch: true,
            margin_collateral: true,
            spot_borrow: Decimal::ZERO,
        };

        let balance: Balance = coin_balance.into();
        assert_eq!(balance.free, Decimal::new(100000000, 8));
        assert_eq!(balance.used, Decimal::ZERO);
        assert_eq!(balance.total, Decimal::new(100000000, 8));
    }

    #[test]
    fn test_bybit_wallet_message_to_balances() {
        let message = BybitWalletMessage {
            id: SmolStr::new("wallet-123"),
            topic: SmolStr::new("wallet"),
            creation_time: 1697682317044,
            data: vec![BybitWalletAccountData {
                account_type: SmolStr::new("UNIFIED"),
                account_im_rate: Decimal::ZERO,
                account_mm_rate: Decimal::ZERO,
                total_equity: Decimal::ZERO,
                total_wallet_balance: Decimal::ZERO,
                total_margin_balance: Decimal::ZERO,
                total_available_balance: Decimal::ZERO,
                total_perp_upl: Decimal::ZERO,
                total_initial_margin: Decimal::ZERO,
                total_maintenance_margin: Decimal::ZERO,
                coin: vec![
                    BybitWalletCoinBalance {
                        coin: SmolStr::new("BTC"),
                        equity: Decimal::new(100000000, 8),
                        usd_value: Decimal::new(200000000, 8),
                        wallet_balance: Decimal::new(100000000, 8),
                        locked: Decimal::new(10000000, 8),
                        spot_hedging_qty: Decimal::ZERO,
                        borrow_amount: Decimal::ZERO,
                        accrued_interest: Decimal::ZERO,
                        total_order_im: Decimal::ZERO,
                        total_position_im: Decimal::ZERO,
                        total_position_mm: Decimal::ZERO,
                        unrealised_pnl: Decimal::ZERO,
                        cum_realised_pnl: Decimal::ZERO,
                        bonus: Decimal::ZERO,
                        collateral_switch: true,
                        margin_collateral: true,
                        spot_borrow: Decimal::ZERO,
                    },
                    BybitWalletCoinBalance {
                        coin: SmolStr::new("USDT"),
                        equity: Decimal::new(100000, 2),
                        usd_value: Decimal::new(100000, 2),
                        wallet_balance: Decimal::new(100000, 2),
                        locked: Decimal::new(5000, 2),
                        spot_hedging_qty: Decimal::ZERO,
                        borrow_amount: Decimal::ZERO,
                        accrued_interest: Decimal::ZERO,
                        total_order_im: Decimal::ZERO,
                        total_position_im: Decimal::ZERO,
                        total_position_mm: Decimal::ZERO,
                        unrealised_pnl: Decimal::ZERO,
                        cum_realised_pnl: Decimal::ZERO,
                        bonus: Decimal::ZERO,
                        collateral_switch: true,
                        margin_collateral: true,
                        spot_borrow: Decimal::ZERO,
                    },
                ],
            }],
        };

        let balances = message.to_balances();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].symbol, Symbol::from("BTC"));
        assert_eq!(balances[0].total, Decimal::new(100000000, 8));
        assert_eq!(balances[1].symbol, Symbol::from("USDT"));
        assert_eq!(balances[1].total, Decimal::new(100000, 2));
    }

    #[test]
    fn test_bybit_wallet_message_empty_coins() {
        let message = BybitWalletMessage {
            id: SmolStr::new("wallet-123"),
            topic: SmolStr::new("wallet"),
            creation_time: 1697682317044,
            data: vec![BybitWalletAccountData {
                account_type: SmolStr::new("UNIFIED"),
                account_im_rate: Decimal::ZERO,
                account_mm_rate: Decimal::ZERO,
                total_equity: Decimal::ZERO,
                total_wallet_balance: Decimal::ZERO,
                total_margin_balance: Decimal::ZERO,
                total_available_balance: Decimal::ZERO,
                total_perp_upl: Decimal::ZERO,
                total_initial_margin: Decimal::ZERO,
                total_maintenance_margin: Decimal::ZERO,
                coin: vec![],
            }],
        };

        let balances = message.to_balances();
        assert!(balances.is_empty());
    }
}
