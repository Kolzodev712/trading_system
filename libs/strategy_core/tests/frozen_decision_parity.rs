//! Prove the extracted model reproduces frozen Candidate #2 decision transitions.
//!
//! This is implementation-parity against stored logs, not the independent Python audit.

use rust_decimal::Decimal;
use serde::Deserialize;
use strategy_core::btc_price_ma_long_flat_v1::{
    config_hash, on_candle, position_qty, Direction, ModelState, Step, DIAGNOSTIC_LOOKBACKS,
    PRIMARY_LOOKBACK, STRATEGY_NAME, STRATEGY_VERSION,
};

#[derive(Debug, Deserialize)]
struct DecisionLog {
    run: RunMetadata,
    decisions: Vec<DecisionRow>,
}

#[derive(Debug, Deserialize)]
struct RunMetadata {
    strategy_id: String,
    strategy_version: String,
    config_hash: String,
    entries: usize,
    exits: usize,
    final_direction: String,
}

#[derive(Debug, Deserialize)]
struct DecisionRow {
    close: Decimal,
    previous_direction: String,
    direction: String,
    readiness: String,
    target: Target,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Target {
    NotReady,
    NoChange,
    Quantity { quantity: Decimal },
}

fn log_path(lookback: usize) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/btc_price_ma_long_flat_v1")
        .join(format!("decisions-n{lookback}.json"))
}

fn load(lookback: usize) -> DecisionLog {
    let body = std::fs::read_to_string(log_path(lookback)).unwrap_or_else(|err| {
        panic!("read frozen decisions for N={lookback}: {err}");
    });
    serde_json::from_str(&body).expect("frozen decision log JSON")
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Flat => "flat",
        Direction::Long => "long",
    }
}

fn replay_matches(lookback: usize) {
    let log = load(lookback);
    assert_eq!(log.run.strategy_id, STRATEGY_NAME);
    assert_eq!(log.run.strategy_version, STRATEGY_VERSION);
    assert_eq!(log.run.config_hash, config_hash(lookback).unwrap());

    let closes: Vec<Decimal> = log.decisions.iter().map(|row| row.close).collect();
    let mut state = ModelState::new();
    let mut entries = 0usize;
    let mut exits = 0usize;

    for (index, row) in log.decisions.iter().enumerate() {
        if index < lookback {
            assert_eq!(row.readiness, "not_ready");
            assert_eq!(row.direction, "flat");
            assert!(matches!(row.target, Target::NotReady));
            continue;
        }

        let window = &closes[index + 1 - lookback..=index];
        let previous = direction_name(state.direction);
        let (next, step) = on_candle(&state, lookback, window).expect("model step");
        assert_eq!(row.readiness, "ready");
        assert_eq!(row.previous_direction, previous);
        assert_eq!(row.direction, direction_name(next.direction));

        match &step {
            Step::NoChange { .. } => {
                assert!(matches!(row.target, Target::NoChange));
                assert_eq!(next.requested_qty, state.requested_qty);
            }
            Step::Transition { target_qty, .. } => {
                match &row.target {
                    Target::Quantity { quantity } => assert_eq!(quantity, target_qty),
                    other => panic!("expected quantity target, got {other:?}"),
                }
                if previous == "flat" && row.direction == "long" && *target_qty == position_qty() {
                    entries += 1;
                }
                if previous == "long" && row.direction == "flat" && *target_qty == Decimal::ZERO {
                    exits += 1;
                }
            }
            Step::NotReady { .. } => panic!("ready window must not emit NotReady"),
        }
        state = next;
    }

    assert_eq!(entries, log.run.entries);
    assert_eq!(exits, log.run.exits);
    assert_eq!(direction_name(state.direction), log.run.final_direction);
}

#[test]
fn frozen_n14_decision_transitions() {
    replay_matches(DIAGNOSTIC_LOOKBACKS[0]);
}

#[test]
fn frozen_n28_decision_transitions() {
    replay_matches(PRIMARY_LOOKBACK);
}

#[test]
fn frozen_n70_decision_transitions() {
    replay_matches(DIAGNOSTIC_LOOKBACKS[1]);
}
