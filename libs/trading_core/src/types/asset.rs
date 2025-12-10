//! Asset data types for exchange market information
use crate::types::common::Pair;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;

/// Complete market data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeMarketData {
    /// Symbol information by pair
    pub symbol_info: HashMap<Pair, SymbolInfo>,

    /// Trading rules by pair
    pub trading_rules: HashMap<Pair, TradingRules>,

    /// Current market prices by pair
    pub current_prices: HashMap<Pair, Decimal>,

    /// Exchange limits (rate limits, position limits, etc.)
    pub exchange_limits: ExchangeLimits,

    /// Last updated timestamp
    pub last_updated: u64,
}

/// Symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Trading pair
    pub pair: Pair,

    /// Symbol status
    pub status: SymbolStatus,

    /// Base asset
    pub base_asset: SmolStr,

    /// Quote asset
    pub quote_asset: SmolStr,

    /// Symbol name on exchange (e.g., "BTCUSDT")
    pub exchange_symbol: SmolStr,

    /// Whether trading is allowed
    pub trading_allowed: bool,

    /// Whether the symbol is active
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolStatus {
    PreTrading,
    Trading,
    PostTrading,
    EndOfDay,
    Halt,
    AuctionMatch,
    Break,
    PendingTrading,
}

/// Trading rules for a symbol
/// All fields are Option to allow fallback to config if exchange data is unavailable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingRules {
    /// Trading pair
    pub pair: Pair,

    /// Minimum order size (in base currency)
    pub min_order_size: Option<Decimal>,

    /// Maximum order size (in base currency, if applicable)
    pub max_order_size: Option<Decimal>,

    /// Step size for order quantities (LOT_SIZE filter)
    /// Quantities must be rounded to a multiple of this
    pub step_size: Option<Decimal>,

    /// Minimum price
    pub min_price: Option<Decimal>,

    /// Maximum price
    pub max_price: Option<Decimal>,

    /// Tick size for order prices (PRICE_FILTER)
    /// Prices must be rounded to a multiple of this
    pub tick_size: Option<Decimal>,

    /// Minimum notional value (price * quantity) in quote currency
    pub min_notional: Option<Decimal>,

    /// Maximum notional value (if applicable)
    pub max_notional: Option<Decimal>,

    /// Price precision (decimal places)
    pub price_precision: Option<u8>,

    /// Quantity precision (decimal places)
    pub quantity_precision: Option<u8>,
}

/// Exchange limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeLimits {
    /// Rate limits
    pub rate_limits: Vec<RateLimit>,

    /// Maximum position size (if applicable)
    pub max_position_size: Option<Decimal>,

    /// Maximum number of open orders
    pub max_open_orders: Option<usize>,

    /// Other exchange-specific limits
    pub other_limits: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub rate_limit_type: RateLimitType,
    pub interval: String, // e.g., "MINUTE", "SECOND"
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RateLimitType {
    RequestWeight,
    Orders,
    RawRequests,
}
