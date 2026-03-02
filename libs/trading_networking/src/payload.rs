//! Payload (de)serialization for events and commands (wire format over Zenoh or other transports).

use crate::commands::CommandPayload;
use crate::error::NetworkError;
use crate::events::{ColdEvent, HotEvent};
use bytes::Bytes;

/// Serialize a hot event to bytes (JSON).
pub fn serialize_hot_event(event: &HotEvent) -> Result<Bytes, NetworkError> {
    serde_json::to_vec(event)
        .map(Bytes::from)
        .map_err(|e| NetworkError::SerializationFailed(format!("hot event: {}", e)))
}

/// Serialize a cold event to bytes (JSON).
pub fn serialize_cold_event(event: &ColdEvent) -> Result<Bytes, NetworkError> {
    serde_json::to_vec(event)
        .map(Bytes::from)
        .map_err(|e| NetworkError::SerializationFailed(format!("cold event: {}", e)))
}

/// Serialize a command payload to bytes (JSON).
pub fn serialize_command(payload: &CommandPayload) -> Result<Bytes, NetworkError> {
    serde_json::to_vec(payload)
        .map(Bytes::from)
        .map_err(|e| NetworkError::SerializationFailed(format!("command: {}", e)))
}

/// Deserialize a hot event from bytes.
pub fn deserialize_hot_event(payload: &[u8]) -> Result<HotEvent, NetworkError> {
    serde_json::from_slice(payload)
        .map_err(|e| NetworkError::DeserializationFailed(format!("hot event: {}", e)))
}

/// Deserialize a cold event from bytes.
pub fn deserialize_cold_event(payload: &[u8]) -> Result<ColdEvent, NetworkError> {
    serde_json::from_slice(payload)
        .map_err(|e| NetworkError::DeserializationFailed(format!("cold event: {}", e)))
}

/// Deserialize a command payload from bytes.
pub fn deserialize_command(payload: &[u8]) -> Result<CommandPayload, NetworkError> {
    serde_json::from_slice(payload)
        .map_err(|e| NetworkError::DeserializationFailed(format!("command: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::{Pair, Symbol};
    use trading_core::types::exchange::Balance;

    #[test]
    fn hot_event_roundtrip() {
        let event = HotEvent::Trade {
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(50000, 0),
            quantity: Decimal::new(1, 0),
            timestamp: 1_000_000,
            trade_id: None,
            trace_context: None,
        };
        let bytes = serialize_hot_event(&event).unwrap();
        let decoded = deserialize_hot_event(&bytes).unwrap();
        match (&event, &decoded) {
            (
                HotEvent::Trade {
                    pair: p1,
                    price: pr1,
                    quantity: q1,
                    timestamp: t1,
                    ..
                },
                HotEvent::Trade {
                    pair: p2,
                    price: pr2,
                    quantity: q2,
                    timestamp: t2,
                    ..
                },
            ) => {
                assert_eq!(p1, p2);
                assert_eq!(pr1, pr2);
                assert_eq!(q1, q2);
                assert_eq!(t1, t2);
            }
            _ => panic!("variant mismatch"),
        }
    }

    #[test]
    fn cold_event_roundtrip() {
        let balance = Balance {
            symbol: Symbol::from("USDT"),
            free: Decimal::new(1000, 0),
            used: Decimal::new(100, 0),
            total: Decimal::new(1100, 0),
        };
        let event = ColdEvent::Balance {
            balance: balance.clone(),
            trace_context: None,
        };
        let bytes = serialize_cold_event(&event).unwrap();
        let decoded = deserialize_cold_event(&bytes).unwrap();
        match &decoded {
            ColdEvent::Balance { balance: b, .. } => {
                assert_eq!(b.symbol.as_ref(), balance.symbol.as_ref());
                assert_eq!(b.free, balance.free);
                assert_eq!(b.used, balance.used);
                assert_eq!(b.total, balance.total);
            }
            _ => panic!("expected Balance variant"),
        }
    }

    #[test]
    fn command_roundtrip() {
        let payload = CommandPayload::SetTradingEnabled { enabled: true };
        let bytes = serialize_command(&payload).unwrap();
        let decoded = deserialize_command(&bytes).unwrap();
        match (payload, decoded) {
            (
                CommandPayload::SetTradingEnabled { enabled: a },
                CommandPayload::SetTradingEnabled { enabled: b },
            ) => assert_eq!(a, b),
            _ => panic!("variant mismatch"),
        }
    }
}
