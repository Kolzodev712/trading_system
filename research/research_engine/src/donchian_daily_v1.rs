//! Deterministic DonchianDailyV1 model.
//!
//! This module does not submit orders, read the clock, or fetch candles.
//! A caller supplies one completed candle at a time. Channels use only the
//! candles already stored, which end at t−1. Candle t is appended after the decision.

use rust_decimal::Decimal;
use std::collections::VecDeque;

pub const STRATEGY_NAME: &str = "DonchianDailyV1";
pub const STRATEGY_VERSION: &str = "1";
pub const DAILY_INTERVAL: &str = "D";
pub const DAY_MS: u64 = 86_400_000;
pub const ENTRY_LOOKBACK: usize = 20;
pub const EXIT_LOOKBACK: usize = 10;

pub mod aux;
pub mod backtest;
pub mod offline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfig {
    pub venue: String,
    pub product: String,
    pub instrument: String,
    pub interval: String,
    pub entry_lookback: usize,
    pub exit_lookback: usize,
    pub position_qty: Decimal,
    pub allow_short: bool,
}

impl ModelConfig {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.venue.is_empty() || self.product.is_empty() || self.instrument.is_empty() {
            return Err(ModelError::Config("identity fields must be non-empty"));
        }
        if self.interval != DAILY_INTERVAL {
            return Err(ModelError::Config("V1 interval must be D"));
        }
        if self.entry_lookback < 2
            || self.exit_lookback < 1
            || self.exit_lookback > self.entry_lookback
        {
            return Err(ModelError::Config(
                "lookbacks must satisfy 1 <= exit <= entry and entry >= 2",
            ));
        }
        if self.position_qty <= Decimal::ZERO {
            return Err(ModelError::Config("position quantity must be positive"));
        }
        Ok(())
    }
}

/// Identity of one completed candle. Interval end is exclusive: `[start, end)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandleId {
    pub venue: String,
    pub product: String,
    pub instrument: String,
    pub interval: String,
    pub start_time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedCandle {
    pub id: CandleId,
    pub end_time_exclusive_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    /// Exchange finality. A forming candle is rejected and does not change state.
    pub confirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Flat,
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendReason {
    Gap,
    Correction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredCandle {
    start_time_ms: u64,
    end_time_exclusive_ms: u64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelState {
    pub strategy_id: &'static str,
    pub version: &'static str,
    pub direction: Direction,
    /// `None` means no decision has been produced. That is not a flatten target.
    pub target_qty: Option<Decimal>,
    pub last_start_ms: Option<u64>,
    pub entries_suspended: Option<SuspendReason>,
    window: VecDeque<StoredCandle>,
}

impl ModelState {
    pub fn new() -> Self {
        Self {
            strategy_id: STRATEGY_NAME,
            version: STRATEGY_VERSION,
            direction: Direction::Flat,
            target_qty: None,
            last_start_ms: None,
            entries_suspended: None,
            window: VecDeque::new(),
        }
    }

    pub fn preceding_len(&self) -> usize {
        self.window.len()
    }
}

impl Default for ModelState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    pub close: Decimal,
    pub entry_high: Option<Decimal>,
    pub entry_low: Option<Decimal>,
    pub exit_high: Option<Decimal>,
    pub exit_low: Option<Decimal>,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    IgnoredDuplicate,
    IgnoredForming,
    NotReady {
        preceding: usize,
        required: usize,
    },
    Suspended {
        reason: SuspendReason,
    },
    NoChange {
        diagnostics: Diagnostics,
    },
    Transition {
        from: Direction,
        to: Direction,
        target_qty: Decimal,
        diagnostics: Diagnostics,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    Config(&'static str),
    Candle(&'static str),
}

/// Clear an entry suspension after an operator rebuilds history. Does not replay missed trades.
#[allow(dead_code)]
pub fn rearm(state: &mut ModelState) {
    state.entries_suspended = None;
}

pub fn on_candle(
    state: &ModelState,
    config: &ModelConfig,
    candle: &ClosedCandle,
) -> Result<(ModelState, Step), ModelError> {
    config.validate()?;
    if !candle.confirmed {
        return Ok((state.clone(), Step::IgnoredForming));
    }
    validate_candle(config, candle)?;

    if let Some(last_start) = state.last_start_ms {
        if candle.id.start_time_ms == last_start {
            return duplicate_or_correction(state, candle);
        }
    }

    if let Some(reason) = state.entries_suspended {
        return Ok((state.clone(), Step::Suspended { reason }));
    }

    if let Some(last) = state.window.back() {
        if candle.id.start_time_ms != last.end_time_exclusive_ms {
            let mut next = state.clone();
            next.entries_suspended = Some(SuspendReason::Gap);
            return Ok((
                next,
                Step::Suspended {
                    reason: SuspendReason::Gap,
                },
            ));
        }
    }

    let mut next = state.clone();
    let preceding = next.window.len();
    if preceding < config.entry_lookback {
        append(&mut next, candle, config.entry_lookback);
        return Ok((
            next,
            Step::NotReady {
                preceding,
                required: config.entry_lookback,
            },
        ));
    }

    let (entry_high, entry_low) = channel(&next.window, config.entry_lookback)
        .expect("entry window is available once preceding >= entry_lookback");
    let (exit_high, exit_low) = channel(&next.window, config.exit_lookback)
        .expect("exit lookback is not larger than entry lookback");
    let close = candle.close;
    let diagnostics = |reason: &'static str| Diagnostics {
        close,
        entry_high: Some(entry_high),
        entry_low: Some(entry_low),
        exit_high: Some(exit_high),
        exit_low: Some(exit_low),
        reason,
    };

    let from = next.direction;
    let outcome = match from {
        Direction::Flat => {
            if close > entry_high {
                next.direction = Direction::Long;
                next.target_qty = Some(config.position_qty);
                Step::Transition {
                    from,
                    to: Direction::Long,
                    target_qty: config.position_qty,
                    diagnostics: diagnostics("close above entry high"),
                }
            } else if close < entry_low && config.allow_short {
                next.direction = Direction::Short;
                next.target_qty = Some(-config.position_qty);
                Step::Transition {
                    from,
                    to: Direction::Short,
                    target_qty: -config.position_qty,
                    diagnostics: diagnostics("close below entry low"),
                }
            } else {
                Step::NoChange {
                    diagnostics: diagnostics(if close < entry_low {
                        "short entry blocked"
                    } else {
                        "no entry"
                    }),
                }
            }
        }
        Direction::Long => {
            if close < exit_low {
                next.direction = Direction::Flat;
                next.target_qty = Some(Decimal::ZERO);
                Step::Transition {
                    from,
                    to: Direction::Flat,
                    target_qty: Decimal::ZERO,
                    diagnostics: diagnostics("close below exit low"),
                }
            } else {
                Step::NoChange {
                    diagnostics: diagnostics("long retained"),
                }
            }
        }
        Direction::Short => {
            if close > exit_high {
                next.direction = Direction::Flat;
                next.target_qty = Some(Decimal::ZERO);
                Step::Transition {
                    from,
                    to: Direction::Flat,
                    target_qty: Decimal::ZERO,
                    diagnostics: diagnostics("close above exit high"),
                }
            } else {
                Step::NoChange {
                    diagnostics: diagnostics("short retained"),
                }
            }
        }
    };

    append(&mut next, candle, config.entry_lookback);
    Ok((next, outcome))
}

fn duplicate_or_correction(
    state: &ModelState,
    candle: &ClosedCandle,
) -> Result<(ModelState, Step), ModelError> {
    let Some(stored) = state.window.back() else {
        return Err(ModelError::Candle("duplicate id without stored candle"));
    };
    if stored.start_time_ms != candle.id.start_time_ms {
        return Err(ModelError::Candle("last id does not match window"));
    }
    let same = stored.open == candle.open
        && stored.high == candle.high
        && stored.low == candle.low
        && stored.close == candle.close
        && stored.volume == candle.volume
        && stored.end_time_exclusive_ms == candle.end_time_exclusive_ms;
    if same {
        return Ok((state.clone(), Step::IgnoredDuplicate));
    }
    let mut next = state.clone();
    next.entries_suspended = Some(SuspendReason::Correction);
    Ok((
        next,
        Step::Suspended {
            reason: SuspendReason::Correction,
        },
    ))
}

fn append(state: &mut ModelState, candle: &ClosedCandle, keep: usize) {
    state.window.push_back(StoredCandle {
        start_time_ms: candle.id.start_time_ms,
        end_time_exclusive_ms: candle.end_time_exclusive_ms,
        open: candle.open,
        high: candle.high,
        low: candle.low,
        close: candle.close,
        volume: candle.volume,
    });
    while state.window.len() > keep {
        state.window.pop_front();
    }
    state.last_start_ms = Some(candle.id.start_time_ms);
}

fn channel(window: &VecDeque<StoredCandle>, n: usize) -> Option<(Decimal, Decimal)> {
    if window.len() < n {
        return None;
    }
    let start = window.len() - n;
    let mut high = None;
    let mut low = None;
    for bar in window.iter().skip(start) {
        high = Some(high.map_or(bar.high, |value: Decimal| value.max(bar.high)));
        low = Some(low.map_or(bar.low, |value: Decimal| value.min(bar.low)));
    }
    Some((high?, low?))
}

fn validate_candle(config: &ModelConfig, candle: &ClosedCandle) -> Result<(), ModelError> {
    if candle.id.venue != config.venue
        || candle.id.product != config.product
        || candle.id.instrument != config.instrument
        || candle.id.interval != config.interval
    {
        return Err(ModelError::Candle("candle identity does not match config"));
    }
    if candle.id.interval != DAILY_INTERVAL {
        return Err(ModelError::Candle("V1 accepts only daily candles"));
    }
    if candle.end_time_exclusive_ms != candle.id.start_time_ms.saturating_add(DAY_MS) {
        return Err(ModelError::Candle(
            "daily candle must cover [start, start+1d)",
        ));
    }
    if candle.open <= Decimal::ZERO
        || candle.high <= Decimal::ZERO
        || candle.low <= Decimal::ZERO
        || candle.close <= Decimal::ZERO
    {
        return Err(ModelError::Candle("prices must be positive"));
    }
    if candle.volume < Decimal::ZERO {
        return Err(ModelError::Candle("volume must be nonnegative"));
    }
    if candle.high < candle.low
        || candle.high < candle.open
        || candle.high < candle.close
        || candle.low > candle.open
        || candle.low > candle.close
    {
        return Err(ModelError::Candle("ohlc is inconsistent"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(allow_short: bool) -> ModelConfig {
        ModelConfig {
            venue: "bybit".into(),
            product: "linear".into(),
            instrument: "BTCUSDT".into(),
            interval: DAILY_INTERVAL.into(),
            entry_lookback: ENTRY_LOOKBACK,
            exit_lookback: EXIT_LOOKBACK,
            position_qty: Decimal::new(1, 3),
            allow_short,
        }
    }

    fn candle(day: u64, high: i64, low: i64, close: i64) -> ClosedCandle {
        let start = day * DAY_MS;
        ClosedCandle {
            id: CandleId {
                venue: "bybit".into(),
                product: "linear".into(),
                instrument: "BTCUSDT".into(),
                interval: DAILY_INTERVAL.into(),
                start_time_ms: start,
            },
            end_time_exclusive_ms: start + DAY_MS,
            open: Decimal::new(close, 0),
            high: Decimal::new(high, 0),
            low: Decimal::new(low, 0),
            close: Decimal::new(close, 0),
            volume: Decimal::ONE,
            confirmed: true,
        }
    }

    fn feed(
        mut state: ModelState,
        config: &ModelConfig,
        bars: &[ClosedCandle],
    ) -> (ModelState, Step) {
        let mut last = Step::NotReady {
            preceding: 0,
            required: config.entry_lookback,
        };
        for bar in bars {
            let (next, step) = on_candle(&state, config, bar).unwrap();
            state = next;
            last = step;
        }
        (state, last)
    }

    fn warmup(allow_short: bool) -> (ModelState, ModelConfig) {
        let config = config(allow_short);
        let bars: Vec<_> = (0..20).map(|day| candle(day, 100, 90, 95)).collect();
        let (state, step) = feed(ModelState::new(), &config, &bars);
        assert!(matches!(step, Step::NotReady { preceding: 19, .. }));
        assert_eq!(state.target_qty, None);
        assert_eq!(state.direction, Direction::Flat);
        (state, config)
    }

    #[test]
    fn warmup_does_not_emit_a_flat_target() {
        let (state, _) = warmup(true);
        assert_eq!(state.strategy_id, "DonchianDailyV1");
        assert_eq!(state.version, "1");
        assert_eq!(state.preceding_len(), 20);
        assert_eq!(state.target_qty, None);
    }

    #[test]
    fn entry_uses_prior_twenty_and_excludes_the_current_candle() {
        let (state, config) = warmup(true);
        let breakout = candle(20, 130, 100, 120);
        let (next, step) = on_candle(&state, &config, &breakout).unwrap();
        match step {
            Step::Transition {
                to: Direction::Long,
                target_qty,
                diagnostics,
                ..
            } => {
                assert_eq!(target_qty, Decimal::new(1, 3));
                assert_eq!(diagnostics.entry_high, Some(Decimal::new(100, 0)));
                assert_eq!(diagnostics.entry_low, Some(Decimal::new(90, 0)));
                assert_eq!(diagnostics.exit_low, Some(Decimal::new(90, 0)));
            }
            other => panic!("expected long entry, got {other:?}"),
        }
        assert_eq!(next.direction, Direction::Long);
        assert_eq!(next.target_qty, Some(Decimal::new(1, 3)));
        assert_eq!(next.window.back().unwrap().close, Decimal::new(120, 0));
        assert_ne!(
            channel(&next.window, 20).unwrap().0,
            Decimal::new(100, 0),
            "after append the stored window includes the breakout high"
        );
    }

    #[test]
    fn equality_does_not_enter_or_exit() {
        let (state, config) = warmup(true);
        let equal = candle(20, 100, 90, 100);
        let (next, step) = on_candle(&state, &config, &equal).unwrap();
        assert!(matches!(step, Step::NoChange { .. }));
        assert_eq!(next.direction, Direction::Flat);
        assert_eq!(next.target_qty, None);

        let (long_state, _) = on_candle(&state, &config, &candle(20, 110, 100, 105)).unwrap();
        let hold = candle(21, 100, 90, 90);
        let (held, step) = on_candle(&long_state, &config, &hold).unwrap();
        assert!(matches!(step, Step::NoChange { .. }));
        assert_eq!(held.direction, Direction::Long);
        assert_eq!(held.target_qty, Some(Decimal::new(1, 3)));
    }

    #[test]
    fn short_entry_and_disabled_short() {
        let (state, config) = warmup(true);
        let (next, step) = on_candle(&state, &config, &candle(20, 95, 80, 85)).unwrap();
        assert!(matches!(
            step,
            Step::Transition {
                to: Direction::Short,
                ..
            }
        ));
        assert_eq!(next.target_qty, Some(Decimal::new(-1, 3)));

        let (_, no_short) = warmup(false);
        let (blocked, step) = on_candle(&state, &no_short, &candle(20, 95, 80, 85)).unwrap();
        assert!(matches!(step, Step::NoChange { .. }));
        assert_eq!(blocked.direction, Direction::Flat);
        assert_eq!(blocked.target_qty, None);
    }

    #[test]
    fn long_exit_does_not_reverse_on_the_same_candle() {
        let (state, config) = warmup(true);
        let (long_state, _) = on_candle(&state, &config, &candle(20, 110, 100, 105)).unwrap();
        let crash = candle(21, 80, 70, 75);
        let (flat, step) = on_candle(&long_state, &config, &crash).unwrap();
        match step {
            Step::Transition {
                to: Direction::Flat,
                target_qty,
                ..
            } => assert_eq!(target_qty, Decimal::ZERO),
            other => panic!("expected exit to flat, got {other:?}"),
        }
        assert_eq!(flat.direction, Direction::Flat);
        assert_ne!(flat.direction, Direction::Short);
    }

    #[test]
    fn short_exits_on_prior_exit_high() {
        let (state, config) = warmup(true);
        let (short_state, _) = on_candle(&state, &config, &candle(20, 95, 80, 85)).unwrap();
        let rally = candle(21, 120, 100, 115);
        let (flat, step) = on_candle(&short_state, &config, &rally).unwrap();
        assert!(matches!(
            step,
            Step::Transition {
                to: Direction::Flat,
                ..
            }
        ));
        assert_eq!(flat.target_qty, Some(Decimal::ZERO));
    }

    #[test]
    fn unchanged_signal_does_not_resize() {
        let (state, config) = warmup(true);
        let (long_state, first) = on_candle(&state, &config, &candle(20, 110, 100, 105)).unwrap();
        assert!(matches!(first, Step::Transition { .. }));
        let (held, second) = on_candle(&long_state, &config, &candle(21, 112, 100, 108)).unwrap();
        assert!(matches!(second, Step::NoChange { .. }));
        assert_eq!(held.target_qty, long_state.target_qty);
        assert_eq!(held.target_qty, Some(Decimal::new(1, 3)));
    }

    #[test]
    fn forming_duplicate_gap_and_correction() {
        let (state, config) = warmup(true);
        let mut forming = candle(20, 110, 100, 105);
        forming.confirmed = false;
        let (unchanged, step) = on_candle(&state, &config, &forming).unwrap();
        assert_eq!(step, Step::IgnoredForming);
        assert_eq!(unchanged, state);

        let (entered, _) = on_candle(&state, &config, &candle(20, 110, 100, 105)).unwrap();
        let (again, step) = on_candle(&entered, &config, &candle(20, 110, 100, 105)).unwrap();
        assert_eq!(step, Step::IgnoredDuplicate);
        assert_eq!(again, entered);

        let mut corrected = candle(20, 110, 100, 105);
        corrected.close = Decimal::new(106, 0);
        corrected.high = Decimal::new(111, 0);
        let (suspended, step) = on_candle(&entered, &config, &corrected).unwrap();
        assert_eq!(
            step,
            Step::Suspended {
                reason: SuspendReason::Correction
            }
        );
        assert_eq!(suspended.direction, Direction::Long);
        assert_eq!(suspended.target_qty, entered.target_qty);

        let (fresh, config) = warmup(true);
        let (gapped, step) = on_candle(&fresh, &config, &candle(22, 130, 100, 120)).unwrap();
        assert_eq!(
            step,
            Step::Suspended {
                reason: SuspendReason::Gap
            }
        );
        assert_eq!(gapped.direction, Direction::Flat);
        assert_eq!(gapped.target_qty, None);
        let (still, step) = on_candle(&gapped, &config, &candle(23, 140, 100, 130)).unwrap();
        assert_eq!(
            step,
            Step::Suspended {
                reason: SuspendReason::Gap
            }
        );
        assert_eq!(still.direction, Direction::Flat);
        assert_eq!(still.target_qty, None);

        let mut rearmed = still.clone();
        rearm(&mut rearmed);
        assert_eq!(rearmed.entries_suspended, None);
        let (blocked, step) = on_candle(&rearmed, &config, &candle(23, 140, 100, 130)).unwrap();
        assert_eq!(
            step,
            Step::Suspended {
                reason: SuspendReason::Gap
            }
        );
        assert_eq!(blocked.direction, Direction::Flat);
        assert_eq!(blocked.target_qty, None);
    }

    #[test]
    fn twenty_and_ten_windows_exclude_the_current_candle() {
        let config = config(true);
        let mut bars = Vec::new();
        for day in 0..10 {
            bars.push(candle(day, 110, 50, 80));
        }
        for day in 10..20 {
            bars.push(candle(day, 100, 90, 95));
        }
        let (state, _) = feed(ModelState::new(), &config, &bars);
        let (held, step) = on_candle(&state, &config, &candle(20, 109, 100, 105)).unwrap();
        match step {
            Step::NoChange { diagnostics } => {
                assert_eq!(diagnostics.entry_high, Some(Decimal::new(110, 0)));
                assert_eq!(diagnostics.entry_low, Some(Decimal::new(50, 0)));
                assert_eq!(diagnostics.exit_high, Some(Decimal::new(100, 0)));
                assert_eq!(diagnostics.exit_low, Some(Decimal::new(90, 0)));
            }
            other => panic!("105 is inside the 20-day channel, got {other:?}"),
        }
        assert_eq!(held.direction, Direction::Flat);

        let (long_state, step) = on_candle(&state, &config, &candle(20, 120, 100, 115)).unwrap();
        assert!(matches!(
            step,
            Step::Transition {
                to: Direction::Long,
                ..
            }
        ));
        let (flat, step) = on_candle(&long_state, &config, &candle(21, 89, 85, 88)).unwrap();
        match step {
            Step::Transition {
                to: Direction::Flat,
                diagnostics,
                ..
            } => {
                assert_eq!(diagnostics.exit_low, Some(Decimal::new(90, 0)));
                assert_eq!(diagnostics.entry_low, Some(Decimal::new(50, 0)));
            }
            other => panic!("88 breaks the 10-day exit low only, got {other:?}"),
        }
        assert_eq!(flat.direction, Direction::Flat);
    }

    #[test]
    fn same_inputs_match_independent_of_call_count() {
        let config = config(true);
        let bars: Vec<_> = (0..21).map(|day| candle(day, 100, 90, 95)).collect();
        let (first, step_a) = feed(ModelState::new(), &config, &bars);
        let (second, step_b) = feed(ModelState::new(), &config, &bars);
        assert_eq!(first, second);
        assert_eq!(step_a, step_b);
    }
}
