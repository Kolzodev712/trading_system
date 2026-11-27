//! DEX config file validation (offline).

use dex_engine::DeepBookConfig;
use runner::config::BotConfig;
use rust_decimal_macros::dec;
use std::path::PathBuf;
use trading_core::types::common::Pair;
use trading_core::types::market_data::StructuredOrderbook;
use trading_engine::dex::indicators::{DexIndicatorEngine, DexIndicatorsFile};
use trading_engine::dex::DexLevelsFile;

fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config")
}

fn dex_config_dir() -> PathBuf {
    config_dir().join("dex")
}

#[test]
fn bot_dex_example_loads_and_validates_for_dex_mode() {
    let path = dex_config_dir().join("bot.json");
    let cfg = BotConfig::load_from_file(path.to_str().unwrap()).expect("load config/dex/bot.json");
    cfg.validate().expect("validate");
    cfg.validate_for_runner("dex")
        .expect("validate_for_runner dex");
}

#[test]
fn dex_levels_example_loads_and_parses() {
    let path = dex_config_dir().join("levels.json");
    let file = DexLevelsFile::load_from_file(&path).expect("load config/dex/levels.json");
    assert!(!file.quotes.is_empty());
    assert!(file.quotes.iter().all(|q| !q.id.is_empty()));
}

#[test]
fn dex_indicators_example_loads_and_parses() {
    let path = dex_config_dir().join("indicators.json");
    let file = DexIndicatorsFile::load_from_file(&path).expect("load config/dex/indicators.json");
    assert!(!file.indicators.is_empty());
    assert!(!file.quote_pricing.is_empty());
}

#[tokio::test]
async fn dex_levels_example_builds_place_commands_with_pool_meta() {
    use dex_engine::chains::sui::test_fixtures::sui_dbusdc_pool;
    use std::collections::HashMap;

    let levels_path = dex_config_dir().join("levels.json");
    let indicators_path = dex_config_dir().join("indicators.json");
    let file = DexLevelsFile::load_from_file(&levels_path).expect("load config/dex/levels.json");
    let indicators = DexIndicatorsFile::load_from_file(&indicators_path)
        .expect("load config/dex/indicators.json");
    let engine = DexIndicatorEngine::from_file(indicators).expect("indicator engine");

    let book = StructuredOrderbook {
        pair: Pair::new("SUI", "DBUSDC"),
        bids: vec![(dec!(3.0), dec!(1.0))],
        asks: vec![(dec!(3.2), dec!(1.0))],
        timestamp: 1,
        update_kind: Default::default(),
        sequence: None,
    };
    engine.on_orderbook(&book).await;

    let pool = sui_dbusdc_pool();
    let mut pool_meta = HashMap::new();
    pool_meta.insert(pool.pool_name.clone(), pool);

    let manager = trading_engine::dex::levels::DexQuoteManager::from_file(
        file,
        smol_str::SmolStr::new(
            "0x71483f6105efc83797dd937d9d15be8f8e8eab124bceeed41484c440368595fc",
        ),
        pool_meta,
    )
    .expect("DexQuoteManager")
    .with_indicators(engine);

    for quote_id in ["bid_1", "ask_1"] {
        manager
            .build_place_command(quote_id)
            .await
            .unwrap_or_else(|e| panic!("build_place_command {quote_id}: {e}"));
    }
}

#[test]
fn bot_dex_example_deepbook_has_trading_fields() {
    let path = dex_config_dir().join("bot.json");
    let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
    let dex = cfg.dex_engine.as_ref().unwrap();
    let deepbook = DeepBookConfig::from_dex_engine(dex).unwrap();
    deepbook.validate_for_trading().unwrap();
    assert!(deepbook.private_key.is_some());
}
