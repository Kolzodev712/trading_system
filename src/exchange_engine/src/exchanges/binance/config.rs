use serde::{Deserialize, Serialize};
use trading_core::secret::SecretString;
use trading_core::types::common::Pair;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceConfig {
    /// Trading pairs to use (centralized pair configuration)
    pub pairs: Vec<Pair>,
    pub rest: BinanceCredentials,
    pub ws: BinanceCredentials,
    pub limit: Option<u64>,
    pub base_delay: Option<u64>,
    pub max_delay: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCredentials {
    pub api_key: SecretString,
    pub secret_key: SecretString,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use trading_core::types::common::Pair;

    #[test]
    fn test_binance_credentials_serde() {
        let creds = BinanceCredentials {
            api_key: SecretString::new("test_api_key"),
            secret_key: SecretString::new("test_secret_key"),
        };

        let serialized = serde_json::to_string(&creds).expect("Serialization failed");
        assert!(serialized.contains("test_api_key"));
        assert!(serialized.contains("test_secret_key"));

        let deserialized: BinanceCredentials =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.api_key.expose(), creds.api_key.expose());
        assert_eq!(deserialized.secret_key.expose(), creds.secret_key.expose());
    }

    #[test]
    fn test_binance_config_serde() {
        let config = BinanceConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            rest: BinanceCredentials {
                api_key: SecretString::new("rest_api_key"),
                secret_key: SecretString::new("rest_secret_key"),
            },
            ws: BinanceCredentials {
                api_key: SecretString::new("ws_api_key"),
                secret_key: SecretString::new("ws_secret_key"),
            },
            limit: Some(500),
            base_delay: Some(1000),
            max_delay: Some(60000),
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
        assert!(serialized.contains("rest_api_key"));
        assert!(serialized.contains("ws_api_key"));
        assert!(serialized.contains("500"));
        assert!(serialized.contains("1000"));
        assert!(serialized.contains("60000"));

        let deserialized: BinanceConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(
            deserialized.rest.api_key.expose(),
            config.rest.api_key.expose()
        );
        assert_eq!(
            deserialized.rest.secret_key.expose(),
            config.rest.secret_key.expose()
        );
        assert_eq!(deserialized.ws.api_key.expose(), config.ws.api_key.expose());
        assert_eq!(
            deserialized.ws.secret_key.expose(),
            config.ws.secret_key.expose()
        );
        assert_eq!(deserialized.pairs, config.pairs);
        assert_eq!(deserialized.limit, config.limit);
    }

    #[test]
    fn test_binance_config_optional_fields_none() {
        let config = BinanceConfig {
            pairs: vec![Pair::new("ETH", "USDT")],
            rest: BinanceCredentials {
                api_key: SecretString::new("rest_api_key"),
                secret_key: SecretString::new("rest_secret_key"),
            },
            ws: BinanceCredentials {
                api_key: SecretString::new("ws_api_key"),
                secret_key: SecretString::new("ws_secret_key"),
            },
            limit: None,
            base_delay: None,
            max_delay: None,
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");

        let deserialized: BinanceConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(
            deserialized.rest.api_key.expose(),
            config.rest.api_key.expose()
        );
        assert_eq!(
            deserialized.rest.secret_key.expose(),
            config.rest.secret_key.expose()
        );
        assert_eq!(deserialized.ws.api_key.expose(), config.ws.api_key.expose());
        assert_eq!(
            deserialized.ws.secret_key.expose(),
            config.ws.secret_key.expose()
        );
        assert_eq!(deserialized.pairs, config.pairs);
    }
}
