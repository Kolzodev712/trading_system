//! Exchange-related types (orders, balances, positions, trades)
use crate::types::common::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Order request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    /// Symbol for the open order response
    pub symbol: Symbol,
    /// Side of the book to place the order
    pub side: Side,
    /// Type of the order to place
    pub order_type: OrderType,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size. This is the full, original order amount. Not the remaining amount.
    pub amount: Decimal,
    /// Exchange's timestamp of the order
    pub timestamp: Option<u64>,
    /// Custom params specific to exchanges:
    pub exchange_params: HashMap<String, String>,
}

/// Order
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// Exchange's order ID
    pub order_id: OrderId,
    /// Symbol for the open order response
    pub symbol: Symbol,
    /// Side of the book to place the order
    pub side: Side,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size. This is the full, original order amount. Not the remaining amount.
    pub amount: Decimal,
    /// Internal timestamp at which we received the response of the open order
    pub internal_created_at: u64,
    /// Exchange's timestamp of the order
    pub exchange_created_at: Option<u64>,
    /// Order status (New, Filled, Canceled, etc.)
    #[serde(default = "default_order_status")]
    pub status: OrderStatus,
    /// Filled amount (for partial fills)
    #[serde(default)]
    pub filled_amount: Decimal,
}

fn default_order_status() -> OrderStatus {
    OrderStatus::New
}

/// Account information
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    /// The balances of the account
    pub balances: Vec<Balance>,
    /// The account type
    pub account_type: AccountType,
    /// Unix millisecond timestamp when an update was received from the exchange
    pub update_time: MillisecondTimestamp,
}

/// Balance
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    /// Currency symbol
    pub symbol: Symbol,
    /// The free balance that can be used in new orders
    pub free: Decimal,
    /// The used balance that is bound to open orders
    pub used: Decimal,
    /// The total amount of balance available which should be the sum of [`Balance::free`] and [`Balance::used`]
    pub total: Decimal,
}

/// Position
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Trading pair
    pub pair: Pair,
    /// Position size (positive for long, negative for short)
    pub quantity: Decimal,
    /// Average entry price
    pub avg_price: Decimal,
    /// Current market price
    pub current_price: Decimal,
    /// Unrealized P&L
    pub unrealized_pnl: Decimal,
    /// Last update timestamp
    pub last_update: u64,
}

/// Trade (private trade from execution reports)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    /// Trade ID
    pub trade_id: String,
    /// Order ID that this trade belongs to
    pub order_id: OrderId,
    /// Trading pair
    pub pair: Pair,
    /// Trade price
    pub price: Decimal,
    /// Trade quantity
    pub quantity: Decimal,
    /// Trade side
    pub side: Side,
    /// Trade timestamp
    pub timestamp: u64,
    /// Fee amount
    pub fee: Decimal,
    /// Fee currency
    pub fee_currency: Symbol,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{OrderId, OrderStatus, OrderType, Side};
    use serde_json;

    #[test]
    fn test_order_request_creation() {
        let mut exchange_params = HashMap::new();
        exchange_params.insert("timeInForce".to_string(), "GTC".to_string());

        let order_request = OrderRequest {
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            timestamp: Some(1734819696263),
            exchange_params,
        };

        assert_eq!(order_request.symbol, "BTCUSDT");
        assert_eq!(order_request.side, Side::Buy);
        assert_eq!(order_request.price, Decimal::new(90000, 0));
        assert_eq!(order_request.amount, Decimal::new(10, 2));
    }

    #[test]
    fn test_order_creation() {
        let order = Order {
            order_id: OrderId::new("12345".into()),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        assert_eq!(order.order_id.as_str(), "12345");
        assert_eq!(order.amount, Decimal::new(10, 2));
        assert_eq!(order.filled_amount, Decimal::ZERO);
    }

    #[test]
    fn test_balance_creation() {
        let balance = Balance {
            symbol: "BTC".into(),
            free: Decimal::new(1, 0),
            used: Decimal::new(5, 1),
            total: Decimal::new(15, 1),
        };

        assert_eq!(balance.symbol, "BTC");
        assert_eq!(balance.free, Decimal::new(1, 0));
        assert_eq!(balance.total, balance.free + balance.used);
    }

    #[test]
    fn test_balance_total_calculation() {
        let balance = Balance {
            symbol: "ETH".into(),
            free: Decimal::new(10, 0),
            used: Decimal::new(5, 0),
            total: Decimal::new(15, 0),
        };

        assert_eq!(balance.total, balance.free + balance.used);
    }

    #[test]
    fn test_position_creation() {
        let position = Position {
            pair: Pair::new("BTC", "USDT"),
            quantity: Decimal::new(1, 0),
            avg_price: Decimal::new(90000, 0),
            current_price: Decimal::new(91000, 0),
            unrealized_pnl: Decimal::new(1000, 0),
            last_update: 1734819696263,
        };

        assert_eq!(position.pair.base, "BTC");
        assert_eq!(position.quantity, Decimal::new(1, 0));
    }

    #[test]
    fn test_trade_creation() {
        let trade = Trade {
            trade_id: "12345".to_string(),
            order_id: OrderId::new("67890".into()),
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(90000, 0),
            quantity: Decimal::new(10, 2),
            side: Side::Buy,
            timestamp: 1734819696263,
            fee: Decimal::new(1, 3),
            fee_currency: "BTC".into(),
        };

        assert_eq!(trade.trade_id, "12345");
        assert_eq!(trade.side, Side::Buy);
        assert_eq!(trade.fee_currency, "BTC");
    }

    #[test]
    fn test_account_info_creation() {
        let account_info = AccountInfo {
            balances: vec![
                Balance {
                    symbol: "BTC".into(),
                    free: Decimal::new(1, 0),
                    used: Decimal::ZERO,
                    total: Decimal::new(1, 0),
                },
            ],
            account_type: AccountType::Spot,
            update_time: MillisecondTimestamp(1734819696263),
        };

        assert_eq!(account_info.balances.len(), 1);
        assert_eq!(account_info.account_type, AccountType::Spot);
    }

    #[test]
    fn test_order_serialization() {
        let order = Order {
            order_id: OrderId::new("12345".into()),
            symbol: "BTCUSDT".into(),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let serialized = serde_json::to_string(&order).unwrap();
        assert!(serialized.contains("12345"));
        assert!(serialized.contains("BTCUSDT"));
    }

    #[test]
    fn test_order_deserialization() {
        let json = r#"
        {
            "orderId": "12345",
            "symbol": "BTCUSDT",
            "side": "Buy",
            "price": "90000",
            "amount": "0.10",
            "internalCreatedAt": 1734819700000,
            "exchangeCreatedAt": 1734819696263,
            "status": "New",
            "filledAmount": "0"
        }
        "#;

        let order: Order = serde_json::from_str(json).unwrap();
        assert_eq!(order.order_id.as_str(), "12345");
        assert_eq!(order.status, OrderStatus::New);
    }

    #[test]
    fn test_balance_serialization() {
        let balance = Balance {
            symbol: "BTC".into(),
            free: Decimal::new(1, 0),
            used: Decimal::new(5, 1),
            total: Decimal::new(15, 1),
        };

        let serialized = serde_json::to_string(&balance).unwrap();
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("free"));
    }

    #[test]
    fn test_trade_serialization() {
        let trade = Trade {
            trade_id: "12345".to_string(),
            order_id: OrderId::new("67890".into()),
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(90000, 0),
            quantity: Decimal::new(10, 2),
            side: Side::Buy,
            timestamp: 1734819696263,
            fee: Decimal::new(1, 3),
            fee_currency: "BTC".into(),
        };

        let serialized = serde_json::to_string(&trade).unwrap();
        assert!(serialized.contains("12345"));
        assert!(serialized.contains("BTC"));
    }
}
