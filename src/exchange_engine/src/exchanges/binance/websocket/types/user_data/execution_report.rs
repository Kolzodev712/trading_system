use crate::exchanges::binance::common_types::{
    BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce,
};
use trading_core::types::{common::OrderStatus, exchange::Order};
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

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
    #[serde(rename = "q", with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(rename = "p", with = "rust_decimal::serde::str")]
    pub price: Decimal,
    /// Cumulative filled quantity (z field from Binance API)
    /// This represents the total amount filled so far for this order
    #[serde(rename = "z", default, with = "rust_decimal::serde::str_option")]
    pub cumulative_filled_quantity: Option<Decimal>,
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

        // Use cumulative filled quantity (z field) for filled/partially filled orders
        // This is the authoritative source for how much has been filled so far
        let filled_amount = match status {
            OrderStatus::Filled | OrderStatus::PartiallyFilled => {
                // Always prefer cumulative_filled_quantity (z field) when available
                // For Filled orders, if cumulative is missing, use quantity as fallback
                // For PartiallyFilled orders, cumulative should always be present from Binance
                report.cumulative_filled_quantity.unwrap_or_else(|| {
                    if status == OrderStatus::Filled {
                        // Fully filled but no cumulative - use original quantity as fallback
                        report.quantity
                    } else {
                        // PartiallyFilled without cumulative quantity - this indicates a data issue
                        // Default to zero to avoid incorrect position tracking
                        rust_decimal::Decimal::ZERO
                    }
                })
            }
            _ => rust_decimal::Decimal::ZERO,
        };

        Order {
            order_id: report.order_id.into(),
            client_order_id: None, // ExecutionReport doesn't include client_order_id
            symbol: report.symbol.into(),
            side: report.side.into(),
            price: report.price,
            amount: report.quantity,
            internal_created_at: report.event_time,
            exchange_created_at: Some(report.transaction_time),
            status: status.clone(),
            filled_amount,
            instrument_type: trading_core::types::common::InstrumentType::Spot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::{common::Side, exchange::Order};

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
            cumulative_filled_quantity: None,
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
            cumulative_filled_quantity: None,
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
            cumulative_filled_quantity: None,
            order_status: BinanceOrderStatus::New,
            order_id: 4293153,
            transaction_time: 1628000000000,
            is_order_on_book: true,
        };

        let order: Order = exec_report.into();

        assert_eq!(order.exchange_created_at, Some(1628000000000));
    }
}
