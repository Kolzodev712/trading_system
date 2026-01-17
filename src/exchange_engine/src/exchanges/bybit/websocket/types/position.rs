//! Bybit WebSocket position message type
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/private/position>

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::{
        common::{Instrument, InstrumentType},
        exchange::Position,
    },
    utils::{deserialize_smol_str, serialize_smol_str},
};

use super::{BybitCategory, BybitPositionStatus, BybitSide};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitPositionMessage {
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
    pub data: Vec<BybitPositionData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitPositionData {
    pub position_idx: i32,
    pub risk_id: i32,
    #[serde(with = "rust_decimal::serde::str")]
    pub risk_limit_value: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(default)]
    pub side: Option<BybitSide>,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub position_value: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub entry_price: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub leverage: SmolStr,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub break_even_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub mark_price: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub position_im: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub position_mm: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub liq_price: SmolStr,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub take_profit: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub stop_loss: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub trailing_stop: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub cur_realised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub cum_realised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub session_avg_price: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub created_time: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub updated_time: SmolStr,
    pub category: BybitCategory,
    pub position_status: BybitPositionStatus,
    pub adl_rank_indicator: i32,
    pub auto_add_margin: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub leverage_sys_updated_time: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub mmr_sys_updated_time: SmolStr,
    pub is_reduce_only: bool,
    pub seq: i64,
}

impl BybitPositionData {
    pub fn to_position(self, symbol_registry: Arc<SymbolRegistry>) -> Option<Position> {
        let pair = symbol_registry.get_or_parse_pair(&self.symbol);

        let side = self.side?;
        if self.size.is_zero() {
            return None;
        }

        let quantity = match side {
            BybitSide::Buy => self.size,
            BybitSide::Sell => -self.size,
            BybitSide::Empty => return None,
        };

        let instrument_type: InstrumentType = self.category.clone().into();

        let leverage = self.leverage.parse::<u32>().ok().filter(|&v| v > 0);

        let liquidation_price = if !self.liq_price.is_empty() {
            self.liq_price.parse::<Decimal>().ok()
        } else {
            None
        };

        let last_update = self.updated_time.parse::<u64>().unwrap_or(0);

        Some(Position {
            instrument: Instrument::new(instrument_type, pair),
            quantity,
            avg_price: self.entry_price,
            current_price: self.mark_price,
            unrealized_pnl: self.unrealised_pnl,
            last_update,
            mark_price: Some(self.mark_price),
            index_price: None,
            realized_pnl: Some(self.cum_realised_pnl),
            initial_margin: if self.position_im.is_zero() {
                None
            } else {
                Some(self.position_im)
            },
            maintenance_margin: if self.position_mm.is_zero() {
                None
            } else {
                Some(self.position_mm)
            },
            leverage,
            position_mode: None,
            margin_type: None,
            liquidation_price,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Pair;

    #[test]
    fn test_bybit_position_message_deserialization() {
        let json = r#"
        {
            "id": "1003076014fb7eedb-c7e6-45d6-a8c1-270f0169171a",
            "topic": "position",
            "creationTime": 1697682317044,
            "data": [
                {
                    "positionIdx": 2,
                    "tradeMode": 0,
                    "riskId": 1,
                    "riskLimitValue": "2000000",
                    "symbol": "BTCUSDT",
                    "side": "Buy",
                    "size": "0.1",
                    "entryPrice": "27464.50",
                    "leverage": "10",
                    "breakEvenPrice": "93556.73034991",
                    "positionValue": "0.01092319",
                    "positionBalance": "0",
                    "markPrice": "28184.5",
                    "positionIM": "0.00010923",
                    "positionIMByMp": "0.00010923",
                    "positionMM": "0.0000015",
                    "positionMMByMp": "0.0000015",
                    "takeProfit": "0",
                    "stopLoss": "0",
                    "trailingStop": "0",
                    "unrealisedPnl": "-0.00029413",
                    "curRealisedPnl": "1.26",
                    "cumRealisedPnl": "-25.06579337",
                    "sessionAvgPrice": "0",
                    "createdTime": "1694402496913",
                    "updatedTime": "1697682317038",
                    "tpslMode": "Full",
                    "liqPrice": "25000.00",
                    "bustPrice": "",
                    "category": "linear",
                    "positionStatus": "Normal",
                    "adlRankIndicator": 0,
                    "autoAddMargin": 0,
                    "leverageSysUpdatedTime": "",
                    "mmrSysUpdatedTime": "",
                    "seq": 8327597863,
                    "isReduceOnly": false
                }
            ]
        }
        "#;

        let message: BybitPositionMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.topic, "position");
        assert_eq!(message.data.len(), 1);
        assert_eq!(message.data[0].symbol, "BTCUSDT");
        assert_eq!(message.data[0].side, Some(BybitSide::Buy));
        assert_eq!(message.data[0].category, BybitCategory::Linear);
        assert_eq!(message.data[0].position_status, BybitPositionStatus::Normal);
    }

    #[test]
    fn test_bybit_position_data_to_position_long() {
        let bybit_pos = BybitPositionData {
            position_idx: 0,
            risk_id: 1,
            risk_limit_value: Decimal::new(2000000, 0),
            symbol: SmolStr::new("BTCUSDT"),
            side: Some(BybitSide::Buy),
            size: Decimal::new(10, 1),
            position_value: Decimal::new(1092319, 8),
            entry_price: Decimal::new(2746450, 2),
            leverage: SmolStr::new("10"),
            break_even_price: Decimal::new(9355673034991, 11),
            mark_price: Decimal::new(281845, 1),
            position_im: Decimal::new(10923, 8),
            position_mm: Decimal::new(15, 7),
            liq_price: SmolStr::new("25000.00"),
            take_profit: Decimal::ZERO,
            stop_loss: Decimal::ZERO,
            trailing_stop: Decimal::ZERO,
            unrealised_pnl: Decimal::new(-29413, 8),
            cur_realised_pnl: Decimal::new(126, 2),
            cum_realised_pnl: Decimal::new(-2506579337, 8),
            session_avg_price: Decimal::ZERO,
            created_time: SmolStr::new("1694402496913"),
            updated_time: SmolStr::new("1697682317038"),
            category: BybitCategory::Linear,
            position_status: BybitPositionStatus::Normal,
            adl_rank_indicator: 0,
            auto_add_margin: 0,
            leverage_sys_updated_time: SmolStr::new(""),
            mmr_sys_updated_time: SmolStr::new(""),
            is_reduce_only: false,
            seq: 8327597863,
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos.to_position(symbol_registry).unwrap();

        assert_eq!(position.pair().base, "BTC");
        assert_eq!(position.pair().quote, "USDT");
        assert_eq!(position.quantity, Decimal::new(10, 1));
        assert_eq!(position.avg_price, Decimal::new(2746450, 2));
        assert_eq!(position.unrealized_pnl, Decimal::new(-29413, 8));
        assert_eq!(position.leverage, Some(10));
    }

    #[test]
    fn test_bybit_position_data_to_position_short() {
        let bybit_pos = BybitPositionData {
            position_idx: 0,
            risk_id: 1,
            risk_limit_value: Decimal::new(2000000, 0),
            symbol: SmolStr::new("BTCUSDT"),
            side: Some(BybitSide::Sell),
            size: Decimal::new(50, 1),
            position_value: Decimal::new(5000000, 2),
            entry_price: Decimal::new(5000000, 2),
            leverage: SmolStr::new("20"),
            break_even_price: Decimal::ZERO,
            mark_price: Decimal::new(4900000, 2),
            position_im: Decimal::new(20000, 1),
            position_mm: Decimal::new(2000, 1),
            liq_price: SmolStr::new("55000.00"),
            take_profit: Decimal::ZERO,
            stop_loss: Decimal::ZERO,
            trailing_stop: Decimal::ZERO,
            unrealised_pnl: Decimal::new(10000, 1),
            cur_realised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            session_avg_price: Decimal::ZERO,
            created_time: SmolStr::new("1690872862481"),
            updated_time: SmolStr::new("1690872862481"),
            category: BybitCategory::Linear,
            position_status: BybitPositionStatus::Normal,
            adl_rank_indicator: 0,
            auto_add_margin: 0,
            leverage_sys_updated_time: SmolStr::new(""),
            mmr_sys_updated_time: SmolStr::new(""),
            is_reduce_only: false,
            seq: 12345,
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos.to_position(symbol_registry).unwrap();

        assert_eq!(position.quantity, Decimal::new(-50, 1));
        assert_eq!(position.unrealized_pnl, Decimal::new(10000, 1));
        assert_eq!(position.leverage, Some(20));
    }

    #[test]
    fn test_bybit_position_data_empty_side_returns_none() {
        let bybit_pos = BybitPositionData {
            position_idx: 0,
            risk_id: 1,
            risk_limit_value: Decimal::ZERO,
            symbol: SmolStr::new("BTCUSDT"),
            side: None,
            size: Decimal::new(100, 1),
            position_value: Decimal::ZERO,
            entry_price: Decimal::ZERO,
            leverage: SmolStr::new(""),
            break_even_price: Decimal::ZERO,
            mark_price: Decimal::ZERO,
            position_im: Decimal::ZERO,
            position_mm: Decimal::ZERO,
            liq_price: SmolStr::new(""),
            take_profit: Decimal::ZERO,
            stop_loss: Decimal::ZERO,
            trailing_stop: Decimal::ZERO,
            unrealised_pnl: Decimal::ZERO,
            cur_realised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            session_avg_price: Decimal::ZERO,
            created_time: SmolStr::new("1690872862481"),
            updated_time: SmolStr::new("1690872862481"),
            category: BybitCategory::Linear,
            position_status: BybitPositionStatus::Normal,
            adl_rank_indicator: 0,
            auto_add_margin: 0,
            leverage_sys_updated_time: SmolStr::new(""),
            mmr_sys_updated_time: SmolStr::new(""),
            is_reduce_only: false,
            seq: 0,
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos.to_position(symbol_registry);
        assert!(position.is_none());
    }

    #[test]
    fn test_bybit_position_data_zero_size_returns_none() {
        let bybit_pos = BybitPositionData {
            position_idx: 0,
            risk_id: 1,
            risk_limit_value: Decimal::ZERO,
            symbol: SmolStr::new("BTCUSDT"),
            side: Some(BybitSide::Buy),
            size: Decimal::ZERO,
            position_value: Decimal::ZERO,
            entry_price: Decimal::ZERO,
            leverage: SmolStr::new("10"),
            break_even_price: Decimal::ZERO,
            mark_price: Decimal::ZERO,
            position_im: Decimal::ZERO,
            position_mm: Decimal::ZERO,
            liq_price: SmolStr::new(""),
            take_profit: Decimal::ZERO,
            stop_loss: Decimal::ZERO,
            trailing_stop: Decimal::ZERO,
            unrealised_pnl: Decimal::ZERO,
            cur_realised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            session_avg_price: Decimal::ZERO,
            created_time: SmolStr::new("1690872862481"),
            updated_time: SmolStr::new("1690872862481"),
            category: BybitCategory::Linear,
            position_status: BybitPositionStatus::Normal,
            adl_rank_indicator: 0,
            auto_add_margin: 0,
            leverage_sys_updated_time: SmolStr::new(""),
            mmr_sys_updated_time: SmolStr::new(""),
            is_reduce_only: false,
            seq: 0,
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos.to_position(symbol_registry);
        assert!(position.is_none());
    }
}
