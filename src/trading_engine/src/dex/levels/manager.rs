//! In-memory quote state and open-order reconciliation.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use dex_engine::chains::sui::{price_to_ticks, qty_to_lots};
use dex_engine::DeepBookPoolMeta;
use smol_str::SmolStr;
use tracing::warn;
use trading_core::types::common::Side;
use trading_core::venue::dex::types::{DexOpenOrder, ExecutionResult, TxStatus};
use trading_core::venue::dex::{DexError, TradeCommand};

use super::config::{DexLevelsDefaults, DexLevelsFile, DexQuoteConfig};
use super::quote::{DexQuoteRuntime, DexQuoteState};

pub struct DexQuoteManager {
    defaults: DexLevelsDefaults,
    balance_manager_id: SmolStr,
    quotes: HashMap<String, DexQuoteRuntime>,
    /// Pending tx signature → quote id (for linking after confirm).
    pending_by_sig: HashMap<SmolStr, String>,
    pool_meta: HashMap<SmolStr, DeepBookPoolMeta>,
}

impl DexQuoteManager {
    pub fn from_file(
        file: DexLevelsFile,
        balance_manager_id: SmolStr,
        pool_meta: HashMap<SmolStr, DeepBookPoolMeta>,
    ) -> Result<Self, String> {
        let defaults = file.defaults.clone();
        let mut quotes = HashMap::new();
        for quote in file.quotes {
            validate_quote(&quote, &pool_meta)?;
            quotes.insert(quote.id.clone(), DexQuoteRuntime::new(quote));
        }
        Ok(Self {
            defaults,
            balance_manager_id,
            quotes,
            pending_by_sig: HashMap::new(),
            pool_meta,
        })
    }

    pub fn defaults(&self) -> &DexLevelsDefaults {
        &self.defaults
    }

    pub fn quote_ids_needing_placement(&self, now: Instant) -> Vec<String> {
        self.quotes
            .iter()
            .filter(|(_, q)| q.needs_placement(now))
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn build_place_command(&self, quote_id: &str) -> Result<TradeCommand, DexError> {
        let runtime = self.quotes.get(quote_id).ok_or_else(|| {
            DexError::InvalidConfig(format!("unknown quote id {quote_id}").into())
        })?;
        let pool_name = SmolStr::new(runtime.config.pool.as_str());
        let meta = self.pool_meta.get(&pool_name).ok_or_else(|| {
            DexError::InvalidConfig(
                format!("pool metadata missing for {}", runtime.config.pool).into(),
            )
        })?;
        qty_to_lots(runtime.config.quantity, meta)?;
        price_to_ticks(runtime.config.price, meta)?;

        Ok(TradeCommand::PlaceLimitOrder {
            pool_id: meta.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            side: runtime.config.side.clone(),
            price: runtime.config.price,
            quantity: runtime.config.quantity,
            pay_fees_in_deep: runtime.config.pay_fees_in_deep(&self.defaults),
        })
    }

    pub fn on_place_submitted(&mut self, quote_id: &str, signature: SmolStr) {
        if let Some(runtime) = self.quotes.get_mut(quote_id) {
            runtime.state = DexQuoteState::PendingTx {
                signature: signature.clone(),
            };
            self.pending_by_sig.insert(signature, quote_id.to_string());
        }
    }

    pub fn on_execution_result(&mut self, result: &ExecutionResult) {
        let Some(quote_id) = self.pending_by_sig.remove(&result.signature) else {
            return;
        };
        let Some(runtime) = self.quotes.get_mut(&quote_id) else {
            return;
        };
        match result.status {
            TxStatus::Pending => {}
            TxStatus::Confirmed => {
                // Order id linked on next open-orders update.
                runtime.state = DexQuoteState::PendingTx {
                    signature: result.signature.clone(),
                };
            }
            TxStatus::Failed => {
                warn!(
                    quote_id = %quote_id,
                    error = ?result.error,
                    "DEX quote placement failed"
                );
                runtime.state = DexQuoteState::Idle;
            }
        }
    }

    /// Reconcile quote states against indexer open orders; schedule repost when orders vanish.
    pub fn on_open_orders(&mut self, orders: &[DexOpenOrder], now: Instant) {
        let open_ids: HashSet<&str> = orders.iter().map(|o| o.order_id.as_ref()).collect();

        for runtime in self.quotes.values_mut() {
            if !runtime.config.enabled {
                runtime.state = DexQuoteState::Disabled;
                continue;
            }

            match &runtime.state {
                DexQuoteState::PendingTx { signature } => {
                    let sig = signature.clone();
                    if let Some(order) = find_matching_order(orders, &runtime.config) {
                        runtime.state = DexQuoteState::Placed {
                            order_id: order.order_id.clone(),
                        };
                        self.pending_by_sig.remove(&sig);
                    }
                }
                DexQuoteState::Placed { order_id } => {
                    if !open_ids.contains(order_id.as_ref()) {
                        if runtime.config.auto_repost(&self.defaults) {
                            let delay = Duration::from_secs(self.defaults.repost_delay_secs);
                            runtime.state = DexQuoteState::RepostScheduled { after: now + delay };
                        } else {
                            runtime.state = DexQuoteState::Idle;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn active_quote_count(&self) -> usize {
        self.quotes
            .values()
            .filter(|q| matches!(q.state, DexQuoteState::Placed { .. }))
            .count()
    }

    pub fn quote_count(&self) -> usize {
        self.quotes.len()
    }
}

fn validate_quote(
    quote: &DexQuoteConfig,
    pool_meta: &HashMap<SmolStr, DeepBookPoolMeta>,
) -> Result<(), String> {
    let pool_name = SmolStr::new(quote.pool.as_str());
    let meta = pool_meta
        .get(&pool_name)
        .ok_or_else(|| format!("pool {} not in dex_engine config / indexer", quote.pool))?;
    qty_to_lots(quote.quantity, meta).map_err(|e| format!("quote {}: {e}", quote.id))?;
    price_to_ticks(quote.price, meta).map_err(|e| format!("quote {}: {e}", quote.id))?;
    Ok(())
}

fn find_matching_order<'a>(
    orders: &'a [DexOpenOrder],
    config: &DexQuoteConfig,
) -> Option<&'a DexOpenOrder> {
    let side_str = match config.side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    };
    orders.iter().find(|o| {
        o.pool_name.as_ref() == config.pool
            && o.side.eq_ignore_ascii_case(side_str)
            && o.price == config.price
            && o.quantity == config.quantity
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;

    fn fixture_meta() -> DeepBookPoolMeta {
        DeepBookPoolMeta {
            pool_id: SmolStr::new("0xpool"),
            pool_name: SmolStr::new("SUI_DBUSDC"),
            base_asset_id: SmolStr::new("sui"),
            base_asset_decimals: 9,
            base_asset_symbol: SmolStr::new("SUI"),
            base_asset_name: SmolStr::new("Sui"),
            quote_asset_id: SmolStr::new("dbusdc"),
            quote_asset_decimals: 6,
            quote_asset_symbol: SmolStr::new("DBUSDC"),
            quote_asset_name: SmolStr::new("Deepbook USDC"),
            min_size: 1_000_000_000,
            lot_size: 100_000_000,
            tick_size: 10,
        }
    }

    fn sample_file() -> DexLevelsFile {
        DexLevelsFile {
            quotes: vec![
                DexQuoteConfig {
                    id: "bid_1".into(),
                    pool: "SUI_DBUSDC".into(),
                    side: Side::Buy,
                    price: dec!(0.70),
                    quantity: dec!(1.0),
                    enabled: true,
                    auto_repost: None,
                    pay_fees_in_deep: None,
                },
                DexQuoteConfig {
                    id: "ask_1".into(),
                    pool: "SUI_DBUSDC".into(),
                    side: Side::Sell,
                    price: dec!(0.75),
                    quantity: dec!(1.0),
                    enabled: true,
                    auto_repost: None,
                    pay_fees_in_deep: None,
                },
            ],
            defaults: DexLevelsDefaults::default(),
        }
    }

    #[test]
    fn builds_place_command_for_valid_quote() {
        let mut meta = HashMap::new();
        let m = fixture_meta();
        meta.insert(m.pool_name.clone(), m);
        let mgr = DexQuoteManager::from_file(sample_file(), SmolStr::new("0xbm"), meta).unwrap();
        let cmd = mgr.build_place_command("bid_1").unwrap();
        assert!(matches!(cmd, TradeCommand::PlaceLimitOrder { .. }));
    }

    #[test]
    fn schedules_repost_when_placed_order_disappears() {
        let mut meta = HashMap::new();
        let m = fixture_meta();
        meta.insert(m.pool_name.clone(), m);
        let mut mgr =
            DexQuoteManager::from_file(sample_file(), SmolStr::new("0xbm"), meta).unwrap();
        let now = Instant::now();
        mgr.quotes.get_mut("bid_1").unwrap().state = DexQuoteState::Placed {
            order_id: SmolStr::new("order_1"),
        };
        mgr.on_open_orders(&[], now);
        assert!(matches!(
            mgr.quotes.get("bid_1").unwrap().state,
            DexQuoteState::RepostScheduled { .. }
        ));
    }
}
