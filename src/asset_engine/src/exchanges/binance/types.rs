//! Binance exchangeInfo response types - minimal structure for trading rules only
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

/// Binance exchangeInfo response - only fields we need
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceExchangeInfo {
    pub server_time: u64,
    pub rate_limits: Vec<BinanceRateLimit>,
    pub symbols: Vec<BinanceSymbolInfo>,
}

/// Binance rate limit
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceRateLimit {
    pub rate_limit_type: String,
    pub interval: String,
    pub interval_num: u64,
    pub limit: u64,
}

/// Binance symbol information - only fields we need for trading rules
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceSymbolInfo {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub status: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub base_asset: SmolStr,
    pub base_asset_precision: u8,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub quote_asset: SmolStr,
    pub quote_precision: u8,
    pub is_spot_trading_allowed: bool,
    pub filters: Vec<BinanceFilter>,
}

/// Binance filter - only the filters we need for trading rules
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "filterType")]
pub enum BinanceFilter {
    #[serde(rename = "PRICE_FILTER")]
    PriceFilter {
        #[serde(rename = "minPrice", with = "rust_decimal::serde::str_option")]
        min_price: Option<Decimal>,
        #[serde(rename = "maxPrice", with = "rust_decimal::serde::str_option")]
        max_price: Option<Decimal>,
        #[serde(rename = "tickSize", with = "rust_decimal::serde::str_option")]
        tick_size: Option<Decimal>,
    },
    #[serde(rename = "LOT_SIZE")]
    LotSize {
        #[serde(rename = "minQty", with = "rust_decimal::serde::str")]
        min_qty: Decimal,
        #[serde(rename = "maxQty", with = "rust_decimal::serde::str")]
        max_qty: Decimal,
        #[serde(rename = "stepSize", with = "rust_decimal::serde::str")]
        step_size: Decimal,
    },
    #[serde(rename = "MIN_NOTIONAL")]
    MinNotional {
        #[serde(rename = "minNotional", with = "rust_decimal::serde::str")]
        min_notional: Decimal,
    },
    #[serde(other)]
    Other,
}
