pub mod authenticator;
pub mod channels;
pub mod data;
pub mod error;
pub mod redis;
pub mod rest_client;
pub mod traits;
pub mod utils;
pub mod ws_client;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum Exchange {
    Binance,
}
