//! Bybit WebSocket message types

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

pub use crate::exchanges::bybit::common_types::{
    BybitCategory, BybitOrderStatus, BybitOrderType, BybitSide,
};

pub mod execution;
pub mod order;
pub mod position;
pub mod rpc;
pub mod wallet;
pub mod ws_streaming;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum BybitPositionStatus {
    Normal,
    Liq,
    Adl,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BybitWebSocketMessage<T> {
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
    pub data: T,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_bybit_position_status_deserialization() {
        assert_eq!(
            serde_json::from_str::<BybitPositionStatus>(r#""Normal""#).unwrap(),
            BybitPositionStatus::Normal
        );
        assert_eq!(
            serde_json::from_str::<BybitPositionStatus>(r#""Liq""#).unwrap(),
            BybitPositionStatus::Liq
        );
        assert_eq!(
            serde_json::from_str::<BybitPositionStatus>(r#""Adl""#).unwrap(),
            BybitPositionStatus::Adl
        );
    }
}
