//! DEX venue state primitives.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::time::Instant;

use super::command::TradeCommand;

/// On-chain transaction lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
}

/// Submitted transaction tracked until confirmation.
#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub signature: SmolStr,
    pub command: TradeCommand,
    pub status: TxStatus,
    pub submitted_at: Instant,
}

/// Result of an executed DEX command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub signature: SmolStr,
    pub status: TxStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SmolStr>,
}

/// Wallet balances keyed by mint symbol or address.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DexWalletBalances {
    pub balances: HashMap<SmolStr, Decimal>,
}
