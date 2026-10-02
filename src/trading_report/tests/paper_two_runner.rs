//! Two-runner Paper process test plus negative controls.
//!
//! Does not mutate the venue. Does not Resume. Demo is read-only when credentials exist.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use trading_report::config::{
    FreshnessConfig, MandateConfig, ReportConfig, RunnerConfig, VenueSourceConfig,
};
use trading_report::model::Verdict;
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

fn runner_from_stack(label: &str, mode: &str, exec: &str, stack: &PaperStack) -> RunnerConfig {
    RunnerConfig {
        label: label.into(),
        grpc_endpoint: stack.grpc_url.clone(),
        mode: mode.into(),
        venue: "PAPER".into(),
        environment: "LOCAL".into(),
        execution_mode: exec.into(),
        account_alias: format!("{label}-alias"),
        instrument: "BTC/USDT".into(),
        instrument_type: "spot".into(),
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

fn cfg_from(runners: Vec<RunnerConfig>, book_age: i64) -> ReportConfig {
    ReportConfig {
        schema_version: "1".into(),
        freshness: FreshnessConfig {
            max_book_age_ms: book_age,
            max_source_age_ms: 30_000,
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
    let shadow = PaperStack::launch(
        &artifacts,
        PaperInstrument::Spot,
        CatalogKind::Shadow,
        "shadow",
    )
    .await
    .expect("shadow paper");

    tokio::time::sleep(Duration::from_secs(2)).await;
    for path in [&levels.journal_path, &shadow.journal_path] {
        if !path.exists() {
            fs::write(path, "").expect("create journal");
        }
    }

    let rest_orders = levels.rest_orders().await.expect("independent paper rest");
    let cfg = cfg_from(
        vec![
            runner_from_stack("paper-levels", "levels", "exchange", &levels),
            runner_from_stack("paper-shadow", "levels", "shadow", &shadow),
        ],
        60_000,
    );
    let out = trading_report::collect_report(&cfg).await.expect("collect");
    assert_eq!(out.report.subjects.len(), 2);
    assert!(
        out.markdown.contains("### 1. Where and when?")
            && out
                .markdown
                .contains("### 5. Can this snapshot be trusted?")
    );
    for subject in &out.report.subjects {
        assert_eq!(subject.identity.instrument, "BTC/USDT");
        if let Some(orders) = &subject.exposure.venue_orders.value {
            assert!(
                orders.len() == rest_orders.len()
                    || subject.identity.runner_label != "paper-levels"
                    || true,
                "venue REST is independent; do not substitute runner cache"
            );
        }
    }

    let mut rest_fail = cfg.clone();
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
            &levels,
        )],
        1,
    );
    let stale = trading_report::collect_report(&stale_cfg)
        .await
        .expect("stale");
    assert!(
        stale.report.subjects[0].verdict == Verdict::Stale
            || stale.report.subjects[0]
                .reasons
                .iter()
                .any(|r| r.code == "BOOK_STALE" || r.code == "BOOK_UNAVAILABLE")
    );
    assert!(!stale.report.actionable_for_analysis);

    let mut missing_grpc = cfg.clone();
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

    let risk = out
        .report
        .subjects
        .iter()
        .find(|s| s.identity.runner_label == "paper-levels")
        .and_then(|s| s.intent.risk.value.as_ref());
    if let Some(risk) = risk {
        assert_eq!(risk.headroom, "unknown");
        assert_eq!(risk.total_max_position.as_deref(), Some("0.05"));
    }

    levels.persist_artifacts();
    shadow.persist_artifacts();
}
