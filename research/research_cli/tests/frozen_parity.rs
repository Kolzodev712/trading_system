//! Stored-decision and Candidate #2 model parity. Does not require archived ledgers.

use research_engine::btc_price_ma_long_flat_v1::experiment::{scenario_by_name, score_lookback};
use research_engine::btc_price_ma_long_flat_v1::offline::{
    default_replay_request, merge_datasets, replay,
};
use research_engine::btc_price_ma_long_flat_v1::PRIMARY_LOOKBACK;
use research_engine::daily_dataset::load_historical_dataset;
use research_engine::market_inputs::{
    load_funding_file, load_price_series, market_inputs_from_files,
};
use research_engine::simulation::{evaluate_experiment, load_decision_log, EvaluateRequest};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const DONCHIAN_COST_A: &str = "-41.4028767858150";
const DONCHIAN_CONFIG: &str =
    "sha256:8057b5024b6df11096bc7541e8f470e52794f5e2025503da45beedc0aab33114";
const DONCHIAN_SCENARIO: &str =
    "sha256:8d60a72a99883fd12c3ce06628d51d644b7fa15c4dbc770d1289f48050734636";
const MA_COST_A: &str = "20.6120701293059";
const MA_CONFIG: &str = "sha256:6e02ddef41a418dc26f5ea8acfd592417846362211b3f70de3ab553d41c3559a";
const MA_SCENARIO: &str = "sha256:80603e448c62b8e225bdd2ce12d0a3e503e666097c04b3025d884f5fad9d1fbf";

#[test]
fn frozen_donchian_stored_decisions_score_cost_a() {
    let root = root();
    let saved =
        load_decision_log(&root.join(
            "data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01-decisions.json",
        ))
        .unwrap();
    assert_eq!(saved.run.config_hash, DONCHIAN_CONFIG);
    assert_eq!(saved.run.entries, 29);
    assert_eq!(saved.run.exits, 29);

    let aux = root.join("data/donchian_daily/aux");
    let market = market_inputs_from_files(
        &load_price_series(&aux.join("trade-1m.json")).unwrap(),
        &load_funding_file(&aux.join("funding.json")).unwrap(),
        &load_price_series(&aux.join("mark-60.json")).unwrap(),
        None,
    )
    .unwrap();
    let evaluation = evaluate_experiment(&EvaluateRequest {
        decisions: &saved.decisions,
        final_direction: &saved.run.final_direction,
        signal_config_hash: &saved.run.config_hash,
        daily_dataset_hash: &saved.run.dataset_hash,
        position_qty: saved.run.position_qty,
        market: &market,
    })
    .unwrap();
    let cost_a = evaluation
        .pairs
        .iter()
        .find(|pair| pair.scenario.name == "cost_a")
        .unwrap();
    assert_eq!(cost_a.donchian.summary.net_pnl.to_string(), DONCHIAN_COST_A);
    assert_eq!(cost_a.donchian.summary.signal_config_hash, DONCHIAN_CONFIG);
    assert_eq!(cost_a.donchian.summary.scenario_hash, DONCHIAN_SCENARIO);
}

#[test]
fn frozen_ma_n28_replay_and_cost_a_match_stored_artifacts() {
    let root = root();
    let main = load_historical_dataset(
        &root.join("data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json"),
    )
    .unwrap();
    let warmup = load_historical_dataset(
        &root.join("data/btc_price_ma_long_flat_v1/warmup-2022-11-01_2023-01-01.json"),
    )
    .unwrap();
    let merged = merge_datasets(&warmup, &main).unwrap();
    let log = replay(&merged, &default_replay_request(PRIMARY_LOOKBACK)).unwrap();
    let stored =
        load_decision_log(&root.join("data/btc_price_ma_long_flat_v1/decisions-n28.json")).unwrap();
    assert_eq!(log.run.config_hash, stored.run.config_hash);
    assert_eq!(log.run.entries, stored.run.entries);
    assert_eq!(log.run.exits, stored.run.exits);
    assert_eq!(log.run.final_direction, stored.run.final_direction);

    let aux = root.join("data/btc_price_ma_long_flat_v1/aux");
    let market = market_inputs_from_files(
        &load_price_series(&aux.join("trade-1m.json")).unwrap(),
        &load_funding_file(&aux.join("funding.json")).unwrap(),
        &load_price_series(&aux.join("mark-60.json")).unwrap(),
        None,
    )
    .unwrap();
    let sims = score_lookback(&log, PRIMARY_LOOKBACK, &market).unwrap();
    let cost_a = scenario_by_name(&sims, "cost_a").unwrap();
    assert_eq!(cost_a.summary.net_pnl.to_string(), MA_COST_A);
    assert_eq!(cost_a.summary.signal_config_hash, MA_CONFIG);
    assert_eq!(cost_a.summary.scenario_hash, MA_SCENARIO);
}
