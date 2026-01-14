//! Bybit exchange configuration

use serde::{Deserialize, Serialize};
use trading_core::secret::SecretString;
use trading_core::types::common::Pair;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BybitConfig {
    /// Trading pairs to use (centralized pair configuration)
    pub pairs: Vec<Pair>,
    /// API credentials (shared for both spot and futures)
    pub credentials: BybitCredentials,
    /// Testnet mode
    #[serde(default)]
    pub testnet: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BybitCredentials {
    pub api_key: SecretString,
    pub api_secret: SecretString,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_bybit_credentials_serde() {
        let creds = BybitCredentials {
            api_key: SecretString::new("test_api_key"),
            api_secret: SecretString::new("test_api_secret"),
        };

        let serialized = serde_json::to_string(&creds).expect("Serialization failed");
        assert!(serialized.contains("test_api_key"));
        assert!(serialized.contains("test_api_secret"));

        let deserialized: BybitCredentials =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.api_key.expose(), creds.api_key.expose());
        assert_eq!(deserialized.api_secret.expose(), creds.api_secret.expose());
    }

    #[test]
    fn test_bybit_config_serde() {
        let config = BybitConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            credentials: BybitCredentials {
                api_key: SecretString::new("api_key"),
                api_secret: SecretString::new("api_secret"),
            },
            testnet: false,
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));

        let deserialized: BybitConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(
            deserialized.credentials.api_key.expose(),
            config.credentials.api_key.expose()
        );
        assert_eq!(deserialized.pairs, config.pairs);
        assert_eq!(deserialized.testnet, config.testnet);
    }
}
