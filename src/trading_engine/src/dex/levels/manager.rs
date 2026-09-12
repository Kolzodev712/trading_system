//! In-memory quote state and open-order reconciliation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dex_engine::chains::sui::{price_to_ticks, qty_to_lots, snap_price_to_tick};
use dex_engine::DeepBookPoolMeta;
use smol_str::SmolStr;
use tracing::warn;
use trading_core::types::common::Side;
use trading_core::venue::dex::types::{DexOpenOrder, ExecutionResult, TxStatus};
use trading_core::venue::dex::{DexError, TradeCommand};

use super::config::{DexLevelsDefaults, DexLevelsFile, DexQuoteConfig};
use super::quote::{DexQuoteRuntime, DexQuoteState};
use crate::dex::indicators::DexPriceConfig;
use crate::dex::indicators::{resolve_quote_price, DexIndicatorEngine};

/// Read-only quote row for monitoring / gRPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DexQuoteSnapshot {
    pub id: String,
    pub pool: String,
    pub side: String,
    pub price: String,
    /// Last resolved price when using dynamic / binding config (empty if unknown).
    pub resolved_price: String,
    pub quantity: String,
    pub enabled: bool,
    pub auto_repost: bool,
    pub state: String,
    pub order_id: String,
    pub last_signature: String,
}

const PLACE_FAILURE_BACKOFF_SECS: u64 = 30;

pub struct DexQuoteManager {
    defaults: DexLevelsDefaults,
    balance_manager_id: SmolStr,
    quotes: HashMap<String, DexQuoteRuntime>,
    /// Pending tx signature → quote id (for linking after confirm).
    pending_by_sig: HashMap<SmolStr, String>,
    pool_meta: HashMap<SmolStr, DeepBookPoolMeta>,
    indicators: Option<Arc<DexIndicatorEngine>>,
    last_resolved: HashMap<String, rust_decimal::Decimal>,
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
            indicators: None,
            last_resolved: HashMap::new(),
        })
    }

    pub fn with_indicators(mut self, engine: Arc<DexIndicatorEngine>) -> Self {
        self.indicators = Some(engine);
        self
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

    pub async fn build_place_command(&self, quote_id: &str) -> Result<TradeCommand, DexError> {
        let runtime = self.quotes.get(quote_id).ok_or_else(|| {
            DexError::InvalidConfig(format!("unknown quote id {quote_id}").into())
        })?;
        let resolved_price = self.resolve_price(quote_id, &runtime.config).await?;
        let pool_name = SmolStr::new(runtime.config.pool.as_str());
        let meta = self.pool_meta.get(&pool_name).ok_or_else(|| {
            DexError::InvalidConfig(
                format!("pool metadata missing for {}", runtime.config.pool).into(),
            )
        })?;
        qty_to_lots(runtime.config.quantity, meta)?;
        let price = snap_price_to_tick(resolved_price, meta)?;

        Ok(TradeCommand::PlaceLimitOrder {
            pool_id: meta.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            side: runtime.config.side.clone(),
            price,
            quantity: runtime.config.quantity,
            pay_fees_in_deep: runtime.config.pay_fees_in_deep(&self.defaults),
        })
    }

    async fn resolve_price(
        &self,
        quote_id: &str,
        config: &DexQuoteConfig,
    ) -> Result<rust_decimal::Decimal, DexError> {
        if let DexPriceConfig::Fixed(p) = &config.price {
            return Ok(*p);
        }
        let Some(engine) = &self.indicators else {
            return Err(DexError::InvalidConfig(
                "dynamic quote price requires DEX indicator engine".into(),
            ));
        };
        let orderbooks = engine.orderbooks().await;
        let values = engine.values().await;
        resolve_quote_price(
            quote_id,
            &config.pool,
            &config.price,
            engine.quote_pricing(),
            &orderbooks,
            &values,
        )
    }

    pub fn record_resolved_price(&mut self, quote_id: &str, price: rust_decimal::Decimal) {
        self.last_resolved.insert(quote_id.to_string(), price);
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
                    if let Some(order) = find_matching_order(
                        orders,
                        &runtime.config,
                        self.last_resolved.get(&runtime.config.id).copied(),
                    ) {
                        runtime.state = DexQuoteState::Placed {
                            order_id: order.order_id.clone(),
                        };
                        self.pending_by_sig.remove(&sig);
                    }
                }
                DexQuoteState::Placed { order_id } if !open_ids.contains(order_id.as_ref()) => {
                    if runtime.config.auto_repost(&self.defaults) {
                        let delay = Duration::from_secs(self.defaults.repost_delay_secs);
                        runtime.state = DexQuoteState::RepostScheduled { after: now + delay };
                    } else {
                        runtime.state = DexQuoteState::Idle;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn on_place_failed(&mut self, quote_id: &str, now: Instant) {
        let Some(runtime) = self.quotes.get_mut(quote_id) else {
            return;
        };
        if !runtime.config.enabled {
            return;
        }
        let delay = Duration::from_secs(
            self.defaults
                .repost_delay_secs
                .max(PLACE_FAILURE_BACKOFF_SECS),
        );
        runtime.state = DexQuoteState::RepostScheduled { after: now + delay };
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

    pub fn quote_snapshots(&self) -> Vec<DexQuoteSnapshot> {
        let mut out: Vec<DexQuoteSnapshot> = self
            .quotes
            .values()
            .map(|runtime| {
                runtime_to_snapshot(
                    runtime,
                    &self.defaults,
                    self.last_resolved.get(&runtime.config.id),
                )
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn toggle_quote(&mut self, quote_id: &str, enabled: bool) -> Result<(), String> {
        let runtime = self
            .quotes
            .get_mut(quote_id)
            .ok_or_else(|| format!("unknown quote id {quote_id}"))?;
        runtime.config.enabled = enabled;
        if enabled {
            if matches!(runtime.state, DexQuoteState::Disabled) {
                runtime.state = DexQuoteState::Idle;
            }
        } else {
            runtime.state = DexQuoteState::Disabled;
        }
        Ok(())
    }

    pub fn asset_labels(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for meta in self.pool_meta.values() {
            map.insert(
                meta.base_asset_id.to_string(),
                meta.base_asset_symbol.to_string(),
            );
            map.insert(
                meta.quote_asset_id.to_string(),
                meta.quote_asset_symbol.to_string(),
            );
            map.insert(
                meta.base_asset_symbol.to_string(),
                meta.base_asset_symbol.to_string(),
            );
            map.insert(
                meta.quote_asset_symbol.to_string(),
                meta.quote_asset_symbol.to_string(),
            );
        }
        map
    }

    pub fn pool_id_for_name(&self, pool_name: &str) -> Option<SmolStr> {
        self.pool_meta
            .get(&SmolStr::new(pool_name))
            .map(|m| m.pool_id.clone())
    }

    pub fn configured_pools(&self) -> Vec<(SmolStr, SmolStr)> {
        self.pool_meta
            .values()
            .map(|m| (m.pool_name.clone(), m.pool_id.clone()))
            .collect()
    }
}

fn runtime_to_snapshot(
    runtime: &DexQuoteRuntime,
    defaults: &DexLevelsDefaults,
    resolved: Option<&rust_decimal::Decimal>,
) -> DexQuoteSnapshot {
    let side = match runtime.config.side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    };
    let (order_id, last_signature) = match &runtime.state {
        DexQuoteState::Placed { order_id } => (order_id.to_string(), String::new()),
        DexQuoteState::PendingTx { signature } => (String::new(), signature.to_string()),
        _ => (String::new(), String::new()),
    };
    DexQuoteSnapshot {
        id: runtime.config.id.clone(),
        pool: runtime.config.pool.clone(),
        side: side.to_string(),
        price: format_price_spec(&runtime.config.price),
        resolved_price: resolved.map(|p| p.to_string()).unwrap_or_default(),
        quantity: runtime.config.quantity.to_string(),
        enabled: runtime.config.enabled,
        auto_repost: runtime.config.auto_repost(defaults),
        state: quote_state_label(&runtime.state),
        order_id,
        last_signature,
    }
}

fn quote_state_label(state: &DexQuoteState) -> String {
    match state {
        DexQuoteState::Idle => "idle".to_string(),
        DexQuoteState::PendingTx { .. } => "pending_tx".to_string(),
        DexQuoteState::Placed { .. } => "placed".to_string(),
        DexQuoteState::RepostScheduled { .. } => "repost_scheduled".to_string(),
        DexQuoteState::Disabled => "disabled".to_string(),
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
    if let DexPriceConfig::Fixed(price) = quote.price {
        price_to_ticks(price, meta).map_err(|e| format!("quote {}: {e}", quote.id))?;
    }
    Ok(())
}

fn find_matching_order<'a>(
    orders: &'a [DexOpenOrder],
    config: &DexQuoteConfig,
    resolved_price: Option<rust_decimal::Decimal>,
) -> Option<&'a DexOpenOrder> {
    let side_str = match config.side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    };
    let expected_price = match config.price {
        DexPriceConfig::Fixed(p) => Some(p),
        _ => resolved_price,
    };
    orders.iter().find(|o| {
        o.pool_name.as_ref() == config.pool
            && o.side.eq_ignore_ascii_case(side_str)
            && o.quantity == config.quantity
            && expected_price.map(|p| o.price == p).unwrap_or(true)
    })
}

fn format_price_spec(price: &DexPriceConfig) -> String {
    match price {
        DexPriceConfig::Fixed(d) => d.to_string(),
        DexPriceConfig::Binding => "Binding".to_string(),
        DexPriceConfig::Mid { offset_percent } => format!("Mid({offset_percent}%)"),
        DexPriceConfig::BestBid { offset } => format!("BestBid(-{offset})"),
        DexPriceConfig::BestAsk { offset } => format!("BestAsk(+{offset})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex::indicators::DexPriceConfig;
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
                    price: DexPriceConfig::Fixed(dec!(0.70)),
                    quantity: dec!(1.0),
                    enabled: true,
                    auto_repost: None,
                    pay_fees_in_deep: None,
                },
                DexQuoteConfig {
                    id: "ask_1".into(),
                    pool: "SUI_DBUSDC".into(),
                    side: Side::Sell,
                    price: DexPriceConfig::Fixed(dec!(0.75)),
                    quantity: dec!(1.0),
                    enabled: true,
                    auto_repost: None,
                    pay_fees_in_deep: None,
                },
            ],
            defaults: DexLevelsDefaults::default(),
        }
    }

    #[tokio::test]
    async fn builds_place_command_for_valid_quote() {
        let mut meta = HashMap::new();
        let m = fixture_meta();
        meta.insert(m.pool_name.clone(), m);
        let mgr = DexQuoteManager::from_file(sample_file(), SmolStr::new("0xbm"), meta).unwrap();
        let cmd = mgr.build_place_command("bid_1").await.unwrap();
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

    #[test]
    fn toggle_quote_updates_enabled_and_state() {
        let mut meta = HashMap::new();
        let m = fixture_meta();
        meta.insert(m.pool_name.clone(), m);
        let mut mgr =
            DexQuoteManager::from_file(sample_file(), SmolStr::new("0xbm"), meta).unwrap();
        mgr.toggle_quote("bid_1", false).unwrap();
        assert!(!mgr.quotes.get("bid_1").unwrap().config.enabled);
        assert!(matches!(
            mgr.quotes.get("bid_1").unwrap().state,
            DexQuoteState::Disabled
        ));
        mgr.toggle_quote("bid_1", true).unwrap();
        assert!(mgr.quotes.get("bid_1").unwrap().config.enabled);
        assert!(matches!(
            mgr.quotes.get("bid_1").unwrap().state,
            DexQuoteState::Idle
        ));
    }

    #[test]
    fn schedules_backoff_after_place_failure() {
        let mut meta = HashMap::new();
        let m = fixture_meta();
        meta.insert(m.pool_name.clone(), m);
        let mut mgr =
            DexQuoteManager::from_file(sample_file(), SmolStr::new("0xbm"), meta).unwrap();
        let now = Instant::now();
        mgr.on_place_failed("bid_1", now);
        assert!(matches!(
            mgr.quotes.get("bid_1").unwrap().state,
            DexQuoteState::RepostScheduled { .. }
        ));
        assert!(!mgr.quotes.get("bid_1").unwrap().needs_placement(now));
    }
}
