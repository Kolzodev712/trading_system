use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceConfig {
    pub api_key: String,
    pub secret_key: String,
}
