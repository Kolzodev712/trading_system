//! DEX execution commands (separate from CEX `OrderRequest`).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Lending / perp protocol identifier on a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LendingProtocol {
    Marginfi,
    Drift,
    Kamino,
}

/// DEX-only commands routed through `dex_engine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TradeCommand {
    Liquidate {
        protocol: LendingProtocol,
        /// Target account (base58 or opaque id until chain types land in 02).
        account: SmolStr,
        max_amount: Decimal,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bonus_bps: Option<u32>,
    },
    Swap {
        input_mint: SmolStr,
        output_mint: SmolStr,
        amount: Decimal,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slippage_bps: Option<u32>,
    },
}
