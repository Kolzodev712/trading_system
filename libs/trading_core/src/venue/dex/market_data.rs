//! DEX-specific market data payloads (chain-agnostic; Solana fills these in Proposal 01).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::command::LendingProtocol;

/// Normalized on-chain account health (e.g. lending position nearing liquidation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DexAccountHealth {
    pub protocol: LendingProtocol,
    pub account: SmolStr,
    pub health_factor: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collateral_value: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liability_value: Option<Decimal>,
}
