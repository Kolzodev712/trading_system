//! Exchange-related types (orders, balances, positions, trades)
use crate::types::common::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;

/// Order request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    /// Trading instrument (pair + instrument type)
    pub instrument: Instrument,
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
    pub exchange_params: HashMap<SmolStr, SmolStr>,
}

/// Order
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// Exchange's order ID
    pub order_id: OrderId,
    /// Client order ID (if provided by exchange, used for cancellation fallback)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<SmolStr>,
    /// Trading instrument (pair + instrument type)
    pub instrument: Instrument,
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

/// Amended Order - represents an order that was amended (canceled and replaced)
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmendedOrder {
    /// Original order ID (the one that was canceled)
    pub original_order_id: OrderId,
    /// New order ID (the replacement order)
    pub new_order_id: OrderId,
    /// Trading instrument (pair + instrument type)
    pub instrument: Instrument,
    /// Side of the book
    pub side: Side,
    /// Original price (before amendment)
    pub original_price: Decimal,
    /// New price (after amendment)
    pub new_price: Decimal,
    /// Original amount (before amendment)
    pub original_amount: Decimal,
    /// New amount (after amendment)
    pub new_amount: Decimal,
    /// Timestamp when amendment occurred
    pub amended_at: u64,
    /// Status of the new order
    pub new_order_status: OrderStatus,
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

/// Position (unified for both spot and futures)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Trading instrument (pair + instrument type)
    pub instrument: Instrument,
    /// Position size (positive for long, negative for short)
    pub quantity: Decimal,
    /// Average entry price
    pub avg_price: Decimal,
    /// Current market price (spot) or last trade price (futures)
    pub current_price: Decimal,
    /// Unrealized P&L
    pub unrealized_pnl: Decimal,
    /// Last update timestamp
    pub last_update: u64,

    // Futures-specific fields (optional, None for spot positions)
    /// Mark price (futures only, for accurate P&L calculation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark_price: Option<Decimal>,
    /// Index price (futures only, for funding rate calculation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_price: Option<Decimal>,
    /// Realized P&L (futures only, cumulative)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realized_pnl: Option<Decimal>,
    /// Initial margin (futures only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_margin: Option<Decimal>,
    /// Maintenance margin (futures only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintenance_margin: Option<Decimal>,
    /// Leverage used (futures only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leverage: Option<u32>,
    /// Position mode (futures only: one-way or hedge)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_mode: Option<crate::types::futures::PositionMode>,
    /// Margin type (futures only: isolated or cross)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_type: Option<crate::types::futures::MarginType>,
    /// Liquidation price (futures only, estimated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidation_price: Option<Decimal>,
}

impl Position {
    /// Check if this is a futures position
    pub fn is_futures(&self) -> bool {
        matches!(
            self.instrument.instrument_type,
            crate::types::common::InstrumentType::Futures
        )
    }

    /// Check if this is a spot position
    pub fn is_spot(&self) -> bool {
        matches!(
            self.instrument.instrument_type,
            crate::types::common::InstrumentType::Spot
        )
    }

    /// Get the pair
    pub fn pair(&self) -> &Pair {
        &self.instrument.pair
    }

    /// Get mark price (returns current_price for spot, mark_price for futures)
    pub fn effective_mark_price(&self) -> Decimal {
        self.mark_price.unwrap_or(self.current_price)
    }
}

/// Trade (private trade from execution reports)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    /// Trade ID
    pub trade_id: SmolStr,
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

/// Result of a single operation within a bulk request
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOperationResult<T> {
    pub success: bool,
    /// Result data if successful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Index in the original request (0-based)
    pub index: usize,
}

/// Bulk open orders request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOrderRequest {
    /// List of order requests to place
    pub requests: Vec<OrderRequest>,
}

/// Bulk open orders response
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOrderResponse {
    /// Results for each order request
    pub results: Vec<BulkOperationResult<Order>>,
    /// Number of successful orders
    pub success_count: usize,
    /// Number of failed orders
    pub failure_count: usize,
}

/// Bulk cancel orders request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkCancelRequest {
    /// List of order IDs to cancel
    pub order_ids: Vec<OrderId>,
    /// Optional pair filter (if provided, only cancel orders for this pair)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair: Option<Pair>,
}

/// Bulk cancel orders response
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkCancelResponse {
    /// Results for each cancellation request
    pub results: Vec<BulkOperationResult<OrderId>>,
    /// List of successfully cancelled order IDs
    pub cancelled_ids: Vec<OrderId>,
    /// List of order IDs that failed to cancel (with their errors)
    pub failed_ids: Vec<(OrderId, String)>,
}

/// Single amendment item in a bulk amend request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkAmendItem {
    /// Order ID to amend
    pub order_id: OrderId,
    /// New order request (replaces the existing order)
    pub new_request: OrderRequest,
}

/// Bulk amend orders request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkAmendRequest {
    /// List of amendments to perform
    pub amendments: Vec<BulkAmendItem>,
}

/// Bulk amend orders response
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkAmendResponse {
    /// Results for each amendment request
    pub results: Vec<BulkOperationResult<Order>>,
    /// List of successfully amended orders
    pub amended_orders: Vec<Order>,
    /// List of failed amendments (order_id, error)
    pub failed_amendments: Vec<(OrderId, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{OrderId, OrderStatus, OrderType, Side};
    use serde_json;

    #[test]
    fn test_order_request_creation() {
        let mut exchange_params = HashMap::new();
        exchange_params.insert(SmolStr::new("timeInForce"), SmolStr::new("GTC"));

        let order_request = OrderRequest {
            instrument: Instrument::new(
                crate::types::common::InstrumentType::Spot,
                Pair::new("BTC", "USDT"),
            ),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            timestamp: Some(1734819696263),
            exchange_params,
        };

        assert_eq!(order_request.instrument.symbol(), "BTCUSDT");
        assert_eq!(order_request.side, Side::Buy);
        assert_eq!(order_request.price, Decimal::new(90000, 0));
        assert_eq!(order_request.amount, Decimal::new(10, 2));
    }

    #[test]
    fn test_order_creation() {
        let order = Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: Instrument::new(
                crate::types::common::InstrumentType::Spot,
                Pair::new("BTC", "USDT"),
            ),
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
            instrument: Instrument::new(
                crate::types::common::InstrumentType::Spot,
                Pair::new("BTC", "USDT"),
            ),
            quantity: Decimal::new(1, 0),
            avg_price: Decimal::new(90000, 0),
            current_price: Decimal::new(91000, 0),
            unrealized_pnl: Decimal::new(1000, 0),
            last_update: 1734819696263,
            mark_price: None,
            index_price: None,
            realized_pnl: None,
            initial_margin: None,
            maintenance_margin: None,
            leverage: None,
            position_mode: None,
            margin_type: None,
            liquidation_price: None,
        };

        assert_eq!(position.pair().base, "BTC");
        assert_eq!(position.quantity, Decimal::new(1, 0));
    }

    #[test]
    fn test_trade_creation() {
        let trade = Trade {
            trade_id: SmolStr::new("12345"),
            order_id: OrderId::new("67890".into()),
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(90000, 0),
            quantity: Decimal::new(10, 2),
            side: Side::Buy,
            timestamp: 1734819696263,
            fee: Decimal::new(1, 3),
            fee_currency: "BTC".into(),
        };

        assert_eq!(trade.trade_id, SmolStr::new("12345"));
        assert_eq!(trade.side, Side::Buy);
        assert_eq!(trade.fee_currency, "BTC");
    }

    #[test]
    fn test_account_info_creation() {
        let account_info = AccountInfo {
            balances: vec![Balance {
                symbol: "BTC".into(),
                free: Decimal::new(1, 0),
                used: Decimal::ZERO,
                total: Decimal::new(1, 0),
            }],
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
            client_order_id: None,
            instrument: Instrument::new(
                crate::types::common::InstrumentType::Spot,
                Pair::new("BTC", "USDT"),
            ),
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
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
    }

    #[test]
    fn test_order_deserialization() {
        // Create an order and serialize it to get the correct format
        let order = Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let json = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.order_id.as_str(), "12345");
        assert_eq!(deserialized.status, OrderStatus::New);
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
            trade_id: SmolStr::new("12345"),
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

    #[test]
    fn test_bulk_operation_result_creation() {
        let success_result = BulkOperationResult {
            success: true,
            data: Some(OrderId::new("12345".into())),
            error: None,
            index: 0,
        };

        let failure_result: BulkOperationResult<OrderId> = BulkOperationResult {
            success: false,
            data: None,
            error: Some("Order not found".to_string()),
            index: 1,
        };

        assert!(success_result.success);
        assert_eq!(success_result.data, Some(OrderId::new("12345".into())));
        assert!(!failure_result.success);
        assert_eq!(failure_result.error, Some("Order not found".to_string()));
    }

    #[test]
    fn test_bulk_order_request_creation() {
        let requests = vec![
            OrderRequest {
                instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
                side: Side::Buy,
                order_type: OrderType::Limit,
                price: Decimal::new(90000, 0),
                amount: Decimal::new(1, 0),
                timestamp: None,
                exchange_params: HashMap::new(),
            },
            OrderRequest {
                instrument: Instrument::new(InstrumentType::Spot, Pair::new("ETH", "USDT")),
                side: Side::Sell,
                order_type: OrderType::Limit,
                price: Decimal::new(3000, 0),
                amount: Decimal::new(10, 0),
                timestamp: None,
                exchange_params: HashMap::new(),
            },
        ];

        let bulk_request = BulkOrderRequest { requests };

        assert_eq!(bulk_request.requests.len(), 2);
        assert_eq!(bulk_request.requests[0].instrument.symbol(), "BTCUSDT");
        assert_eq!(bulk_request.requests[1].instrument.symbol(), "ETHUSDT");
    }

    #[test]
    fn test_bulk_order_response_creation() {
        let order1 = Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(1, 0),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let order2 = Order {
            order_id: OrderId::new("67890".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("ETH", "USDT")),
            side: Side::Sell,
            price: Decimal::new(3000, 0),
            amount: Decimal::new(10, 0),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let results = vec![
            BulkOperationResult {
                success: true,
                data: Some(order1.clone()),
                error: None,
                index: 0,
            },
            BulkOperationResult {
                success: false,
                data: None,
                error: Some("Insufficient balance".to_string()),
                index: 1,
            },
            BulkOperationResult {
                success: true,
                data: Some(order2.clone()),
                error: None,
                index: 2,
            },
        ];

        let response = BulkOrderResponse {
            results: results.clone(),
            success_count: 2,
            failure_count: 1,
        };

        assert_eq!(response.results.len(), 3);
        assert_eq!(response.success_count, 2);
        assert_eq!(response.failure_count, 1);
        assert_eq!(response.results[0].data, Some(order1));
        assert_eq!(response.results[2].data, Some(order2));
    }

    #[test]
    fn test_bulk_cancel_request_creation() {
        let order_ids = vec![
            OrderId::new("12345".into()),
            OrderId::new("67890".into()),
            OrderId::new("11111".into()),
        ];

        let request_with_pair = BulkCancelRequest {
            order_ids: order_ids.clone(),
            pair: Some(Pair::new("BTC", "USDT")),
        };

        let request_without_pair = BulkCancelRequest {
            order_ids: order_ids.clone(),
            pair: None,
        };

        assert_eq!(request_with_pair.order_ids.len(), 3);
        assert_eq!(request_with_pair.pair, Some(Pair::new("BTC", "USDT")));
        assert_eq!(request_without_pair.pair, None);
    }

    #[test]
    fn test_bulk_cancel_response_creation() {
        let cancelled_ids = vec![OrderId::new("12345".into()), OrderId::new("67890".into())];

        let failed_ids = vec![(OrderId::new("11111".into()), "Order not found".to_string())];

        let results = vec![
            BulkOperationResult {
                success: true,
                data: Some(OrderId::new("12345".into())),
                error: None,
                index: 0,
            },
            BulkOperationResult {
                success: true,
                data: Some(OrderId::new("67890".into())),
                error: None,
                index: 1,
            },
            BulkOperationResult {
                success: false,
                data: None,
                error: Some("Order not found".to_string()),
                index: 2,
            },
        ];

        let response = BulkCancelResponse {
            results,
            cancelled_ids: cancelled_ids.clone(),
            failed_ids: failed_ids.clone(),
        };

        assert_eq!(response.cancelled_ids.len(), 2);
        assert_eq!(response.failed_ids.len(), 1);
        assert_eq!(response.cancelled_ids[0], OrderId::new("12345".into()));
        assert_eq!(response.failed_ids[0].0, OrderId::new("11111".into()));
    }

    #[test]
    fn test_bulk_amend_item_creation() {
        let amend_item = BulkAmendItem {
            order_id: OrderId::new("12345".into()),
            new_request: OrderRequest {
                instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
                side: Side::Buy,
                order_type: OrderType::Limit,
                price: Decimal::new(91000, 0),
                amount: Decimal::new(2, 0),
                timestamp: None,
                exchange_params: HashMap::new(),
            },
        };

        assert_eq!(amend_item.order_id.as_str(), "12345");
        assert_eq!(amend_item.new_request.price, Decimal::new(91000, 0));
        assert_eq!(amend_item.new_request.amount, Decimal::new(2, 0));
    }

    #[test]
    fn test_bulk_amend_request_creation() {
        let amendments = vec![
            BulkAmendItem {
                order_id: OrderId::new("12345".into()),
                new_request: OrderRequest {
                    instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
                    side: Side::Buy,
                    order_type: OrderType::Limit,
                    price: Decimal::new(91000, 0),
                    amount: Decimal::new(2, 0),
                    timestamp: None,
                    exchange_params: HashMap::new(),
                },
            },
            BulkAmendItem {
                order_id: OrderId::new("67890".into()),
                new_request: OrderRequest {
                    instrument: Instrument::new(InstrumentType::Spot, Pair::new("ETH", "USDT")),
                    side: Side::Sell,
                    order_type: OrderType::Limit,
                    price: Decimal::new(3100, 0),
                    amount: Decimal::new(15, 0),
                    timestamp: None,
                    exchange_params: HashMap::new(),
                },
            },
        ];

        let request = BulkAmendRequest { amendments };

        assert_eq!(request.amendments.len(), 2);
        assert_eq!(request.amendments[0].order_id.as_str(), "12345");
        assert_eq!(request.amendments[1].order_id.as_str(), "67890");
    }

    #[test]
    fn test_bulk_amend_response_creation() {
        let amended_order = Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(91000, 0),
            amount: Decimal::new(2, 0),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::Amended,
            filled_amount: Decimal::ZERO,
        };

        let failed_amendments = vec![(
            OrderId::new("67890".into()),
            "Order already filled".to_string(),
        )];

        let results = vec![
            BulkOperationResult {
                success: true,
                data: Some(amended_order.clone()),
                error: None,
                index: 0,
            },
            BulkOperationResult {
                success: false,
                data: None,
                error: Some("Order already filled".to_string()),
                index: 1,
            },
        ];

        let response = BulkAmendResponse {
            results,
            amended_orders: vec![amended_order.clone()],
            failed_amendments: failed_amendments.clone(),
        };

        assert_eq!(response.amended_orders.len(), 1);
        assert_eq!(response.failed_amendments.len(), 1);
        assert_eq!(
            response.amended_orders[0].order_id,
            OrderId::new("12345".into())
        );
        assert_eq!(
            response.failed_amendments[0].0,
            OrderId::new("67890".into())
        );
    }

    #[test]
    fn test_bulk_order_request_serialization() {
        let request = BulkOrderRequest {
            requests: vec![OrderRequest {
                instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
                side: Side::Buy,
                order_type: OrderType::Limit,
                price: Decimal::new(90000, 0),
                amount: Decimal::new(1, 0),
                timestamp: None,
                exchange_params: HashMap::new(),
            }],
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
        assert!(serialized.contains("requests"));
    }

    #[test]
    fn test_bulk_operation_result_serialization() {
        let result = BulkOperationResult {
            success: true,
            data: Some(OrderId::new("12345".into())),
            error: None,
            index: 0,
        };

        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("success"));
        assert!(serialized.contains("12345"));
        assert!(serialized.contains("index"));
    }

    #[test]
    fn test_bulk_cancel_request_serialization() {
        let request = BulkCancelRequest {
            order_ids: vec![OrderId::new("12345".into()), OrderId::new("67890".into())],
            pair: Some(Pair::new("BTC", "USDT")),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("orderIds"));
        assert!(serialized.contains("pair"));
        assert!(serialized.contains("12345"));
    }

    #[test]
    fn test_bulk_amend_request_serialization() {
        let request = BulkAmendRequest {
            amendments: vec![BulkAmendItem {
                order_id: OrderId::new("12345".into()),
                new_request: OrderRequest {
                    instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
                    side: Side::Buy,
                    order_type: OrderType::Limit,
                    price: Decimal::new(91000, 0),
                    amount: Decimal::new(2, 0),
                    timestamp: None,
                    exchange_params: HashMap::new(),
                },
            }],
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("amendments"));
        assert!(serialized.contains("12345"));
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
    }
}
