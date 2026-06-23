pub mod chains;
pub mod config;
pub mod monitoring;
pub mod run;
pub mod runtime;
pub mod state;

pub use config::DexEngineConfig;
pub use run::run;
pub use runtime::DexEngineRuntime;

pub use chains::sui::execution::DeepBookSetup;
pub use chains::sui::indexer::IndexerOrderbook;
pub use chains::sui::{
    pool_name_to_pair, DeepBookConfig, DeepBookIndexerClient, DeepBookPoolMeta,
    DeepBookVenueProvider, IndexerTrade, OrderbookSnapshot,
};

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::venue::VenueId;

    #[test]
    fn deepbook_config_deserializes() {
        let config = DexEngineConfig {
            venue: VenueId::DeepBook,
            chain: config::DexChain::Sui,
            config: serde_json::json!({
                "network": "testnet",
                "pools": ["SUI_DBUSDC"]
            }),
        };
        let deepbook = config.deepbook_config().unwrap();
        assert_eq!(deepbook.pools.len(), 1);
    }
}
