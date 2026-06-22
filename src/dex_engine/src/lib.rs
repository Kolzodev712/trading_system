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
    use trading_core::config::ChannelConfig;
    use trading_core::venue::dex::provider::VenueProvider;
    use trading_core::venue::VenueId;

    #[tokio::test]
    async fn mock_provider_bootstraps() {
        let provider = chains::mock::MockVenueProvider;
        assert_eq!(provider.venue_id(), VenueId::Solana);
        let wallet = provider.bootstrap_state().await.unwrap();
        assert!(wallet.wallet.balances.is_empty());
        assert!(wallet.open_orders.is_empty());
    }

    #[tokio::test]
    async fn run_starts_runtime_with_mock_solana() {
        let config = DexEngineConfig {
            venue: VenueId::Solana,
            chain: config::DexChain::Solana,
            config: serde_json::json!({}),
        };
        let (_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        let runtime = run(config, ChannelConfig::default(), shutdown_rx, 0, false)
            .await
            .unwrap();
        assert!(runtime
            .state_manager
            .state()
            .read()
            .wallet
            .balances
            .is_empty());
    }

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
