//! DEX indicator and quote price types (pool-scoped, DeepBook orderbook–driven).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::str::FromStr;

/// Built-in indicator kinds computed from a pool orderbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DexIndicatorKind {
    Mid,
    BestBid,
    BestAsk,
    Spread,
}

/// Live value for a configured DEX indicator.
#[derive(Debug, Clone, PartialEq)]
pub struct DexIndicatorValue {
    pub indicator_id: SmolStr,
    pub name: SmolStr,
    pub pool: SmolStr,
    pub value: Decimal,
    pub timestamp: u64,
}

/// Quote price specification (fixed decimal or orderbook-derived).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DexPriceConfig {
    Fixed(Decimal),
    Mid {
        offset_percent: Decimal,
    },
    BestBid {
        offset: Decimal,
    },
    BestAsk {
        offset: Decimal,
    },
    /// Resolve via `quote_pricing` in `config/dex/indicators.json` for this quote id.
    Binding,
}

impl<'de> Deserialize<'de> for DexPriceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct DexPriceConfigVisitor;

        impl<'de> Visitor<'de> for DexPriceConfigVisitor {
            type Value = DexPriceConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a decimal string, \"Binding\", or an object with Mid/BestBid/BestAsk fields",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.eq_ignore_ascii_case("binding") {
                    return Ok(DexPriceConfig::Binding);
                }
                let price = Decimal::from_str(value).map_err(de::Error::custom)?;
                Ok(DexPriceConfig::Fixed(price))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Decimal::try_from(value)
                    .map(DexPriceConfig::Fixed)
                    .map_err(de::Error::custom)
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DexPriceConfig::Fixed(Decimal::from(value)))
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut offset_percent = None;
                let mut offset = None;
                let mut kind: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "offset_percent" => {
                            offset_percent = Some(map.next_value()?);
                            if kind.is_none() {
                                kind = Some("mid".to_string());
                            }
                        }
                        "offset" => {
                            offset = Some(map.next_value()?);
                            if kind.is_none() {
                                kind = Some("bid_or_ask".to_string());
                            }
                        }
                        "type" => {
                            kind = Some(map.next_value::<String>()?.to_ascii_lowercase());
                        }
                        other => {
                            return Err(de::Error::unknown_field(
                                other,
                                &["offset_percent", "offset", "type"],
                            ));
                        }
                    }
                }

                match kind.as_deref() {
                    Some("mid") => {
                        let offset_percent = offset_percent
                            .ok_or_else(|| de::Error::missing_field("offset_percent"))?;
                        Ok(DexPriceConfig::Mid { offset_percent })
                    }
                    Some("bestbid") => {
                        let offset = offset.ok_or_else(|| de::Error::missing_field("offset"))?;
                        Ok(DexPriceConfig::BestBid { offset })
                    }
                    Some("bestask") => {
                        let offset = offset.ok_or_else(|| de::Error::missing_field("offset"))?;
                        Ok(DexPriceConfig::BestAsk { offset })
                    }
                    Some("bid_or_ask") => {
                        let offset = offset.ok_or_else(|| de::Error::missing_field("offset"))?;
                        Ok(DexPriceConfig::BestBid { offset })
                    }
                    _ => Err(de::Error::custom(
                        "expected Mid, BestBid, or BestAsk price object",
                    )),
                }
            }
        }

        deserializer.deserialize_any(DexPriceConfigVisitor)
    }
}

/// Offset from a bound indicator (aligned with CEX `PricingOffsetType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DexPricingOffsetType {
    Absolute,
    Percent,
    PercentBelow,
    PercentAbove,
}
