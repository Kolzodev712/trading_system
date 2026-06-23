//! Maintains pool orderbooks and configured indicator values from market data.

use std::collections::HashMap;
use std::sync::Arc;

use smol_str::SmolStr;
use tokio::sync::RwLock;
use tracing::debug;
use trading_core::channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver};
use trading_core::types::market_data::StructuredOrderbook;

use super::config::{DexIndicatorDef, DexIndicatorsFile, DexQuotePricingRegistry};
use super::types::{DexIndicatorKind, DexIndicatorValue};

#[derive(Debug)]
pub struct DexIndicatorEngine {
    orderbooks: RwLock<HashMap<SmolStr, StructuredOrderbook>>,
    values: RwLock<HashMap<SmolStr, DexIndicatorValue>>,
    defs: Vec<DexIndicatorDef>,
    quote_pricing: DexQuotePricingRegistry,
}

impl DexIndicatorEngine {
    pub fn from_file(file: DexIndicatorsFile) -> Result<Arc<Self>, String> {
        let quote_pricing = DexQuotePricingRegistry::from_rules(file.quote_pricing)?;
        Ok(Arc::new(Self {
            orderbooks: RwLock::new(HashMap::new()),
            values: RwLock::new(HashMap::new()),
            defs: file.indicators,
            quote_pricing,
        }))
    }

    pub fn quote_pricing(&self) -> &DexQuotePricingRegistry {
        &self.quote_pricing
    }

    pub async fn orderbooks(&self) -> HashMap<SmolStr, StructuredOrderbook> {
        self.orderbooks.read().await.clone()
    }

    pub async fn values(&self) -> HashMap<SmolStr, DexIndicatorValue> {
        self.values.read().await.clone()
    }

    pub async fn on_orderbook(&self, orderbook: StructuredOrderbook) {
        let pool = pool_name_from_orderbook(&orderbook);
        {
            let mut books = self.orderbooks.write().await;
            books.insert(SmolStr::new(&pool), orderbook.clone());
        }
        self.refresh_indicators_for_pool(&pool, &orderbook).await;
    }

    async fn refresh_indicators_for_pool(&self, pool: &str, orderbook: &StructuredOrderbook) {
        let timestamp = orderbook.timestamp;
        let mut values = self.values.write().await;
        for def in &self.defs {
            if def.pool != pool {
                continue;
            }
            let Some(value) = compute_kind(def.kind, orderbook) else {
                continue;
            };
            let id = SmolStr::new(&def.id);
            values.insert(
                id.clone(),
                DexIndicatorValue {
                    indicator_id: id,
                    name: SmolStr::new(kind_label(def.kind)),
                    pool: SmolStr::new(pool),
                    value,
                    timestamp,
                },
            );
        }
    }

    /// Export for gRPC / CLI (`pair` field carries pool name).
    pub async fn monitoring_values(&self) -> Vec<DexIndicatorValue> {
        let mut out: Vec<DexIndicatorValue> = self.values.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.indicator_id.cmp(&b.indicator_id));
        out
    }
}

pub fn spawn_market_data_listener(
    engine: Arc<DexIndicatorEngine>,
    mut market_data_rx: MarketDataUpdateReceiver,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                update = market_data_rx.recv() => {
                    match update {
                        Ok(MarketDataUpdate::Orderbook { orderbook, .. }) => {
                            engine.on_orderbook(orderbook).await;
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            debug!("DEX indicator engine lagged {n} market data message(s)");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = shutdown_rx.recv() => break,
            }
        }
    })
}

fn compute_kind(
    kind: DexIndicatorKind,
    orderbook: &StructuredOrderbook,
) -> Option<rust_decimal::Decimal> {
    match kind {
        DexIndicatorKind::Mid => orderbook.mid_price(),
        DexIndicatorKind::BestBid => orderbook.best_bid_price(),
        DexIndicatorKind::BestAsk => orderbook.best_ask_price(),
        DexIndicatorKind::Spread => {
            let bid = orderbook.best_bid_price()?;
            let ask = orderbook.best_ask_price()?;
            Some(ask - bid)
        }
    }
}

fn kind_label(kind: DexIndicatorKind) -> &'static str {
    match kind {
        DexIndicatorKind::Mid => "Mid",
        DexIndicatorKind::BestBid => "BestBid",
        DexIndicatorKind::BestAsk => "BestAsk",
        DexIndicatorKind::Spread => "Spread",
    }
}

fn pool_name_from_orderbook(orderbook: &StructuredOrderbook) -> String {
    format!("{}_{}", orderbook.pair.base, orderbook.pair.quote)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use trading_core::types::common::Pair;
    use trading_core::types::market_data::StructuredOrderbook;

    #[tokio::test]
    async fn updates_mid_indicator_from_orderbook() {
        let file = DexIndicatorsFile {
            indicators: vec![DexIndicatorDef {
                id: "mid_sui".into(),
                pool: "SUI_DBUSDC".into(),
                kind: DexIndicatorKind::Mid,
            }],
            quote_pricing: vec![],
        };
        let engine = DexIndicatorEngine::from_file(file).unwrap();
        let book = StructuredOrderbook {
            pair: Pair::new("SUI", "DBUSDC"),
            bids: vec![(dec!(3.0), dec!(1.0))],
            asks: vec![(dec!(3.2), dec!(1.0))],
            timestamp: 42,
        };
        engine.on_orderbook(book).await;
        let values = engine.values().await;
        assert_eq!(
            values.get(&SmolStr::new("mid_sui")).unwrap().value,
            dec!(3.1)
        );
    }
}
