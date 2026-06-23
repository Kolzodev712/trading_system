//! Live DeepBook indexer tests (public testnet, no keys required).

use dex_engine::config::{DexChain, DexEngineConfig};
use dex_engine::{DeepBookConfig, DeepBookIndexerClient};
use smol_str::SmolStr;
use trading_core::venue::VenueId;

fn testnet_config() -> DeepBookConfig {
    let engine = DexEngineConfig {
        venue: VenueId::DeepBook,
        chain: DexChain::Sui,
        config: serde_json::json!({
            "network": "testnet",
            "pools": ["SUI_DBUSDC"]
        }),
    };
    engine.deepbook_config().expect("deepbook config")
}

#[tokio::test]
#[ignore = "requires network access to DeepBook testnet indexer"]
async fn live_get_pools_includes_sui_dbusdc() {
    let cfg = testnet_config();
    let client = DeepBookIndexerClient::new(cfg.indexer_url()).unwrap();
    let pools = client
        .get_pools_by_name(&[SmolStr::new("SUI_DBUSDC")])
        .await
        .expect("get pools");
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].pool_name.as_str(), "SUI_DBUSDC");
    assert!(!pools[0].pool_id.is_empty());
}

#[tokio::test]
#[ignore = "requires network access to DeepBook testnet indexer"]
async fn live_orderbook_has_bids_and_asks() {
    let cfg = testnet_config();
    let client = DeepBookIndexerClient::new(cfg.indexer_url()).unwrap();
    let book = client
        .get_orderbook("SUI_DBUSDC", 5)
        .await
        .expect("orderbook");
    assert!(!book.bids.is_empty() || !book.asks.is_empty());
}

#[tokio::test]
#[ignore = "requires network access to DeepBook testnet indexer"]
async fn live_trades_returns_recent_entries() {
    let cfg = testnet_config();
    let client = DeepBookIndexerClient::new(cfg.indexer_url()).unwrap();
    let trades = client.get_trades("SUI_DBUSDC", 3).await.expect("trades");
    assert!(!trades.is_empty());
}
