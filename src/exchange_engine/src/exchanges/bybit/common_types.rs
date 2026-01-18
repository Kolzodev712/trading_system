//! Common Bybit types shared between REST and WebSocket

use serde::{Deserialize, Serialize};
use trading_core::types::{
    common::{InstrumentType, OrderStatus, OrderType, Side},
    futures::PositionSide,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum BybitSide {
    Buy,
    Sell,
    #[serde(rename = "")]
    Empty,
}

impl From<BybitSide> for Side {
    fn from(side: BybitSide) -> Self {
        match side {
            BybitSide::Buy => Side::Buy,
            BybitSide::Sell => Side::Sell,
            BybitSide::Empty => Side::Buy,
        }
    }
}

impl From<Side> for BybitSide {
    fn from(side: Side) -> Self {
        match side {
            Side::Buy => BybitSide::Buy,
            Side::Sell => BybitSide::Sell,
        }
    }
}

impl TryFrom<&str> for BybitSide {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Buy" => Ok(BybitSide::Buy),
            "Sell" => Ok(BybitSide::Sell),
            "" => Ok(BybitSide::Empty),
            _ => Err(()),
        }
    }
}

impl From<BybitSide> for PositionSide {
    fn from(side: BybitSide) -> Self {
        match side {
            BybitSide::Buy => PositionSide::Long,
            BybitSide::Sell => PositionSide::Short,
            BybitSide::Empty => PositionSide::Long,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum BybitOrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Created,
    Deactivated,
    Active,
    Untriggered,
    Triggered,
    #[serde(other)]
    Unknown,
}

impl From<BybitOrderStatus> for OrderStatus {
    fn from(status: BybitOrderStatus) -> Self {
        match status {
            BybitOrderStatus::New | BybitOrderStatus::PartiallyFilled => OrderStatus::New,
            BybitOrderStatus::Filled => OrderStatus::Filled,
            BybitOrderStatus::Cancelled | BybitOrderStatus::Rejected => OrderStatus::Canceled,
            _ => OrderStatus::New,
        }
    }
}

impl TryFrom<&str> for BybitOrderStatus {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "New" => Ok(BybitOrderStatus::New),
            "PartiallyFilled" => Ok(BybitOrderStatus::PartiallyFilled),
            "Filled" => Ok(BybitOrderStatus::Filled),
            "Cancelled" => Ok(BybitOrderStatus::Cancelled),
            "Rejected" => Ok(BybitOrderStatus::Rejected),
            "Created" => Ok(BybitOrderStatus::Created),
            "Deactivated" => Ok(BybitOrderStatus::Deactivated),
            "Active" => Ok(BybitOrderStatus::Active),
            "Untriggered" => Ok(BybitOrderStatus::Untriggered),
            "Triggered" => Ok(BybitOrderStatus::Triggered),
            _ => Ok(BybitOrderStatus::Unknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum BybitOrderType {
    Limit,
    Market,
    #[serde(other)]
    Unknown,
}

impl From<BybitOrderType> for OrderType {
    fn from(order_type: BybitOrderType) -> Self {
        match order_type {
            BybitOrderType::Limit => OrderType::Limit,
            BybitOrderType::Market => OrderType::Market,
            BybitOrderType::Unknown => OrderType::Limit,
        }
    }
}

impl TryFrom<&str> for BybitOrderType {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Limit" => Ok(BybitOrderType::Limit),
            "Market" => Ok(BybitOrderType::Market),
            _ => Ok(BybitOrderType::Unknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BybitCategory {
    Spot,
    Linear,
    Inverse,
    Option,
}

impl From<BybitCategory> for InstrumentType {
    fn from(category: BybitCategory) -> Self {
        match category {
            BybitCategory::Spot => InstrumentType::Spot,
            BybitCategory::Linear | BybitCategory::Inverse => InstrumentType::Futures,
            BybitCategory::Option => InstrumentType::Derivitives,
        }
    }
}

impl From<InstrumentType> for BybitCategory {
    fn from(instrument_type: InstrumentType) -> Self {
        match instrument_type {
            InstrumentType::Spot => BybitCategory::Spot,
            InstrumentType::Futures => BybitCategory::Linear, // Default to linear for futures
            InstrumentType::Derivitives => BybitCategory::Option,
        }
    }
}

impl TryFrom<&str> for BybitCategory {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "spot" => Ok(BybitCategory::Spot),
            "linear" => Ok(BybitCategory::Linear),
            "inverse" => Ok(BybitCategory::Inverse),
            "option" => Ok(BybitCategory::Option),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_bybit_side_deserialization() {
        assert_eq!(
            serde_json::from_str::<BybitSide>(r#""Buy""#).unwrap(),
            BybitSide::Buy
        );
        assert_eq!(
            serde_json::from_str::<BybitSide>(r#""Sell""#).unwrap(),
            BybitSide::Sell
        );
        assert_eq!(
            serde_json::from_str::<BybitSide>(r#""""#).unwrap(),
            BybitSide::Empty
        );
    }

    #[test]
    fn test_bybit_side_to_side() {
        assert_eq!(Side::from(BybitSide::Buy), Side::Buy);
        assert_eq!(Side::from(BybitSide::Sell), Side::Sell);
        assert_eq!(Side::from(BybitSide::Empty), Side::Buy);
    }

    #[test]
    fn test_bybit_side_try_from() {
        assert_eq!(BybitSide::try_from("Buy").unwrap(), BybitSide::Buy);
        assert_eq!(BybitSide::try_from("Sell").unwrap(), BybitSide::Sell);
        assert_eq!(BybitSide::try_from("").unwrap(), BybitSide::Empty);
        assert!(BybitSide::try_from("Invalid").is_err());
    }

    #[test]
    fn test_bybit_side_to_position_side() {
        assert_eq!(PositionSide::from(BybitSide::Buy), PositionSide::Long);
        assert_eq!(PositionSide::from(BybitSide::Sell), PositionSide::Short);
        assert_eq!(PositionSide::from(BybitSide::Empty), PositionSide::Long);
    }

    #[test]
    fn test_bybit_order_status_deserialization() {
        assert_eq!(
            serde_json::from_str::<BybitOrderStatus>(r#""New""#).unwrap(),
            BybitOrderStatus::New
        );
        assert_eq!(
            serde_json::from_str::<BybitOrderStatus>(r#""Filled""#).unwrap(),
            BybitOrderStatus::Filled
        );
        assert_eq!(
            serde_json::from_str::<BybitOrderStatus>(r#""UnknownStatus""#).unwrap(),
            BybitOrderStatus::Unknown
        );
    }

    #[test]
    fn test_bybit_order_status_to_order_status() {
        assert_eq!(OrderStatus::from(BybitOrderStatus::New), OrderStatus::New);
        assert_eq!(
            OrderStatus::from(BybitOrderStatus::PartiallyFilled),
            OrderStatus::New
        );
        assert_eq!(
            OrderStatus::from(BybitOrderStatus::Filled),
            OrderStatus::Filled
        );
        assert_eq!(
            OrderStatus::from(BybitOrderStatus::Cancelled),
            OrderStatus::Canceled
        );
        assert_eq!(
            OrderStatus::from(BybitOrderStatus::Rejected),
            OrderStatus::Canceled
        );
    }

    #[test]
    fn test_bybit_order_status_try_from() {
        assert_eq!(
            BybitOrderStatus::try_from("New").unwrap(),
            BybitOrderStatus::New
        );
        assert_eq!(
            BybitOrderStatus::try_from("PartiallyFilled").unwrap(),
            BybitOrderStatus::PartiallyFilled
        );
        assert_eq!(
            BybitOrderStatus::try_from("InvalidStatus").unwrap(),
            BybitOrderStatus::Unknown
        );
    }

    #[test]
    fn test_bybit_order_type_deserialization() {
        assert_eq!(
            serde_json::from_str::<BybitOrderType>(r#""Limit""#).unwrap(),
            BybitOrderType::Limit
        );
        assert_eq!(
            serde_json::from_str::<BybitOrderType>(r#""Market""#).unwrap(),
            BybitOrderType::Market
        );
        assert_eq!(
            serde_json::from_str::<BybitOrderType>(r#""UnknownType""#).unwrap(),
            BybitOrderType::Unknown
        );
    }

    #[test]
    fn test_bybit_order_type_to_order_type() {
        assert_eq!(OrderType::from(BybitOrderType::Limit), OrderType::Limit);
        assert_eq!(OrderType::from(BybitOrderType::Market), OrderType::Market);
        assert_eq!(OrderType::from(BybitOrderType::Unknown), OrderType::Limit);
    }

    #[test]
    fn test_bybit_order_type_try_from() {
        assert_eq!(
            BybitOrderType::try_from("Limit").unwrap(),
            BybitOrderType::Limit
        );
        assert_eq!(
            BybitOrderType::try_from("Market").unwrap(),
            BybitOrderType::Market
        );
        assert_eq!(
            BybitOrderType::try_from("InvalidType").unwrap(),
            BybitOrderType::Unknown
        );
    }

    #[test]
    fn test_bybit_category_deserialization() {
        assert_eq!(
            serde_json::from_str::<BybitCategory>(r#""spot""#).unwrap(),
            BybitCategory::Spot
        );
        assert_eq!(
            serde_json::from_str::<BybitCategory>(r#""linear""#).unwrap(),
            BybitCategory::Linear
        );
        assert_eq!(
            serde_json::from_str::<BybitCategory>(r#""inverse""#).unwrap(),
            BybitCategory::Inverse
        );
        assert_eq!(
            serde_json::from_str::<BybitCategory>(r#""option""#).unwrap(),
            BybitCategory::Option
        );
    }

    #[test]
    fn test_bybit_category_try_from() {
        assert_eq!(
            BybitCategory::try_from("spot").unwrap(),
            BybitCategory::Spot
        );
        assert_eq!(
            BybitCategory::try_from("linear").unwrap(),
            BybitCategory::Linear
        );
        assert_eq!(
            BybitCategory::try_from("inverse").unwrap(),
            BybitCategory::Inverse
        );
        assert_eq!(
            BybitCategory::try_from("option").unwrap(),
            BybitCategory::Option
        );
        assert!(BybitCategory::try_from("invalid").is_err());
    }

    #[test]
    fn test_bybit_category_to_instrument_type() {
        assert_eq!(
            InstrumentType::from(BybitCategory::Spot),
            InstrumentType::Spot
        );
        assert_eq!(
            InstrumentType::from(BybitCategory::Linear),
            InstrumentType::Futures
        );
        assert_eq!(
            InstrumentType::from(BybitCategory::Inverse),
            InstrumentType::Futures
        );
        assert_eq!(
            InstrumentType::from(BybitCategory::Option),
            InstrumentType::Derivitives
        );
    }

    #[test]
    fn test_instrument_type_to_bybit_category() {
        assert_eq!(
            BybitCategory::from(InstrumentType::Spot),
            BybitCategory::Spot
        );
        assert_eq!(
            BybitCategory::from(InstrumentType::Futures),
            BybitCategory::Linear
        );
        assert_eq!(
            BybitCategory::from(InstrumentType::Derivitives),
            BybitCategory::Option
        );
    }
}
