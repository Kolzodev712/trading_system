//! Integration Tests
//!
//! ## Component integration (`integration/internal/`)
//! In-memory tests of manager, risk, catalog, and related methods. They do **not**
//! launch a runner, exchange, journal, or ledger. Do not treat a green result here
//! as Paper/Demo/live end-to-end evidence. Included in `just test-fast`.
//!
//! ## External connection probes (`integration/external/`)
//! Mostly `#[ignore]`. `just test-integration` and `just ci` do **not** execute
//! ignored cases. The helper must not build a Paper process client or a Bybit
//! mainnet trading client.
//!
//! Paper process workflows: `just test-paper` / `just ci-local`.
//! Bybit Demo: `just test-demo-readonly` and `just test-demo-mutating` (opt-in).

mod integration {
    pub mod external;
    pub mod helpers;
    pub mod internal;
}

pub use integration::*;
