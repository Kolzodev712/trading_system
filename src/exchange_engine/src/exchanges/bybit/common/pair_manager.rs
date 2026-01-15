//! Bybit pair and instrument management
//!
//! Handles mapping between our `Instrument` type and Bybit's symbol/category format.
//! Bybit uses the same symbol string (e.g., "BTCUSDT") for both spot and futures,
//! distinguished by the `category` parameter in API calls.

use smol_str::SmolStr;
use std::collections::HashMap;
use trading_core::types::common::{Instrument, InstrumentType, Pair};

/// Bybit category types for API calls
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BybitCategory {
    Spot,
    Linear,
    Inverse,
    Option,
}

impl BybitCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            BybitCategory::Spot => "spot",
            BybitCategory::Linear => "linear",
            BybitCategory::Inverse => "inverse",
            BybitCategory::Option => "option",
        }
    }
}

/// Mapping from Instrument to Bybit API format
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitInstrumentMapping {
    /// Bybit symbol string (e.g., "BTCUSDT")
    pub symbol: SmolStr,
    /// Bybit category (e.g., "spot", "linear")
    pub category: BybitCategory,
}

/// Bybit pair manager
///
/// Manages the mapping between our `Instrument` type and Bybit's symbol/category format.
/// Handles:
/// - Converting `Instrument` to Bybit API format (symbol + category)
/// - Reverse mapping from Bybit responses to `Instrument`
/// - Validating instruments exist on Bybit
/// - Caching instrument information
#[derive(Debug)]
pub struct BybitPairManager {
    /// Cache of validated instruments
    /// Maps (symbol, category) -> Instrument info
    instrument_cache: HashMap<(SmolStr, BybitCategory), InstrumentInfo>,
}

/// Cached instrument information
#[derive(Debug, Clone)]
struct InstrumentInfo {
    /// Our internal instrument representation
    instrument: Instrument,
    /// Bybit symbol string
    symbol: SmolStr,
    /// Bybit category
    category: BybitCategory,
    /// Whether the instrument is currently active/trading
    is_active: bool,
}

impl BybitPairManager {
    /// Create a new Bybit pair manager
    pub fn new() -> Self {
        Self {
            instrument_cache: HashMap::new(),
        }
    }

    /// Convert an `Instrument` to Bybit API format (symbol + category)
    ///
    /// # Arguments
    /// * `instrument` - Our internal instrument representation
    ///
    /// # Returns
    /// Mapping containing Bybit symbol and category
    pub fn instrument_to_bybit(&self, instrument: &Instrument) -> BybitInstrumentMapping {
        let symbol = instrument.pair.convert_to_smolstr();
        let category = Self::instrument_type_to_category(&instrument.instrument_type);

        BybitInstrumentMapping { symbol, category }
    }

    /// Convert Bybit symbol and category to our `Instrument` type
    ///
    /// # Arguments
    /// * `_symbol` - Bybit symbol string (e.g., "BTCUSDT") - reserved for future validation
    /// * `category` - Bybit category string (e.g., "spot", "linear")
    /// * `base_coin` - Base coin from Bybit response (e.g., "BTC")
    /// * `quote_coin` - Quote coin from Bybit response (e.g., "USDT")
    ///
    /// # Returns
    /// `Instrument` representation, or `None` if category is invalid
    ///
    /// # Note
    /// This is a basic implementation. Enhanced version will:
    /// - Validate symbol format matches base_coin + quote_coin
    /// - Handle edge cases (e.g., options with different symbol format)
    /// - Cache the result
    pub fn bybit_to_instrument(
        &self,
        _symbol: &str,
        category: &str,
        base_coin: &str,
        quote_coin: &str,
    ) -> Option<Instrument> {
        let instrument_type = Self::category_to_instrument_type(category)?;
        let pair = Pair::new(base_coin, quote_coin);

        Some(Instrument {
            instrument_type,
            pair,
        })
    }

    /// Get Bybit category string from InstrumentType
    fn instrument_type_to_category(instrument_type: &InstrumentType) -> BybitCategory {
        match instrument_type {
            InstrumentType::Spot => BybitCategory::Spot,
            InstrumentType::Futures => BybitCategory::Linear, // Default to linear for futures
            InstrumentType::Derivitives => BybitCategory::Option, // Options are derivatives
        }
    }

    /// Get InstrumentType from Bybit category string
    fn category_to_instrument_type(category: &str) -> Option<InstrumentType> {
        match category {
            "spot" => Some(InstrumentType::Spot),
            "linear" => Some(InstrumentType::Futures),
            "inverse" => Some(InstrumentType::Futures), // Inverse contracts are also futures
            "option" => Some(InstrumentType::Derivitives),
            _ => None,
        }
    }

    /// Validate that an instrument exists on Bybit
    ///
    /// # Arguments
    /// * `instrument` - Instrument to validate
    ///
    /// # Returns
    /// `true` if instrument is cached and active, `false` otherwise
    ///
    /// # Note
    /// This is a placeholder. Full implementation will:
    /// - Query Bybit API if not in cache
    /// - Handle pagination for large instrument lists
    /// - Check instrument status (Trading, PreLaunch, etc.)
    pub fn validate_instrument(&self, instrument: &Instrument) -> bool {
        let mapping = self.instrument_to_bybit(instrument);
        let key = (mapping.symbol.clone(), mapping.category.clone());

        self.instrument_cache
            .get(&key)
            .map(|info| info.is_active)
            .unwrap_or(false)
    }

    /// Cache an instrument from Bybit API response
    ///
    /// # Arguments
    /// * `symbol` - Bybit symbol string
    /// * `category` - Bybit category
    /// * `base_coin` - Base coin
    /// * `quote_coin` - Quote coin
    /// * `status` - Instrument status (e.g., "Trading", "PreLaunch")
    ///
    /// # Note
    /// This is a placeholder. Full implementation will:
    /// - Parse all fields from Bybit response
    /// - Store trading rules (price precision, lot size, etc.)
    /// - Handle status transitions
    pub fn cache_instrument(
        &mut self,
        symbol: &str,
        category: &str,
        base_coin: &str,
        quote_coin: &str,
        status: &str,
    ) -> Option<Instrument> {
        let instrument = self.bybit_to_instrument(symbol, category, base_coin, quote_coin)?;
        let category_enum = Self::category_to_category_enum(category)?;
        let is_active = status == "Trading";

        let info = InstrumentInfo {
            instrument: instrument.clone(),
            symbol: SmolStr::new(symbol),
            category: category_enum,
            is_active,
        };

        let key = (info.symbol.clone(), info.category.clone());
        self.instrument_cache.insert(key, info);

        Some(instrument)
    }

    /// Convert category string to enum
    fn category_to_category_enum(category: &str) -> Option<BybitCategory> {
        match category {
            "spot" => Some(BybitCategory::Spot),
            "linear" => Some(BybitCategory::Linear),
            "inverse" => Some(BybitCategory::Inverse),
            "option" => Some(BybitCategory::Option),
            _ => None,
        }
    }

    /// Get all cached instruments
    pub fn get_cached_instruments(&self) -> Vec<&Instrument> {
        self.instrument_cache
            .values()
            .map(|info| &info.instrument)
            .collect()
    }

    /// Clear the instrument cache
    pub fn clear_cache(&mut self) {
        self.instrument_cache.clear();
    }
}

impl Default for BybitPairManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_to_bybit_spot() {
        let manager = BybitPairManager::new();
        let instrument = Instrument {
            instrument_type: InstrumentType::Spot,
            pair: Pair::new("BTC", "USDT"),
        };

        let mapping = manager.instrument_to_bybit(&instrument);
        assert_eq!(mapping.symbol.as_str(), "BTCUSDT");
        assert_eq!(mapping.category, BybitCategory::Spot);
    }

    #[test]
    fn test_instrument_to_bybit_futures() {
        let manager = BybitPairManager::new();
        let instrument = Instrument {
            instrument_type: InstrumentType::Futures,
            pair: Pair::new("BTC", "USDT"),
        };

        let mapping = manager.instrument_to_bybit(&instrument);
        assert_eq!(mapping.symbol.as_str(), "BTCUSDT");
        assert_eq!(mapping.category, BybitCategory::Linear);
    }

    #[test]
    fn test_bybit_to_instrument_spot() {
        let manager = BybitPairManager::new();
        let instrument = manager
            .bybit_to_instrument("BTCUSDT", "spot", "BTC", "USDT")
            .unwrap();

        assert_eq!(instrument.instrument_type, InstrumentType::Spot);
        assert_eq!(instrument.pair.base, "BTC");
        assert_eq!(instrument.pair.quote, "USDT");
    }

    #[test]
    fn test_bybit_to_instrument_linear() {
        let manager = BybitPairManager::new();
        let instrument = manager
            .bybit_to_instrument("BTCUSDT", "linear", "BTC", "USDT")
            .unwrap();

        assert_eq!(instrument.instrument_type, InstrumentType::Futures);
        assert_eq!(instrument.pair.base, "BTC");
        assert_eq!(instrument.pair.quote, "USDT");
    }

    #[test]
    fn test_bybit_to_instrument_invalid_category() {
        let manager = BybitPairManager::new();
        let result = manager.bybit_to_instrument("BTCUSDT", "invalid", "BTC", "USDT");
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_instrument() {
        let mut manager = BybitPairManager::new();
        let instrument = manager
            .cache_instrument("BTCUSDT", "spot", "BTC", "USDT", "Trading")
            .unwrap();

        assert_eq!(instrument.instrument_type, InstrumentType::Spot);
        assert!(manager.validate_instrument(&instrument));
    }

    #[test]
    fn test_validate_instrument_not_cached() {
        let manager = BybitPairManager::new();
        let instrument = Instrument {
            instrument_type: InstrumentType::Spot,
            pair: Pair::new("BTC", "USDT"),
        };

        assert!(!manager.validate_instrument(&instrument));
    }
}
