//! External connection probes.
//!
//! Tests that verify external connections to exchange APIs work correctly.
//! Almost all cases are `#[ignore]` and are **not** run by `just test-integration`
//! or `just ci`. A green default CI run is not evidence that these probes passed.
//!
//! Do not use `helpers::external::ExchangeClientBuilder` for Paper process
//! workflows or mutating Bybit (that builder refuses Paper and Bybit mainnet).

pub mod asset_engine;
pub mod dex_engine;
pub mod exchange_engine;
pub mod market_data_engine;
