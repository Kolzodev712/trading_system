use super::strategy::TradingState;
use core_logic::{data::base_data::Pair, error::EngineError, traits::TradingClient};
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Reconciles internal trading state with exchange balances
#[derive(Clone)]
pub(crate) struct PositionReconciler {
    trading_client: Arc<RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>>,
}

impl PositionReconciler {
    pub fn new(
        trading_client: Arc<
            RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
        >,
    ) -> Self {
        Self { trading_client }
    }

    /// Reconcile positions by fetching balances from exchange and comparing
    pub async fn reconcile(
        &self,
        state: &mut TradingState,
        symbol: &Pair,
    ) -> Result<ReconciliationResult, EngineError> {
        // Fetch balances from exchange
        let balances = self.trading_client.read().await.fetch_balance().await?;

        // Find the balance for our trading pair
        // Balance uses Symbol (e.g., "BTCUSDT"), we need to match against base/quote
        // For now, we'll look for balances that match the base currency symbol
        // This is a simplified approach - in production, you'd want proper symbol mapping
        let base_balance = balances
            .iter()
            .find(|b| {
                // Try to match by checking if symbol contains base currency
                let balance_symbol = b.symbol.as_ref();
                balance_symbol.starts_with(symbol.base.as_str())
                    || balance_symbol == symbol.base.as_str()
            })
            .map(|b| b.free + b.used)
            .unwrap_or(Decimal::ZERO);

        // Get internal position
        let internal_position = state.get_position(symbol);

        let internal_qty = internal_position
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);

        // Compare
        let discrepancy = base_balance - internal_qty;

        if discrepancy.abs() > Decimal::new(1, 6) {
            // Significant discrepancy (more than 0.000001)
            warn!(
                "Position discrepancy detected for {symbol}: internal={internal_qty}, exchange={base_balance}, diff={discrepancy}",
                symbol = symbol,
                internal_qty = internal_qty,
                base_balance = base_balance,
                discrepancy = discrepancy
            );

            // Update internal state to match exchange
            if base_balance > Decimal::ZERO {
                // We have a position on exchange but not internally
                // Use a reasonable average price (could be improved)
                let avg_price = internal_position
                    .map(|p| p.avg_price)
                    .unwrap_or(Decimal::ZERO);

                state.update_position(symbol.clone(), base_balance, avg_price);
                info!(
                    "Updated internal position to match exchange: {base_balance} {base}",
                    base = symbol.base
                );
            } else if base_balance == Decimal::ZERO && internal_qty > Decimal::ZERO {
                // Exchange shows no position but we think we have one
                // Clear internal position
                let symbol_to_remove = symbol.clone();
                state.positions.remove(&symbol_to_remove);
                info!("Cleared internal position (exchange shows zero)");
            }

            Ok(ReconciliationResult {
                symbol: symbol.clone(),
                internal_quantity: internal_qty,
                exchange_quantity: base_balance,
                discrepancy,
                reconciled: true,
            })
        } else {
            // Positions match (within tolerance)
            Ok(ReconciliationResult {
                symbol: symbol.clone(),
                internal_quantity: internal_qty,
                exchange_quantity: base_balance,
                discrepancy,
                reconciled: false,
            })
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are returned for potential future use/logging
pub(crate) struct ReconciliationResult {
    pub symbol: Pair,
    pub internal_quantity: Decimal,
    pub exchange_quantity: Decimal,
    pub discrepancy: Decimal,
    pub reconciled: bool,
}
