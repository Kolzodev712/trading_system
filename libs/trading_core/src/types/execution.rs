//! Runtime execution permission. Venue/environment is a separate dimension.

use serde::{Deserialize, Serialize};

/// Whether this process may submit exchange orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Evaluate and record. Never place, amend, or cancel exchange orders.
    Shadow,
    /// Orders may be submitted when trading is enabled.
    #[default]
    Exchange,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::Exchange => "exchange",
        }
    }

    /// Shadow never submits. Exchange submits only when the operator enable flag is on.
    pub fn permits_exchange_orders(self, trading_enabled: bool) -> bool {
        matches!(self, Self::Exchange) && trading_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_never_permits_orders() {
        assert!(!ExecutionMode::Shadow.permits_exchange_orders(true));
        assert!(!ExecutionMode::Shadow.permits_exchange_orders(false));
    }

    #[test]
    fn exchange_requires_enable_flag() {
        assert!(ExecutionMode::Exchange.permits_exchange_orders(true));
        assert!(!ExecutionMode::Exchange.permits_exchange_orders(false));
    }
}
