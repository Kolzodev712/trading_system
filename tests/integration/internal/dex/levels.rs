//! Dex levels → PlaceLimitOrder command wiring (offline).

use dex_engine::chains::sui::test_fixtures::sui_dbusdc_pool;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::str::FromStr;
use trading_core::types::common::Side;
use trading_core::venue::dex::TradeCommand;
use trading_engine::dex::levels::DexQuoteConfig;
use trading_engine::dex::DexLevelsFile;

#[test]
fn dex_levels_place_command_from_fixture_pool_meta() {
    let pool = sui_dbusdc_pool();
    let mut pool_meta = HashMap::new();
    pool_meta.insert(pool.pool_name.clone(), pool.clone());

    let file = DexLevelsFile {
        defaults: Default::default(),
        quotes: vec![DexQuoteConfig {
            id: "bid-1".to_string(),
            pool: "SUI_DBUSDC".to_string(),
            side: Side::Buy,
            price: Decimal::from_str("3.71").unwrap(),
            quantity: Decimal::from_str("10").unwrap(),
            enabled: true,
            auto_repost: None,
            pay_fees_in_deep: None,
        }],
    };

    let manager = trading_engine::dex::levels::DexQuoteManager::from_file(
        file,
        SmolStr::new("0x71483f6105efc83797dd937d9d15be8f8e8eab124bceeed41484c440368595fc"),
        pool_meta,
    )
    .unwrap();

    let cmd = manager.build_place_command("bid-1").unwrap();
    match cmd {
        TradeCommand::PlaceLimitOrder {
            pool_id,
            balance_manager_id,
            side,
            ..
        } => {
            assert_eq!(pool_id.as_str(), pool.pool_id.as_str());
            assert!(balance_manager_id.as_str().starts_with("0x7148"));
            assert_eq!(side, Side::Buy);
        }
        other => panic!("expected PlaceLimitOrder, got {other:?}"),
    }
}
