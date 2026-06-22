//! DeepBook V3 configuration (exchange). Chain-specific settings live in `DexEngineConfig.chain`.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::venue::VenueId;

use crate::config::{DexChain, DexEngineConfig};
use trading_core::venue::dex::DexError;

/// Sui network selector (chain layer — gRPC / indexer defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuiNetwork {
    Mainnet,
    Testnet,
    Devnet,
}

impl SuiNetwork {
    pub fn default_indexer_url(self) -> &'static str {
        match self {
            Self::Mainnet => "https://deepbook-indexer.mainnet.mystenlabs.com",
            Self::Testnet => "https://deepbook-indexer.testnet.mystenlabs.com",
            Self::Devnet => "https://deepbook-indexer.testnet.mystenlabs.com",
        }
    }

    pub fn default_grpc_url(self) -> &'static str {
        match self {
            Self::Mainnet => "https://fullnode.mainnet.sui.io",
            Self::Testnet => "https://fullnode.testnet.sui.io",
            Self::Devnet => "https://fullnode.devnet.sui.io",
        }
    }

    pub fn default_deepbook_package_id(self) -> &'static str {
        match self {
            Self::Mainnet => "0x337f4f4f6567fcd778d5454f27c16c70e2f274cc6377ea6249ddf491482ef497",
            Self::Testnet | Self::Devnet => {
                "0x22be4cade64bf2d02412c7e8d0e8beea2f78828b948118d46735315409371a3c"
            }
        }
    }

    pub fn default_registry_id(self) -> &'static str {
        match self {
            Self::Mainnet => "0xaf16199a2dff736e9f07a845f23c5da6df6f756eddb631aed9d24a93efc4549d",
            Self::Testnet | Self::Devnet => {
                "0x7c256edbda983a2cd6f946655f4bf3f00a41043993781f8674a7046e8c0e11d1"
            }
        }
    }
}

/// DeepBook exchange config embedded in `bot.json` → `dex_engine.config` (requires `chain: sui`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepBookConfig {
    pub network: SuiNetwork,
    #[serde(default, alias = "grpcUrl")]
    pub rpc_url: Option<SmolStr>,
    #[serde(default)]
    pub indexer_url: Option<SmolStr>,
    #[serde(default)]
    pub deepbook_package_id: Option<SmolStr>,
    #[serde(default)]
    pub registry_id: Option<SmolStr>,
    #[serde(default)]
    pub balance_manager_id: Option<SmolStr>,
    /// Sui wallet address for read-only balance polling (Layer 2+).
    #[serde(default)]
    pub wallet_address: Option<SmolStr>,
    pub pools: Vec<SmolStr>,
    #[serde(default)]
    pub pay_fees_in_deep: bool,
    #[serde(default = "default_private_key_env")]
    pub private_key_env: SmolStr,
    /// Indexer poll interval for market data (milliseconds).
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    /// Wallet balance poll interval for DEX reconcile stream (milliseconds).
    #[serde(default = "default_balance_poll_interval_ms")]
    pub balance_poll_interval_ms: u64,
}

fn default_private_key_env() -> SmolStr {
    SmolStr::new("SUI_PRIVATE_KEY")
}

fn default_poll_interval_ms() -> u64 {
    1000
}

fn default_balance_poll_interval_ms() -> u64 {
    5000
}

impl DeepBookConfig {
    pub fn from_dex_engine(config: &DexEngineConfig) -> Result<Self, DexError> {
        if config.venue != VenueId::DeepBook {
            return Err(DexError::InvalidConfig(
                format!(
                    "expected dex_engine.venue \"deepbook\", got {:?}",
                    config.venue
                )
                .into(),
            ));
        }
        if config.chain != DexChain::Sui {
            return Err(DexError::InvalidConfig(
                format!("expected dex_engine.chain \"sui\", got {:?}", config.chain).into(),
            ));
        }
        let parsed: Self = serde_json::from_value(config.config.clone())
            .map_err(|e| DexError::InvalidConfig(format!("invalid DeepBookConfig: {e}").into()))?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<(), DexError> {
        if self.pools.is_empty() {
            return Err(DexError::InvalidConfig(
                "dex_engine.config.pools must contain at least one pool name".into(),
            ));
        }
        Ok(())
    }

    /// gRPC full-node URL (reads and writes).
    pub fn grpc_url(&self) -> SmolStr {
        self.rpc_url
            .clone()
            .unwrap_or_else(|| SmolStr::new(self.network.default_grpc_url()))
    }

    pub fn deepbook_package_id(&self) -> Result<SmolStr, DexError> {
        Ok(self
            .deepbook_package_id
            .clone()
            .unwrap_or_else(|| SmolStr::new(self.network.default_deepbook_package_id())))
    }

    pub fn registry_id(&self) -> SmolStr {
        self.registry_id
            .clone()
            .unwrap_or_else(|| SmolStr::new(self.network.default_registry_id()))
    }

    pub fn indexer_url(&self) -> SmolStr {
        self.indexer_url
            .clone()
            .unwrap_or_else(|| SmolStr::new(self.network.default_indexer_url()))
    }

    /// Wallet address for Sui RPC balance reads, if configured.
    pub fn wallet_address(&self) -> Option<&str> {
        self.wallet_address.as_deref()
    }

    /// DeepBook `BalanceManager` object ID for indexer open-order reads.
    pub fn balance_manager_id(&self) -> Option<&str> {
        self.balance_manager_id.as_deref()
    }

    /// Required fields for live DEX trading (`--mode dex`).
    pub fn validate_for_trading(&self) -> Result<(), DexError> {
        if self.balance_manager_id().is_none() {
            return Err(DexError::InvalidConfig(
                "balanceManagerId required for DEX trading (run create_balance_manager first)"
                    .into(),
            ));
        }
        if self.wallet_address().is_none() {
            return Err(DexError::InvalidConfig(
                "walletAddress required for DEX trading".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_testnet_config() {
        let json = serde_json::json!({
            "network": "testnet",
            "pools": ["SUI_DBUSDC"],
            "payFeesInDeep": false
        });
        let cfg = DexEngineConfig {
            venue: VenueId::DeepBook,
            chain: DexChain::Sui,
            config: json,
        };
        let deepbook = DeepBookConfig::from_dex_engine(&cfg).unwrap();
        assert_eq!(deepbook.network, SuiNetwork::Testnet);
        assert_eq!(deepbook.pools.len(), 1);
        assert_eq!(
            deepbook.indexer_url(),
            "https://deepbook-indexer.testnet.mystenlabs.com"
        );
    }

    #[test]
    fn accepts_legacy_venue_alias_sui() {
        let cfg: DexEngineConfig = serde_json::from_value(serde_json::json!({
            "venue": "sui",
            "chain": "sui",
            "config": { "network": "testnet", "pools": ["SUI_DBUSDC"] }
        }))
        .unwrap();
        assert_eq!(cfg.venue, VenueId::DeepBook);
    }
}
