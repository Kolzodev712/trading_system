//! Bybit position response types
//! See: <https://bybit-exchange.github.io/docs/v5/position>

use super::{BybitCategory, BybitSide};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::{
        common::{Instrument, InstrumentType, Pair},
        exchange::Position,
    },
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitPositionResponse {
    pub ret_code: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub ret_msg: SmolStr,
    pub result: BybitPositionResult,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}

fn deserialize_bybit_category<'de, D>(deserializer: D) -> Result<BybitCategory, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    BybitCategory::try_from(s.as_str())
        .map_err(|_| serde::de::Error::custom(format!("Unknown category: {}", s)))
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitPositionResult {
    #[serde(deserialize_with = "deserialize_bybit_category")]
    pub category: BybitCategory,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub next_page_cursor: SmolStr,
    pub list: Vec<BybitPosition>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitPosition {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub base_coin: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub settle_coin: SmolStr,
    #[serde(default)]
    pub side: Option<BybitSide>,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub avg_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub mark_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub cum_realised_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub position_im: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub position_mm: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub leverage: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub liq_price: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub updated_time: SmolStr,
}

impl BybitPosition {
    pub fn to_position(
        self,
        category: &BybitCategory,
        symbol_registry: Arc<SymbolRegistry>,
    ) -> Option<Position> {
        let side = self.side?;
        if self.size.is_zero() {
            return None;
        }

        let pair = if !self.base_coin.is_empty() && !self.settle_coin.is_empty() {
            Pair::new(&self.base_coin, &self.settle_coin)
        } else {
            symbol_registry.get_or_parse_pair(&self.symbol)
        };

        let quantity = match side {
            BybitSide::Buy => self.size,
            BybitSide::Sell => -self.size,
            BybitSide::Empty => return None,
        };

        let instrument_type: InstrumentType = category.clone().into();

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
            avg_price: self.avg_price,
            current_price: self.mark_price,
            unrealized_pnl: self.unrealised_pnl,
            last_update,
            mark_price: Some(self.mark_price),
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
            liquidation_price,
            index_price: None,
            position_mode: None,
            margin_type: None,
        })
    }
}

impl BybitPositionResponse {
    pub fn to_positions(self, symbol_registry: Arc<SymbolRegistry>) -> Vec<Position> {
        let category = self.result.category.clone();
        self.result
            .list
            .into_iter()
            .filter_map(|pos| pos.to_position(&category, symbol_registry.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::Pair;

    #[test]
    fn test_bybit_position_response_deserialization() {
        let json = r#"
        {
            "retCode": 0,
            "retMsg": "OK",
            "result": {
                "category": "linear",
                "nextPageCursor": "",
                "list": [
                    {
                        "symbol": "BTCUSDT",
                        "side": "Buy",
                        "size": "1.5",
                        "avgPrice": "50000.0",
                        "markPrice": "51000.0",
                        "unrealisedPnl": "1500.0",
                        "cumRealisedPnl": "500.0",
                        "positionIM": "2500.0",
                        "positionMM": "250.0",
                        "leverage": "10",
                        "liqPrice": "45000.0",
                        "updatedTime": "1690872862481"
                    }
                ]
            },
            "retExtInfo": {},
            "time": 1690872862481
        }
        "#;

        let response: BybitPositionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert_eq!(response.result.category, BybitCategory::Linear);
        assert_eq!(response.result.list.len(), 1);
        assert_eq!(response.result.list[0].symbol.as_str(), "BTCUSDT");
        assert_eq!(response.result.list[0].side, Some(BybitSide::Buy));
    }

    #[test]
    fn test_bybit_position_to_position_long() {
        let bybit_pos = BybitPosition {
            symbol: SmolStr::new("BTCUSDT"),
            base_coin: SmolStr::new("BTC"),
            settle_coin: SmolStr::new("USDT"),
            side: Some(BybitSide::Buy),
            size: Decimal::new(150, 1),
            avg_price: Decimal::new(500000, 2),
            mark_price: Decimal::new(510000, 2),
            unrealised_pnl: Decimal::new(15000, 1),
            cum_realised_pnl: Decimal::new(5000, 1),
            position_im: Decimal::new(25000, 1),
            position_mm: Decimal::new(2500, 1),
            leverage: SmolStr::new("10"),
            liq_price: SmolStr::new("45000.0"),
            updated_time: SmolStr::new("1690872862481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos
            .to_position(&BybitCategory::Linear, symbol_registry)
            .unwrap();

        assert_eq!(position.pair().base, "BTC");
        assert_eq!(position.pair().quote, "USDT");
        assert_eq!(position.quantity, Decimal::new(150, 1));
        assert_eq!(position.avg_price, Decimal::new(500000, 2));
        assert_eq!(position.unrealized_pnl, Decimal::new(15000, 1));
        assert_eq!(position.leverage, Some(10));
    }

    #[test]
    fn test_bybit_position_to_position_short() {
        let bybit_pos = BybitPosition {
            symbol: SmolStr::new("BTCUSDT"),
            base_coin: SmolStr::new("BTC"),
            settle_coin: SmolStr::new("USDT"),
            side: Some(BybitSide::Sell),
            size: Decimal::new(100, 1),
            avg_price: Decimal::new(500000, 2),
            mark_price: Decimal::new(490000, 2),
            unrealised_pnl: Decimal::new(10000, 1),
            cum_realised_pnl: Decimal::ZERO,
            position_im: Decimal::new(20000, 1),
            position_mm: Decimal::new(2000, 1),
            leverage: SmolStr::new("20"),
            liq_price: SmolStr::new("55000.0"),
            updated_time: SmolStr::new("1690872862481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos
            .to_position(&BybitCategory::Linear, symbol_registry)
            .unwrap();

        assert_eq!(position.quantity, Decimal::new(-100, 1));
        assert_eq!(position.unrealized_pnl, Decimal::new(10000, 1));
    }

    #[test]
    fn test_bybit_position_empty_side_returns_none() {
        let bybit_pos = BybitPosition {
            symbol: SmolStr::new("BTCUSDT"),
            base_coin: SmolStr::new("BTC"),
            settle_coin: SmolStr::new("USDT"),
            side: None,
            size: Decimal::new(100, 1),
            avg_price: Decimal::new(500000, 2),
            mark_price: Decimal::new(500000, 2),
            unrealised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            position_im: Decimal::ZERO,
            position_mm: Decimal::ZERO,
            leverage: SmolStr::new(""),
            liq_price: SmolStr::new(""),
            updated_time: SmolStr::new("1690872862481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos.to_position(&BybitCategory::Linear, symbol_registry);

        assert!(position.is_none());
    }

    #[test]
    fn test_bybit_position_zero_size_returns_none() {
        let bybit_pos = BybitPosition {
            symbol: SmolStr::new("BTCUSDT"),
            base_coin: SmolStr::new("BTC"),
            settle_coin: SmolStr::new("USDT"),
            side: Some(BybitSide::Buy),
            size: Decimal::ZERO,
            avg_price: Decimal::new(500000, 2),
            mark_price: Decimal::new(500000, 2),
            unrealised_pnl: Decimal::ZERO,
            cum_realised_pnl: Decimal::ZERO,
            position_im: Decimal::ZERO,
            position_mm: Decimal::ZERO,
            leverage: SmolStr::new("10"),
            liq_price: SmolStr::new(""),
            updated_time: SmolStr::new("1690872862481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let position = bybit_pos.to_position(&BybitCategory::Linear, symbol_registry);

        assert!(position.is_none());
    }
}
