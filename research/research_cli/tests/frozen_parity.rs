//! Reproduce frozen Candidate #1/#2 identity and Cost A nets through research_cli wiring.

use research_engine::btc_price_ma_long_flat_v1::experiment::{scenario_by_name, score_lookback};
use research_engine::btc_price_ma_long_flat_v1::offline::{
    default_replay_request, merge_datasets, replay,
};
use research_engine::btc_price_ma_long_flat_v1::PRIMARY_LOOKBACK;
use research_engine::donchian_daily_v1::aux::{
    load_funding_file, load_price_series, market_inputs_from_files,
};
use research_engine::donchian_daily_v1::backtest::{
    evaluate_experiment, load_decision_log, reproduce_frozen_decisions, EvaluateRequest,
};
use research_engine::donchian_daily_v1::offline::load_historical_dataset;
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn frozen_donchian_replay_and_cost_a_match_stored_artifacts() {
    let root = root();
    let dataset = load_historical_dataset(
        &root.join("data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json"),
    )
    .unwrap();
    let saved =
        load_decision_log(&root.join(
            "data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01-decisions.json",
        ))
        .unwrap();
    let fresh = reproduce_frozen_decisions(&dataset, &saved).unwrap();
    assert_eq!(fresh.run.config_hash, saved.run.config_hash);
    assert_eq!(fresh.run.dataset_hash, saved.run.dataset_hash);
    assert_eq!(fresh.run.entries, 29);
    assert_eq!(fresh.run.exits, 29);

    let aux = root.join("data/donchian_daily/aux");
    let market = market_inputs_from_files(
        &load_price_series(&aux.join("trade-1m.json")).unwrap(),
        &load_funding_file(&aux.join("funding.json")).unwrap(),
        &load_price_series(&aux.join("mark-60.json")).unwrap(),
        None,
    )
    .unwrap();
    let evaluation = evaluate_experiment(&EvaluateRequest {
        decisions: &fresh.decisions,
        final_direction: &fresh.run.final_direction,
        signal_config_hash: &fresh.run.config_hash,
        daily_dataset_hash: &fresh.run.dataset_hash,
        position_qty: fresh.run.position_qty,
        market: &market,
    })
    .unwrap();
    let cost_a = evaluation
        .pairs
        .iter()
        .find(|pair| pair.scenario.name == "cost_a")
        .unwrap();
    let stored: Value = serde_json::from_slice(
        &std::fs::read(
            root.join("data/donchian_daily/backtest/donchian_daily_v1/cost_a/summary.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        cost_a.donchian.summary.net_pnl.to_string(),
        stored["net_pnl"].as_str().unwrap()
    );
    assert_eq!(
        cost_a.donchian.summary.signal_config_hash,
        stored["signal_config_hash"].as_str().unwrap()
    );
    assert_eq!(
        cost_a.donchian.summary.scenario_hash,
        stored["scenario_hash"].as_str().unwrap()
    );
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
    let stored_summary: Value =
        serde_json::from_slice(
            &std::fs::read(root.join(
                "data/btc_price_ma_long_flat_v1/backtest/primary_n28/n28/cost_a/summary.json",
            ))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        cost_a.summary.net_pnl.to_string(),
        stored_summary["net_pnl"].as_str().unwrap()
    );
    assert_eq!(
        cost_a.summary.signal_config_hash,
        stored_summary["signal_config_hash"].as_str().unwrap()
    );
    assert_eq!(
        cost_a.summary.scenario_hash,
        stored_summary["scenario_hash"].as_str().unwrap()
    );
}
