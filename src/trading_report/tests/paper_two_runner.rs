//! Two-runner Paper process test plus negative controls.
//!
//! Does not mutate the venue. Does not Resume. Demo is read-only when credentials exist.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use trading_report::config::{
    FreshnessConfig, MandateConfig, ReportConfig, RunnerConfig, VenueSourceConfig,
};
use trading_report::model::Verdict;
use trading_report::venue_source::{matches_instrument, qty_close};
use workflow_harness::paper::{CatalogKind, PaperInstrument, PaperStack};

fn mandate() -> MandateConfig {
    MandateConfig {
        advisory: true,
        allowed_instruments: vec!["BTC/USDT".into()],
        allowed_direction: "both".into(),
        max_leverage: "1".into(),
        max_exposure: "0.05".into(),
        max_loss_per_trade: "50".into(),
        max_loss_per_day: "100".into(),
        time_horizon: "intraday".into(),
    }
}

fn runner_from_stack(
    label: &str,
    mode: &str,
    exec: &str,
    instrument_type: &str,
    stack: &PaperStack,
) -> RunnerConfig {
    RunnerConfig {
        label: label.into(),
        grpc_endpoint: stack.grpc_url.clone(),
        mode: mode.into(),
        venue: "PAPER".into(),
        environment: "LOCAL".into(),
        execution_mode: exec.into(),
        account_alias: format!("{label}-alias"),
        instrument: "BTC/USDT".into(),
        instrument_type: instrument_type.into(),
        journal_path: stack.journal_path.clone(),
        executable_revision: None,
        venue_source: VenueSourceConfig {
            kind: "paper".into(),
            rest_base: stack.http.clone(),
            api_key_env: None,
            api_secret_env: None,
            category: None,
        },
    }
}

fn cfg_from(runners: Vec<RunnerConfig>, book_age: i64, source_age: i64) -> ReportConfig {
    ReportConfig {
        schema_version: "1".into(),
        freshness: FreshnessConfig {
            max_book_age_ms: book_age,
            max_source_age_ms: source_age,
            qty_tolerance: "0.00000001".into(),
        },
        mandate: mandate(),
        runners,
    }
}

#[tokio::test]
#[ignore = "spawns k-paper and two runners; run with --ignored"]
async fn two_runner_paper_and_negatives() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/trading-report-paper");
    let _ = fs::create_dir_all(&artifacts);
    let levels = PaperStack::launch(
        &artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    .expect("levels paper");
    let strategies = PaperStack::launch_c2_shadow_readonly(&artifacts)
        .await
        .expect("strategies shadow");

    tokio::time::sleep(Duration::from_secs(2)).await;
    for path in [&levels.journal_path, &strategies.journal_path] {
        if !path.exists() {
            fs::write(path, "").expect("create journal");
        }
    }

    let rest_orders = levels.rest_orders().await.expect("independent paper rest");
    let rest_balances = levels.rest_balances().await.expect("independent balances");
    let rest_ids: BTreeSet<_> = rest_orders
        .iter()
        .filter(|o| matches_instrument(&o.instrument, "BTC/USDT"))
        .map(|o| o.order_id.clone())
        .collect();
    let rest_btc = rest_balances
        .iter()
        .find(|b| b.asset.eq_ignore_ascii_case("BTC"))
        .map(|b| b.total.normalize().to_string())
        .unwrap_or_else(|| "0".into());

    let two_cfg = cfg_from(
        vec![
            runner_from_stack("paper-levels", "levels", "exchange", "spot", &levels),
            runner_from_stack(
                "paper-strategies-shadow",
                "strategies",
                "shadow",
                "spot",
                &strategies,
            ),
        ],
        60_000,
        30_000,
    );
    let two = trading_report::collect_report(&two_cfg)
        .await
        .expect("two-container collect");
    assert_eq!(two.report.subjects.len(), 2);
    let worst = two
        .report
        .subjects
        .iter()
        .map(|s| s.verdict)
        .fold(Verdict::Verified, Verdict::worse);
    assert_eq!(two.report.verdict, worst);
    assert_eq!(
        two.report.verdict,
        Verdict::Verified,
        "combined Levels + Strategies overall must be verified: {:?}",
        two.report.reasons
    );
    assert!(two.report.actionable_for_analysis);
    let combined_dir = artifacts.join("combined-report");
    trading_report::write_outputs(&combined_dir, &two).expect("write combined");
    let combined_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(combined_dir.join("report.json")).unwrap())
            .unwrap();
    assert_eq!(combined_json["verdict"], "verified");
    assert_eq!(combined_json["actionable_for_analysis"], true);
    assert_eq!(
        combined_json["subjects"].as_array().map(|a| a.len()),
        Some(2)
    );
    let strategies_subject = two
        .report
        .subjects
        .iter()
        .find(|s| s.identity.runner_label == "paper-strategies-shadow")
        .expect("strategies subject");
    assert_eq!(
        strategies_subject.identity.runner_mode.value.as_deref(),
        Some("strategies")
    );
    assert_eq!(
        strategies_subject.identity.execution_mode.value.as_deref(),
        Some("shadow")
    );
    assert_eq!(strategies_subject.verdict, Verdict::Verified);
    let levels_subject = two
        .report
        .subjects
        .iter()
        .find(|s| s.identity.runner_label == "paper-levels")
        .expect("levels subject");
    assert_eq!(levels_subject.verdict, Verdict::Verified);

    let healthy_cfg = cfg_from(
        vec![runner_from_stack(
            "paper-levels",
            "levels",
            "exchange",
            "spot",
            &levels,
        )],
        60_000,
        30_000,
    );
    let healthy = trading_report::collect_report(&healthy_cfg)
        .await
        .expect("levels collect");
    assert_eq!(healthy.report.subjects.len(), 1);
    assert_eq!(
        healthy.report.verdict,
        Verdict::Verified,
        "healthy levels report must be verified: {:?}",
        healthy.report.reasons
    );
    assert!(healthy.report.actionable_for_analysis);
    assert!(
        healthy.markdown.contains("### 1. Where and when?")
            && healthy
                .markdown
                .contains("### 5. Can this snapshot be trusted?")
    );

    let subject = &healthy.report.subjects[0];
    let venue_orders = subject
        .exposure
        .venue_orders
        .value
        .as_ref()
        .expect("venue orders from independent REST");
    let runner_orders = subject
        .exposure
        .runner_orders
        .value
        .as_ref()
        .expect("runner orders");
    let venue_ids: BTreeSet<_> = venue_orders.iter().map(|o| o.order_id.clone()).collect();
    let runner_ids: BTreeSet<_> = runner_orders.iter().map(|o| o.order_id.clone()).collect();
    assert_eq!(
        venue_ids, rest_ids,
        "report venue orders must equal independent REST, not the runner cache"
    );
    assert_eq!(
        rest_ids, runner_ids,
        "independent REST order identities must agree with runner as sets"
    );

    let venue_btc = subject
        .exposure
        .venue_balances
        .value
        .as_ref()
        .and_then(|bals| {
            bals.iter()
                .find(|b| b.asset.eq_ignore_ascii_case("BTC"))
                .map(|b| b.total.clone())
        })
        .unwrap_or_else(|| "0".into());
    let runner_btc = subject
        .exposure
        .runner_positions
        .value
        .as_ref()
        .and_then(|pos| pos.first().map(|p| p.quantity.clone()))
        .unwrap_or_else(|| "0".into());
    let tol = rust_decimal::Decimal::new(1, 8);
    assert!(
        qty_close(&venue_btc, &rest_btc, tol).expect("venue vs rest qty"),
        "report venue inventory must equal independent REST {venue_btc} vs {rest_btc}"
    );
    assert!(
        qty_close(&rest_btc, &runner_btc, tol).expect("rest vs runner qty"),
        "independent REST quantity must agree with runner {rest_btc} vs {runner_btc}"
    );

    let healthy_dir = artifacts.join("healthy-report");
    trading_report::write_outputs(&healthy_dir, &healthy).expect("write healthy");
    let saved_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(healthy_dir.join("report.json")).unwrap())
            .unwrap();
    let saved_md = fs::read_to_string(healthy_dir.join("report.md")).unwrap();
    assert_eq!(saved_json["verdict"], "verified");
    assert_eq!(saved_json["actionable_for_analysis"], true);
    assert!(saved_md.contains("### 1. Where and when?"));
    assert!(saved_md.contains("### 5. Can this snapshot be trusted?"));
    assert!(!saved_md.to_ascii_lowercase().contains("api_secret"));
    levels
        .write_truth("healthy-sources")
        .await
        .expect("source snapshots");

    let mut unresolved_cfg = healthy_cfg.clone();
    let unresolved_journal = artifacts.join("unresolved-journal.jsonl");
    let mut jf = fs::File::create(&unresolved_journal).unwrap();
    writeln!(
        jf,
        r#"{{"client_order_id":"c-unresolved","state":"submitted","created_at_ms":1,"last_check_at_ms":1}}"#
    )
    .unwrap();
    unresolved_cfg.runners[0].journal_path = unresolved_journal;
    let unresolved = trading_report::collect_report(&unresolved_cfg)
        .await
        .expect("unresolved journal");
    assert_eq!(unresolved.report.verdict, Verdict::Unverified);
    assert!(!unresolved.report.actionable_for_analysis);
    assert!(unresolved.report.subjects[0]
        .reasons
        .iter()
        .any(|r| r.code == "JOURNAL_UNRESOLVED"));

    let mut rest_fail = healthy_cfg.clone();
    rest_fail.runners[0].venue_source.rest_base = "http://127.0.0.1:1".into();
    let failed = trading_report::collect_report(&rest_fail)
        .await
        .expect("rendered failure");
    assert!(!failed.report.actionable_for_analysis);
    assert_eq!(failed.report.subjects[0].verdict, Verdict::Unverified);
    assert!(failed.report.subjects[0]
        .reasons
        .iter()
        .any(|r| r.code == "VENUE_REST_ERROR"));
    assert!(failed.markdown.contains("unverified"));
    assert!(!failed.markdown.contains("orders were cancelled"));

    let stale_cfg = cfg_from(
        vec![runner_from_stack(
            "paper-levels",
            "levels",
            "exchange",
            "spot",
            &levels,
        )],
        1,
        30_000,
    );
    let stale = trading_report::collect_report(&stale_cfg)
        .await
        .expect("stale");
    assert!(
        stale.report.subjects[0].verdict == Verdict::Stale
            || stale.report.subjects[0]
                .reasons
                .iter()
                .any(|r| r.code == "BOOK_STALE"
                    || r.code == "BOOK_UNAVAILABLE"
                    || r.code == "SOURCE_STALE")
    );
    assert!(!stale.report.actionable_for_analysis);

    let mut missing_grpc = healthy_cfg.clone();
    missing_grpc.runners[0].grpc_endpoint = "http://127.0.0.1:1".into();
    let missing = trading_report::collect_report(&missing_grpc)
        .await
        .expect("missing grpc");
    assert_eq!(missing.report.subjects[0].verdict, Verdict::Unverified);
    assert!(missing.report.subjects[0]
        .reasons
        .iter()
        .any(|r| r.code == "MONITORING_MISSING" || r.code == "RISK_UNAVAILABLE"));
    assert!(missing.report.subjects[0]
        .exposure
        .runner_orders
        .value
        .is_none());

    let risk = subject.intent.risk.value.as_ref().expect("risk");
    assert_eq!(risk.headroom, "unknown");
    assert_eq!(risk.instrument, "BTC/USDT");
    assert_eq!(risk.total_max_position.as_deref(), Some("0.05"));

    levels.persist_artifacts();
    strategies.persist_artifacts();
}
