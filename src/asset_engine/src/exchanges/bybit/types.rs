//! Bybit instruments-info response types - minimal structure for trading rules only
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

/// Bybit instruments-info response wrapper
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitInstrumentsInfoResponse {
    pub ret_code: i32,
    pub ret_msg: String,
    pub result: BybitInstrumentsInfoResult,
    #[serde(default)]
    pub time: u64,
}

/// Bybit instruments-info result
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitInstrumentsInfoResult {
    pub category: String,
    #[serde(default)]
    pub next_page_cursor: String,
    pub list: Vec<BybitInstrumentInfo>,
}

/// Bybit instrument info - unified structure for spot and linear/inverse
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitInstrumentInfo {
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
    pub base_coin: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub quote_coin: SmolStr,

    // Spot-specific fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_trading: Option<SmolStr>,

    // Linear/Inverse-specific fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_type: Option<SmolStr>,

    // Common filters
    pub lot_size_filter: BybitLotSizeFilter,
    pub price_filter: BybitPriceFilter,

    // Optional fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_scale: Option<String>,
}

/// Bybit lot size filter
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitLotSizeFilter {
    // Spot fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub base_precision: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub quote_precision: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub min_order_amt: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub max_order_amt: Option<Decimal>,

    // Common fields
    #[serde(with = "rust_decimal::serde::str")]
    pub min_order_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_order_qty: Decimal,
    // qtyStep is only present for linear/inverse, not for spot
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub qty_step: Option<Decimal>,

    // Linear/Inverse fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub min_notional_value: Option<Decimal>,

    // Spot-specific
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub max_limit_order_qty: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub max_market_order_qty: Option<Decimal>,
}

/// Bybit price filter
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitPriceFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub min_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "rust_decimal::serde::str_option")]
    pub max_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub tick_size: Decimal,
}
