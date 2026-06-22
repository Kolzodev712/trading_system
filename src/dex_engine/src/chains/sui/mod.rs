//! DeepBook V3 on Sui chain (`dex_engine` chain implementation).

pub mod config;
pub mod execution;
pub mod grpc;
pub mod indexer;
pub mod precision;
pub mod provider;
pub mod test_fixtures;
pub mod types;

pub use config::{DeepBookConfig, SuiNetwork};
pub use indexer::{DeepBookIndexerClient, IndexerTrade, OrderbookSnapshot};
pub use precision::{
    lots_to_base_qty, price_to_ticks, qty_to_lots, ticks_to_price, validate_min_lots,
};
pub use provider::DeepBookVenueProvider;
pub use types::{pool_name_to_pair, DeepBookPoolMeta};
