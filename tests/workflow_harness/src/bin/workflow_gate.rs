use clap::Parser;
use workflow_harness::artifacts::{
    file_sha256, finish_matrix, git_dirty, git_sha, utc_now, ArtifactSink, Manifest,
};
use workflow_harness::process::{k_paper_dir, ui_dir, workspace_root};
use workflow_harness::scenarios::run_layer;
use workflow_harness::types::{Layer, Verdict};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "paper")]
    layer: String,
    #[arg(long)]
    only: Option<String>,
    /// When set, a FAIL with expected_failure is the success path (negative control).
    #[arg(long)]
    expect_process_failure: bool,
    /// Fail if any participating checkout is dirty. Also honored via WORKFLOW_REQUIRE_CLEAN=1.
    #[arg(long)]
    require_clean: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let layer = match args.layer.as_str() {
        "paper" => Layer::Paper,
        "ui" => Layer::Ui,
        "demo-readonly" => Layer::DemoReadonly,
        "demo-mutating" => Layer::DemoMutating,
        other => {
            eprintln!("unknown layer {other}");
            std::process::exit(2);
        }
    };
    println!("LAYER {}: RUN", args.layer);
    let ws = workspace_root();
    let artifacts_root = ws.join("artifacts/workflows");
    let run_id = format!(
        "{}-{}",
        args.layer,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    let sink = match ArtifactSink::create(&artifacts_root, &run_id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("LAYER {}: FAIL ({e})", args.layer);
            std::process::exit(1);
        }
    };
    let kpaper = k_paper_dir().ok();
    let ui = ui_dir();
    let require_clean =
        args.require_clean || std::env::var("WORKFLOW_REQUIRE_CLEAN").ok().as_deref() == Some("1");
    if require_clean {
        let mut dirty = Vec::new();
        if git_dirty(&ws) {
            dirty.push(format!("trading_system {}", ws.display()));
        }
        if let Some(kp) = kpaper.as_ref() {
            if git_dirty(kp) {
                dirty.push(format!("k-paper {}", kp.display()));
            }
        }
        if matches!(layer, Layer::Ui) && git_dirty(&ui) {
            dirty.push(format!("trading-system-ui {}", ui.display()));
        }
        if !dirty.is_empty() {
            eprintln!(
                "LAYER {}: FAIL (dirty checkout; SHA alone is not reproducible): {}",
                args.layer,
                dirty.join("; ")
            );
            std::process::exit(1);
        }
    }
    let runner_bin = workflow_harness::process::debug_bin(&ws, "runner");
    let k_bin = kpaper
        .as_ref()
        .map(|p| workflow_harness::process::debug_bin(p, "k-paper"));
    let manifest = Manifest {
        run_id: sink.run_id.clone(),
        started_utc: utc_now(),
        ended_utc: None,
        trading_system_sha: git_sha(&ws),
        k_paper_sha: kpaper
            .as_ref()
            .map(|p| git_sha(p))
            .unwrap_or_else(|| "MISSING".into()),
        ui_sha: git_sha(&ui),
        runner_sha256: file_sha256(&runner_bin),
        k_paper_bin_sha256: k_bin
            .as_ref()
            .map(|p| file_sha256(p))
            .unwrap_or_else(|| "MISSING".into()),
        layer: args.layer.clone(),
        config_hashes: Default::default(),
        capability_matrix: match layer {
            Layer::Paper | Layer::Ui => workflow_harness::Capability::paper(),
            Layer::DemoReadonly => workflow_harness::Capability::demo_readonly(),
            Layer::DemoMutating => workflow_harness::Capability::demo_mutating(),
        },
    };

    let scenarios = run_layer(layer, args.only.as_deref(), &sink.dir).await;
    let matrix = match finish_matrix(&sink, manifest, scenarios, vec![args.layer.clone()]) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("LAYER {}: FAIL (artifacts {e})", args.layer);
            std::process::exit(1);
        }
    };
    matrix.print_human();

    if args.expect_process_failure {
        let expected = !matrix.scenarios.is_empty()
            && matrix
                .scenarios
                .iter()
                .all(|s| s.expected_failure && s.verdict == Verdict::Fail);
        if expected {
            println!(
                "LAYER {}: PASS (expected scenario FAIL; negative control)",
                args.layer
            );
            println!("artifacts {}", matrix.artifact_dir.display());
            std::process::exit(0);
        }
        println!(
            "LAYER {}: FAIL (negative control did not fail as expected)",
            args.layer
        );
        println!("artifacts {}", matrix.artifact_dir.display());
        std::process::exit(2);
    }

    if matrix.overall_ok() {
        println!("LAYER {}: PASS", args.layer);
        println!("artifacts {}", matrix.artifact_dir.display());
        std::process::exit(0);
    }
    println!("LAYER {}: FAIL", args.layer);
    println!("artifacts {}", matrix.artifact_dir.display());
    std::process::exit(1);
}
