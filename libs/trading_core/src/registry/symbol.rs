//! Symbol registry for efficient symbol-to-pair and pair-to-symbol conversions
//!
//! This registry maintains bidirectional mappings between trading symbols (e.g., "BTCUSDT")
//! and trading pairs (e.g., Pair { base: "BTC", quote: "USDT" }).
//!
//! This is necessary because exchanges provide symbols as strings, but our system
//! works with Pair types for clearer base/quote separation.

use crate::types::common::{Pair, Symbol};
use std::collections::HashMap;

/// Registry for symbol-to-pair and pair-to-symbol mappings
///
/// Provides efficient bidirectional lookups between Symbol and Pair.
/// This is a shared registry that can be used across multiple components.
#[derive(Debug, Clone)]
pub struct SymbolRegistry {
    /// Symbol -> Pair mapping
    symbol_to_pair: HashMap<Symbol, Pair>,

    /// Pair -> Symbol mapping (reverse lookup)
    pair_to_symbol: HashMap<Pair, Symbol>,
}

impl SymbolRegistry {
    /// Create a new empty symbol registry
    pub fn new() -> Self {
        Self {
            symbol_to_pair: HashMap::new(),
            pair_to_symbol: HashMap::new(),
        }
    }

    /// Create a symbol registry from a list of pairs
    ///
    /// # Arguments
    /// * `pairs` - Vector of pairs to register. Symbols will be generated using `pair.convert_to_smolstr()`
    pub fn from_pairs(pairs: Vec<Pair>) -> Self {
        let mut registry = Self::new();
        for pair in pairs {
            let symbol = Symbol::from(pair.convert_to_smolstr());
            registry.register(symbol, pair);
        }
        registry
    }

    /// Register a symbol-to-pair mapping (bidirectional)
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "BTCUSDT")
    /// * `pair` - The trading pair (e.g., Pair { base: "BTC", quote: "USDT" })
    pub fn register(&mut self, symbol: Symbol, pair: Pair) {
        self.symbol_to_pair.insert(symbol.clone(), pair.clone());
        self.pair_to_symbol.insert(pair, symbol);
    }

    /// Get the Pair for a given Symbol
    ///
    /// # Returns
    /// * `Some(Pair)` if the symbol is registered
    /// * `None` if the symbol is not found
    pub fn get_pair(&self, symbol: &Symbol) -> Option<&Pair> {
        self.symbol_to_pair.get(symbol)
    }

    /// Get the Symbol for a given Pair
    ///
    /// # Returns
    /// * `Some(Symbol)` if the pair is registered
    /// * `None` if the pair is not found
    pub fn get_symbol(&self, pair: &Pair) -> Option<&Symbol> {
        self.pair_to_symbol.get(pair)
    }

    /// Check if a symbol is registered
    pub fn has_symbol(&self, symbol: &Symbol) -> bool {
        self.symbol_to_pair.contains_key(symbol)
    }

    /// Check if a pair is registered
    pub fn has_pair(&self, pair: &Pair) -> bool {
        self.pair_to_symbol.contains_key(pair)
    }

    /// Get all registered pairs
    pub fn all_pairs(&self) -> Vec<&Pair> {
        self.pair_to_symbol.keys().collect()
    }

    /// Get all registered symbols
    pub fn all_symbols(&self) -> Vec<&Symbol> {
        self.symbol_to_pair.keys().collect()
    }

    /// Get the number of registered mappings
    pub fn len(&self) -> usize {
        self.symbol_to_pair.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.symbol_to_pair.is_empty()
    }
}

impl Default for SymbolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_registry_new() {
        let registry = SymbolRegistry::new();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_symbol_registry_register() {
        let mut registry = SymbolRegistry::new();
        let pair = Pair::new("BTC", "USDT");
        let symbol = Symbol::from("BTCUSDT");

        registry.register(symbol.clone(), pair.clone());

        assert_eq!(registry.get_pair(&symbol), Some(&pair));
        assert_eq!(registry.get_symbol(&pair), Some(&symbol));
    }

    #[test]
    fn test_symbol_registry_from_pairs() {
        let pairs = vec![Pair::new("BTC", "USDT"), Pair::new("ETH", "USDT")];
        let registry = SymbolRegistry::from_pairs(pairs);

        assert_eq!(registry.len(), 2);

        let btc_pair = Pair::new("BTC", "USDT");
        let btc_symbol = Symbol::from("BTCUSDT");
        assert_eq!(registry.get_symbol(&btc_pair), Some(&btc_symbol));
        assert_eq!(registry.get_pair(&btc_symbol), Some(&btc_pair));
    }

    #[test]
    fn test_symbol_registry_bidirectional() {
        let mut registry = SymbolRegistry::new();
        let pair = Pair::new("ETH", "BTC");
        let symbol = Symbol::from("ETHBTC");

        registry.register(symbol.clone(), pair.clone());

        // Test both directions
        assert_eq!(registry.get_pair(&symbol), Some(&pair));
        assert_eq!(registry.get_symbol(&pair), Some(&symbol));

        // Test has methods
        assert!(registry.has_symbol(&symbol));
        assert!(registry.has_pair(&pair));
    }

    #[test]
    fn test_symbol_registry_not_found() {
        let registry = SymbolRegistry::new();
        let symbol = Symbol::from("UNKNOWN");
        let pair = Pair::new("UNK", "NOWN");

        assert_eq!(registry.get_pair(&symbol), None);
        assert_eq!(registry.get_symbol(&pair), None);
        assert!(!registry.has_symbol(&symbol));
        assert!(!registry.has_pair(&pair));
    }
}
