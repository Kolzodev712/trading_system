pub mod chains;
pub mod config;
pub mod run;
pub mod runtime;
pub mod state;

pub use config::DexEngineConfig;
pub use run::run;
pub use runtime::DexEngineRuntime;

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
        assert!(wallet.balances.is_empty());
    }

    #[tokio::test]
    async fn run_starts_runtime() {
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
}
