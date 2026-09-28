pub mod asset;
pub mod candle;
pub mod common;
pub mod exchange;
pub mod futures;
pub mod market_data;

pub use candle::{CompletedCandle, DAILY_PERIOD_MS};

// Re-export commonly used types from common
pub use common::*;

// Re-export exchange types (excluding Trade to avoid conflict with market_data::Trade)
pub use exchange::{
    AccountInfo,
    Balance,
    Order,
    OrderRequest,
    Position,
    // Note: exchange::Trade is NOT re-exported here to avoid conflict with market_data::Trade
    // Use exchange::Trade explicitly if needed
};

// Re-export market_data types
pub use market_data::{Orderbook, PublicTrade, StructuredOrderbook, StructuredPublicTrade, Trade};

// Re-export futures types
pub use futures::{
    FundingRate, FuturesBalance, FuturesOrderRequest, LeverageInfo, MarginInfo, MarginType,
    PositionMode, PositionSide, WorkingType,
};
