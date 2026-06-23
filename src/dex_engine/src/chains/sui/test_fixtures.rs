//! Shared DeepBook test fixtures.

use smol_str::SmolStr;

use super::types::DeepBookPoolMeta;

/// Testnet `SUI_DBUSDC` pool metadata (representative indexer shape).
pub fn sui_dbusdc_pool() -> DeepBookPoolMeta {
    DeepBookPoolMeta {
        pool_id: SmolStr::new("0x8a2fb544a74241576c019c7a1521d5c4344b0270a043b89884a78e7c8f2568e"),
        pool_name: SmolStr::new("SUI_DBUSDC"),
        base_asset_id: SmolStr::new("0x2::sui::SUI"),
        base_asset_decimals: 9,
        base_asset_symbol: SmolStr::new("SUI"),
        base_asset_name: SmolStr::new("Sui"),
        quote_asset_id: SmolStr::new(
            "0xf7157867268c83e8b62a035a0dba80a2f89d991a7293069a9775d5d8d6552c8::DBUSDC::DBUSDC",
        ),
        quote_asset_decimals: 6,
        quote_asset_symbol: SmolStr::new("DBUSDC"),
        quote_asset_name: SmolStr::new("DeepBook USDC"),
        min_size: 1_000_000,
        lot_size: 1_000_000,
        tick_size: 10_000,
    }
}
