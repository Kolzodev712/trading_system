//! Live adapter for BTCPriceMALongFlatV1.
//!
//! Owns runtime lifecycle only. Signal math stays in `strategy_core`.
//! Not registered by the live strategy factory.

use super::prop_hold::FLAT_EPS;
use crate::cex::intent::{StrategyContext, StrategyIntent};
use crate::cex::state::Strategy;
use crate::cex::strategies::strategy_manager::StrategyManager;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::VecDeque;
use strategy_core::btc_price_ma_long_flat_v1::{
    on_candle, Direction, ModelState, Step, PRIMARY_LOOKBACK, STRATEGY_NAME, STRATEGY_VERSION,
};
use trading_core::{
    error::EngineError,
    types::{
        common::{Instrument, InstrumentType, Pair},
        exchange::Order,
        CompletedCandle,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    NotReady,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTarget {
    NotReady,
    NoChange,
    Quantity(Decimal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    Clear,
    UnexpectedShort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryFreshness {
    NoLiveTransition,
    Unconfigured,
    Fresh { age_secs: u64 },
    Stale { age_secs: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelTransition {
    pub from: Direction,
    pub to: Direction,
    pub start_time_ms: u64,
    pub candle_end_exclusive_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDecision {
    pub start_time_ms: u64,
    pub end_time_exclusive_ms: u64,
    pub close: Decimal,
    pub previous_direction: Direction,
    pub direction: Direction,
    pub readiness: Readiness,
    pub target: RuntimeTarget,
    pub reason: String,
    pub log_spread: Option<f64>,
    pub geometric_mean: Option<Decimal>,
}

#[derive(Debug, Clone)]
pub struct BtcPriceMaLongFlatV1Config {
    pub symbol: Pair,
    pub lookback: usize,
    pub instrument_type: InstrumentType,
    /// Required before a flat account may enter. `None` never allows a fresh long.
    pub max_entry_age_secs: Option<u64>,
}

impl BtcPriceMaLongFlatV1Config {
    pub fn primary(symbol: Pair, instrument_type: InstrumentType) -> Self {
        Self {
            symbol,
            lookback: PRIMARY_LOOKBACK,
            instrument_type,
            max_entry_age_secs: None,
        }
    }
}

pub struct BtcPriceMaLongFlatV1 {
    symbol: Pair,
    lookback: usize,
    instrument_type: InstrumentType,
    max_entry_age_secs: Option<u64>,
    closes: VecDeque<Decimal>,
    starts: VecDeque<u64>,
    last_processed: Option<CompletedCandle>,
    model: ModelState,
    last_decision: Option<RuntimeDecision>,
    last_transition: Option<ModelTransition>,
    last_price: Option<Decimal>,
    processed_candles: usize,
    model_updates: usize,
    /// Reconstruction finished. Distinct from candle readiness and from entry freshness.
    model_reconstructed: bool,
    last_recovery: RecoveryState,
}

impl BtcPriceMaLongFlatV1 {
    pub fn new(config: BtcPriceMaLongFlatV1Config) -> Result<Self, EngineError> {
        if config.lookback == 0 {
            return Err(EngineError::new_custom_error(
                "lookback must be positive".into(),
            ));
        }
        Ok(Self {
            symbol: config.symbol,
            lookback: config.lookback,
            instrument_type: config.instrument_type,
            max_entry_age_secs: config.max_entry_age_secs,
            closes: VecDeque::new(),
            starts: VecDeque::new(),
            last_processed: None,
            model: ModelState::new(),
            last_decision: None,
            last_transition: None,
            last_price: None,
            processed_candles: 0,
            model_updates: 0,
            model_reconstructed: false,
            last_recovery: RecoveryState::Clear,
        })
    }

    pub fn strategy_id(&self) -> String {
        StrategyManager::make_strategy_id(STRATEGY_NAME, &self.symbol)
    }

    pub fn strategy_version(&self) -> &'static str {
        STRATEGY_VERSION
    }

    pub fn lookback(&self) -> usize {
        self.lookback
    }

    pub fn model_state(&self) -> &ModelState {
        &self.model
    }

    pub fn last_decision(&self) -> Option<&RuntimeDecision> {
        self.last_decision.as_ref()
    }

    pub fn last_transition(&self) -> Option<&ModelTransition> {
        self.last_transition.as_ref()
    }

    pub fn processed_candles(&self) -> usize {
        self.processed_candles
    }

    pub fn model_updates(&self) -> usize {
        self.model_updates
    }

    pub fn is_model_reconstructed(&self) -> bool {
        self.model_reconstructed
    }

    /// Marks model reconstruction complete. Does not grant exchange permission.
    pub fn mark_model_reconstructed(&mut self) {
        self.model_reconstructed = true;
    }

    pub fn last_recovery(&self) -> RecoveryState {
        self.last_recovery
    }

    pub fn readiness(&self) -> Readiness {
        self.last_decision
            .as_ref()
            .map(|d| d.readiness)
            .unwrap_or(Readiness::NotReady)
    }

    pub fn entry_freshness(&self, now_ms: u64) -> EntryFreshness {
        let Some(transition) = self
            .last_transition
            .as_ref()
            .filter(|t| t.from == Direction::Flat && t.to == Direction::Long)
        else {
            return EntryFreshness::NoLiveTransition;
        };
        let Some(max_age) = self.max_entry_age_secs else {
            return EntryFreshness::Unconfigured;
        };
        let age_secs = now_ms.saturating_sub(transition.candle_end_exclusive_ms) / 1_000;
        if age_secs <= max_age {
            EntryFreshness::Fresh { age_secs }
        } else {
            EntryFreshness::Stale { age_secs }
        }
    }

    pub fn observe_completed_candle(
        &mut self,
        candle: &CompletedCandle,
    ) -> Result<Option<&RuntimeDecision>, EngineError> {
        if candle.instrument.pair != self.symbol {
            return Ok(self.last_decision.as_ref());
        }
        if let Some(prev) = &self.last_processed {
            if candle.start_time_ms < prev.start_time_ms {
                return Ok(self.last_decision.as_ref());
            }
            if prev.start_time_ms == candle.start_time_ms {
                return Ok(self.last_decision.as_ref());
            }
            if prev.end_time_exclusive_ms != candle.start_time_ms {
                return Err(EngineError::new_custom_error(
                    format!(
                        "gap before {}: expected start {}",
                        candle.start_time_ms, prev.end_time_exclusive_ms
                    )
                    .into(),
                ));
            }
        }

        self.closes.push_back(candle.close);
        self.starts.push_back(candle.start_time_ms);
        while self.closes.len() > self.lookback {
            self.closes.pop_front();
            self.starts.pop_front();
        }
        self.last_processed = Some(candle.clone());
        self.last_price = Some(candle.close);
        self.processed_candles += 1;

        let previous = self.model.direction;
        // N completed closes initialize the mean; the next candle is the first ready step (N+1).
        if self.processed_candles <= self.lookback {
            let have = self.processed_candles;
            self.last_decision = Some(RuntimeDecision {
                start_time_ms: candle.start_time_ms,
                end_time_exclusive_ms: candle.end_time_exclusive_ms,
                close: candle.close,
                previous_direction: previous,
                direction: previous,
                readiness: Readiness::NotReady,
                target: RuntimeTarget::NotReady,
                reason: format!("not ready: {have} of {} preceding candles", self.lookback),
                log_spread: None,
                geometric_mean: None,
            });
            return Ok(self.last_decision.as_ref());
        }

        let window: Vec<Decimal> = self.closes.iter().copied().collect();
        let (next, step) = on_candle(&self.model, self.lookback, &window)
            .map_err(|err| EngineError::new_custom_error(err.into()))?;
        self.model_updates += 1;
        let decision = match &step {
            Step::NotReady { have, need } => RuntimeDecision {
                start_time_ms: candle.start_time_ms,
                end_time_exclusive_ms: candle.end_time_exclusive_ms,
                close: candle.close,
                previous_direction: previous,
                direction: previous,
                readiness: Readiness::NotReady,
                target: RuntimeTarget::NotReady,
                reason: format!("not ready: {have} of {need} preceding candles"),
                log_spread: None,
                geometric_mean: None,
            },
            Step::NoChange {
                diagnostics,
                reason,
            } => RuntimeDecision {
                start_time_ms: candle.start_time_ms,
                end_time_exclusive_ms: candle.end_time_exclusive_ms,
                close: candle.close,
                previous_direction: previous,
                direction: next.direction,
                readiness: Readiness::Ready,
                target: RuntimeTarget::NoChange,
                reason: (*reason).to_string(),
                log_spread: Some(diagnostics.log_spread),
                geometric_mean: Some(diagnostics.geometric_mean),
            },
            Step::Transition {
                target_qty,
                diagnostics,
                reason,
            } => {
                self.last_transition = Some(ModelTransition {
                    from: previous,
                    to: next.direction,
                    start_time_ms: candle.start_time_ms,
                    candle_end_exclusive_ms: candle.end_time_exclusive_ms,
                });
                RuntimeDecision {
                    start_time_ms: candle.start_time_ms,
                    end_time_exclusive_ms: candle.end_time_exclusive_ms,
                    close: candle.close,
                    previous_direction: previous,
                    direction: next.direction,
                    readiness: Readiness::Ready,
                    target: RuntimeTarget::Quantity(*target_qty),
                    reason: (*reason).to_string(),
                    log_spread: Some(diagnostics.log_spread),
                    geometric_mean: Some(diagnostics.geometric_mean),
                }
            }
        };
        self.model = next;
        self.last_decision = Some(decision);
        Ok(self.last_decision.as_ref())
    }

    fn intent_for_account(
        &mut self,
        ctx: &StrategyContext<'_>,
    ) -> Result<StrategyIntent, EngineError> {
        let current = ctx.account.effective_position_qty(&self.symbol);
        if current < -FLAT_EPS {
            self.last_recovery = RecoveryState::UnexpectedShort;
            return Ok(StrategyIntent::hold(self.strategy_id()));
        }
        self.last_recovery = RecoveryState::Clear;

        if !self.model_reconstructed {
            return Ok(StrategyIntent::hold(self.strategy_id()));
        }
        if self.readiness() != Readiness::Ready {
            return Ok(StrategyIntent::hold(self.strategy_id()));
        }

        let instrument = Instrument::new(self.instrument_type.clone(), self.symbol.clone());
        let target = self.model.requested_qty;
        if target == Decimal::ZERO {
            if current.abs() <= FLAT_EPS {
                return Ok(StrategyIntent::hold(self.strategy_id()));
            }
            return Ok(StrategyIntent::target_position(
                self.strategy_id(),
                instrument,
                Decimal::ZERO,
            ));
        }

        if (current - target).abs() <= FLAT_EPS {
            return Ok(StrategyIntent::hold(self.strategy_id()));
        }

        if current.abs() <= FLAT_EPS {
            match self.entry_freshness(ctx.orderbook.timestamp) {
                EntryFreshness::Fresh { .. } => {
                    return Ok(StrategyIntent::target_position(
                        self.strategy_id(),
                        instrument,
                        target,
                    ));
                }
                EntryFreshness::Stale { .. }
                | EntryFreshness::NoLiveTransition
                | EntryFreshness::Unconfigured => {
                    return Ok(StrategyIntent::hold(self.strategy_id()));
                }
            }
        }

        Ok(StrategyIntent::target_position(
            self.strategy_id(),
            instrument,
            target,
        ))
    }
}

#[async_trait]
impl Strategy for BtcPriceMaLongFlatV1 {
    async fn evaluate(&mut self, ctx: &StrategyContext<'_>) -> Result<StrategyIntent, EngineError> {
        self.intent_for_account(ctx)
    }

    async fn on_execution_report(&mut self, _order: &Order) -> Result<(), EngineError> {
        Ok(())
    }

    fn get_last_price(&self) -> Option<Decimal> {
        self.last_price
    }

    async fn on_completed_candle(&mut self, candle: &CompletedCandle) -> Result<(), EngineError> {
        self.observe_completed_candle(candle)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::completed_candles::SequentialCandleIngest;
    use crate::cex::state::TradingState;
    use serde::Deserialize;
    use strategy_core::btc_price_ma_long_flat_v1::position_qty;
    use trading_core::types::{common::InstrumentType, market_data::Orderbook, DAILY_PERIOD_MS};

    fn pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn bar(start: u64, close: Decimal) -> CompletedCandle {
        CompletedCandle {
            venue: "bybit".into(),
            product: "linear".into(),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            interval: "D".into(),
            start_time_ms: start,
            end_time_exclusive_ms: start + DAILY_PERIOD_MS,
            open: close,
            high: close,
            low: close,
            close,
            volume: Decimal::ONE,
        }
    }

    fn book(now_ms: u64) -> Orderbook {
        Orderbook {
            pair: pair(),
            bids: vec![(Decimal::new(100, 0), Decimal::ONE)],
            asks: vec![(Decimal::new(101, 0), Decimal::ONE)],
            timestamp: now_ms,
            update_kind: Default::default(),
            sequence: None,
        }
    }

    fn adapter(lookback: usize, max_entry_age_secs: Option<u64>) -> BtcPriceMaLongFlatV1 {
        BtcPriceMaLongFlatV1::new(BtcPriceMaLongFlatV1Config {
            symbol: pair(),
            lookback,
            instrument_type: InstrumentType::Futures,
            max_entry_age_secs,
        })
        .unwrap()
    }

    #[test]
    fn missing_history_is_not_ready() {
        let mut strat = adapter(2, Some(86_400));
        let d = strat
            .observe_completed_candle(&bar(0, Decimal::new(100, 0)))
            .unwrap();
        assert_eq!(d.unwrap().readiness, Readiness::NotReady);
        assert_eq!(d.unwrap().target, RuntimeTarget::NotReady);
        assert_eq!(strat.model_updates(), 0);
    }

    #[test]
    fn duplicate_completed_candle_does_not_update_model_twice() {
        let mut strat = adapter(2, Some(86_400));
        let c0 = bar(0, Decimal::new(100, 0));
        let c1 = bar(DAILY_PERIOD_MS, Decimal::new(100, 0));
        let c2 = bar(DAILY_PERIOD_MS * 2, Decimal::new(110, 0));
        strat.observe_completed_candle(&c0).unwrap();
        strat.observe_completed_candle(&c1).unwrap();
        strat.observe_completed_candle(&c2).unwrap();
        let updates = strat.model_updates();
        let processed = strat.processed_candles();
        strat.observe_completed_candle(&c2).unwrap();
        assert_eq!(strat.model_updates(), updates);
        assert_eq!(strat.processed_candles(), processed);
    }

    #[test]
    fn forming_candle_never_reaches_adapter_via_ingest() {
        let mut ingest = SequentialCandleIngest::new();
        let mut strat = adapter(2, Some(86_400));
        let candle = bar(0, Decimal::new(100, 0));
        let now = candle.end_time_exclusive_ms - 1;
        let accepted = ingest.accept_all([candle], now);
        assert!(accepted.is_empty());
        for c in accepted {
            strat.observe_completed_candle(&c).unwrap();
        }
        assert_eq!(strat.processed_candles(), 0);
    }

    async fn evaluate_qty(
        strat: &mut BtcPriceMaLongFlatV1,
        account: &TradingState,
        now_ms: u64,
    ) -> StrategyIntent {
        let orderbook = book(now_ms);
        let ctx = StrategyContext {
            orderbook: &orderbook,
            account,
            instrument_type: InstrumentType::Futures,
        };
        strat.evaluate(&ctx).await.unwrap()
    }

    fn feed_long_setup(strat: &mut BtcPriceMaLongFlatV1) -> CompletedCandle {
        let c0 = bar(0, Decimal::new(100, 0));
        let c1 = bar(DAILY_PERIOD_MS, Decimal::new(100, 0));
        let c2 = bar(DAILY_PERIOD_MS * 2, Decimal::new(110, 0));
        strat.observe_completed_candle(&c0).unwrap();
        strat.observe_completed_candle(&c1).unwrap();
        strat.observe_completed_candle(&c2).unwrap();
        strat.mark_model_reconstructed();
        c2
    }

    #[tokio::test]
    async fn stale_historical_long_does_not_create_fresh_entry() {
        let mut strat = adapter(2, Some(86_400));
        let last = feed_long_setup(&mut strat);
        assert_eq!(strat.model_state().direction, Direction::Long);
        let account = TradingState::new();
        let now = last.end_time_exclusive_ms + 86_400_000 + 1_000;
        match evaluate_qty(&mut strat, &account, now).await {
            StrategyIntent::Hold { .. } => {}
            other => panic!("stale long must not enter, got {other:?}"),
        }
        assert!(matches!(
            strat.entry_freshness(now),
            EntryFreshness::Stale { .. }
        ));
    }

    #[tokio::test]
    async fn fresh_flat_to_long_is_eligible_to_enter() {
        let mut strat = adapter(2, Some(86_400));
        let last = feed_long_setup(&mut strat);
        let account = TradingState::new();
        let now = last.end_time_exclusive_ms + 1_000;
        match evaluate_qty(&mut strat, &account, now).await {
            StrategyIntent::TargetPosition { qty, .. } => {
                assert_eq!(qty, position_qty());
            }
            other => panic!("expected fresh entry, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn account_long_and_model_long_holds() {
        let mut strat = adapter(2, Some(86_400));
        let last = feed_long_setup(&mut strat);
        let mut account = TradingState::new();
        account.set_position(pair(), position_qty(), Decimal::new(110, 0));
        match evaluate_qty(&mut strat, &account, last.end_time_exclusive_ms).await {
            StrategyIntent::Hold { .. } => {}
            other => panic!("expected hold, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn account_long_and_model_flat_targets_zero() {
        let mut strat = adapter(2, Some(86_400));
        strat
            .observe_completed_candle(&bar(0, Decimal::new(110, 0)))
            .unwrap();
        strat
            .observe_completed_candle(&bar(DAILY_PERIOD_MS, Decimal::new(110, 0)))
            .unwrap();
        strat
            .observe_completed_candle(&bar(DAILY_PERIOD_MS * 2, Decimal::new(100, 0)))
            .unwrap();
        strat.mark_model_reconstructed();
        assert_eq!(strat.model_state().direction, Direction::Flat);
        let mut account = TradingState::new();
        account.set_position(pair(), position_qty(), Decimal::new(110, 0));
        match evaluate_qty(&mut strat, &account, DAILY_PERIOD_MS * 3).await {
            StrategyIntent::TargetPosition { qty, .. } => assert_eq!(qty, Decimal::ZERO),
            other => panic!("expected flatten, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unexpected_short_blocks_new_risk() {
        let mut strat = adapter(2, Some(86_400));
        let last = feed_long_setup(&mut strat);
        let mut account = TradingState::new();
        account.set_position(pair(), Decimal::new(-1, 3), Decimal::new(110, 0));
        match evaluate_qty(&mut strat, &account, last.end_time_exclusive_ms).await {
            StrategyIntent::Hold { .. } => {}
            other => panic!("short must not reverse, got {other:?}"),
        }
        assert_eq!(strat.last_recovery(), RecoveryState::UnexpectedShort);
    }

    #[tokio::test]
    async fn bootstrap_does_not_replay_entries() {
        let mut strat = adapter(2, Some(86_400));
        let last = bar(DAILY_PERIOD_MS * 2, Decimal::new(110, 0));
        strat
            .observe_completed_candle(&bar(0, Decimal::new(100, 0)))
            .unwrap();
        strat
            .observe_completed_candle(&bar(DAILY_PERIOD_MS, Decimal::new(100, 0)))
            .unwrap();
        strat.observe_completed_candle(&last).unwrap();
        assert!(!strat.is_model_reconstructed());
        let account = TradingState::new();
        match evaluate_qty(&mut strat, &account, last.end_time_exclusive_ms).await {
            StrategyIntent::Hold { .. } => {}
            other => panic!("bootstrap must not enter, got {other:?}"),
        }
    }

    #[derive(Debug, Deserialize)]
    struct DecisionLog {
        run: RunMetadata,
        decisions: Vec<DecisionRow>,
    }

    #[derive(Debug, Deserialize)]
    struct RunMetadata {
        entries: usize,
        exits: usize,
        final_direction: String,
    }

    #[derive(Debug, Deserialize)]
    struct DecisionRow {
        start_time_ms: u64,
        end_time_exclusive_ms: u64,
        close: Decimal,
        previous_direction: String,
        direction: String,
        readiness: String,
        target: FrozenTarget,
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum FrozenTarget {
        NotReady,
        NoChange,
        Quantity { quantity: Decimal },
    }

    fn direction_name(direction: Direction) -> &'static str {
        match direction {
            Direction::Flat => "flat",
            Direction::Long => "long",
        }
    }

    #[test]
    fn frozen_n28_runtime_transitions_match_offline() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/btc_price_ma_long_flat_v1/decisions-n28.json");
        let body = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("read {}: {err}", path.display());
        });
        let log: DecisionLog = serde_json::from_str(&body).expect("frozen decision log");
        let mut strat = adapter(PRIMARY_LOOKBACK, None);
        let mut entries = 0usize;
        let mut exits = 0usize;

        for row in &log.decisions {
            let candle = CompletedCandle {
                venue: "bybit".into(),
                product: "linear".into(),
                instrument: Instrument::new(InstrumentType::Futures, pair()),
                interval: "D".into(),
                start_time_ms: row.start_time_ms,
                end_time_exclusive_ms: row.end_time_exclusive_ms,
                open: row.close,
                high: row.close,
                low: row.close,
                close: row.close,
                volume: Decimal::ZERO,
            };
            let decision = strat
                .observe_completed_candle(&candle)
                .unwrap()
                .cloned()
                .expect("decision after observe");
            let expected_ready = row.readiness == "ready";
            assert_eq!(
                decision.readiness == Readiness::Ready,
                expected_ready,
                "readiness at {}",
                row.start_time_ms
            );
            assert_eq!(
                direction_name(decision.previous_direction),
                row.previous_direction
            );
            assert_eq!(direction_name(decision.direction), row.direction);
            match (&decision.target, &row.target) {
                (RuntimeTarget::NotReady, FrozenTarget::NotReady) => {}
                (RuntimeTarget::NoChange, FrozenTarget::NoChange) => {}
                (RuntimeTarget::Quantity(got), FrozenTarget::Quantity { quantity }) => {
                    assert_eq!(got, quantity);
                }
                (got, expected) => panic!(
                    "target mismatch at {}: {got:?} vs {expected:?}",
                    row.start_time_ms
                ),
            }
            if matches!(
                (&decision.target, decision.previous_direction, decision.direction),
                (RuntimeTarget::Quantity(qty), Direction::Flat, Direction::Long)
                    if *qty == position_qty()
            ) {
                entries += 1;
            }
            if matches!(
                (&decision.target, decision.previous_direction, decision.direction),
                (RuntimeTarget::Quantity(qty), Direction::Long, Direction::Flat)
                    if *qty == Decimal::ZERO
            ) {
                exits += 1;
            }
        }

        assert_eq!(entries, log.run.entries);
        assert_eq!(exits, log.run.exits);
        assert_eq!(
            direction_name(strat.model_state().direction),
            log.run.final_direction
        );
        strat.mark_model_reconstructed();
        assert!(strat.is_model_reconstructed());
    }
}
