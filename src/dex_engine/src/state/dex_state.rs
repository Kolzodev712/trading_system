use std::collections::HashMap;

use smol_str::SmolStr;
use trading_core::venue::dex::{
    types::{DexWalletBalances, PendingTransaction},
    TradeCommand,
};

/// In-memory DEX venue state (wallet + pending transactions).
#[derive(Debug, Default)]
pub struct DexState {
    pub wallet: DexWalletBalances,
    pub pending: HashMap<SmolStr, PendingTransaction>,
    pub last_command: Option<TradeCommand>,
}

impl DexState {
    pub fn new() -> Self {
        Self::default()
    }
}
