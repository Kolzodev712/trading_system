use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce,
};
use core_logic::data::{base_data::OrderStatus, private_data::Order};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
/// See: <https://developers.binance.com/docs/binance-spot-api-docs/web-socket-api/authentication-requests>
pub struct ExecutionReport {
    #[serde(
        rename = "e",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub event_type: SmolStr,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(
        rename = "s",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "S")]
    pub side: BinanceSide,
    #[serde(rename = "o")]
    pub order_type: BinanceOrderType,
    #[serde(rename = "f")]
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "q")]
    pub quantity: Decimal,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "X")]
    pub order_status: BinanceOrderStatus,
    #[serde(rename = "i")]
    pub order_id: u64,
    #[serde(rename = "T")]
    pub transaction_time: u64,
    #[serde(rename = "w")]
    pub is_order_on_book: bool,
}

impl From<ExecutionReport> for Order {
    fn from(report: ExecutionReport) -> Self {
        let status: OrderStatus = report.order_status.into();
        Order {
            order_id: report.order_id.into(),
            symbol: report.symbol.into(),
            side: report.side.into(),
            price: report.price,
            amount: report.quantity,
            internal_created_at: report.event_time,
            exchange_created_at: Some(report.transaction_time),
            status: status.clone(),
            filled_amount: if matches!(status, OrderStatus::Filled | OrderStatus::PartiallyFilled) {
                report.quantity
            } else {
                rust_decimal::Decimal::ZERO
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_logic::data::{base_data::Side, private_data::Order};
    use rust_decimal::Decimal;

    #[test]
    fn test_deserialization_execution_report() {
        let json = r#"
        {
            "e": "executionReport",
            "E": 1628000000000,
            "s": "BTCUSDT",
            "S": "BUY",
            "o": "LIMIT",
            "f": "GTC",
            "q": "1.0",
            "p": "45000.00",
            "X": "NEW",
            "i": 4293153,
            "T": 1628000000000,
            "w": true
        }
        "#;

        let exec_report: ExecutionReport = serde_json::from_str(json).unwrap();

        assert_eq!(exec_report.event_type, "executionReport");
        assert_eq!(exec_report.symbol, "BTCUSDT");
        assert_eq!(exec_report.side, BinanceSide::Buy);
        assert_eq!(exec_report.price, Decimal::new(4500000, 2));
        assert_eq!(exec_report.quantity, Decimal::new(1, 0));
        assert_eq!(exec_report.order_id, 4293153);
        assert_eq!(exec_report.transaction_time, 1628000000000);
    }

    #[test]
    fn test_execution_report_to_order_conversion() {
        let exec_report = ExecutionReport {
            event_type: "executionReport".into(),
            event_time: 1628000000000,
            symbol: "BTCUSDT".into(),
            side: BinanceSide::Buy,
            order_type: BinanceOrderType::Limit,
            time_in_force: BinanceTimeInForce::Gtc,
            quantity: Decimal::new(1, 0),
            price: Decimal::new(45000, 2),
            order_status: BinanceOrderStatus::New,
            order_id: 4293153,
            transaction_time: 1628000000000,
            is_order_on_book: true,
        };

        let order: Order = exec_report.into();

        assert_eq!(order.order_id, 4293153.into());
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Decimal::new(45000, 2));
        assert_eq!(order.amount, Decimal::new(1, 0));
        assert_eq!(order.internal_created_at, 1628000000000);
        assert_eq!(order.exchange_created_at, Some(1628000000000));
    }

    #[test]
    fn test_execution_report_invalid_side() {
        let exec_report = ExecutionReport {
            event_type: "executionReport".into(),
            event_time: 1628000000000,
            symbol: "BTCUSDT".into(),
            side: BinanceSide::Sell,
            order_type: BinanceOrderType::Limit,
            time_in_force: BinanceTimeInForce::Gtc,
            quantity: Decimal::new(1, 0),
            price: Decimal::new(45000, 2),
            order_status: BinanceOrderStatus::New,
            order_id: 4293153,
            transaction_time: 1628000000000,
            is_order_on_book: true,
        };

        let order: Order = exec_report.into();

        assert_eq!(order.side, Side::Sell);
    }

    #[test]
    fn test_execution_report_with_no_exchange_created_at() {
        let exec_report = ExecutionReport {
            event_type: "executionReport".into(),
            event_time: 1628000000000,
            symbol: "BTCUSDT".into(),
            side: BinanceSide::Buy,
            order_type: BinanceOrderType::Limit,
            time_in_force: BinanceTimeInForce::Gtc,
            quantity: Decimal::new(1, 0),
            price: Decimal::new(45000, 2),
            order_status: BinanceOrderStatus::New,
            order_id: 4293153,
            transaction_time: 1628000000000,
            is_order_on_book: true,
        };

        let order: Order = exec_report.into();

        assert_eq!(order.exchange_created_at, Some(1628000000000));
    }
}
