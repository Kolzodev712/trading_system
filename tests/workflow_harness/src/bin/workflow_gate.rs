use clap::Parser;
use workflow_harness::artifacts::{
    file_sha256, finish_matrix, git_sha, utc_now, ArtifactSink, Manifest,
};
use workflow_harness::process::{k_paper_dir, workspace_root};
use workflow_harness::scenarios::run_layer;
use workflow_harness::types::{Layer, Verdict};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "paper")]
    layer: String,
    #[arg(long)]
    only: Option<String>,
    /// When set, a FAIL from the selected scenario is the success path (negative control).
    #[arg(long)]
    expect_process_failure: bool,
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
        ui_sha: git_sha(&ws.parent().unwrap().join("trading-system-ui")),
        runner_sha256: file_sha256(&runner_bin),
        k_paper_bin_sha256: k_bin
            .as_ref()
            .map(|p| file_sha256(p))
            .unwrap_or_else(|| "MISSING".into()),
        layer: args.layer.clone(),
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
        let failed = matrix.scenarios.iter().any(|s| s.verdict == Verdict::Fail);
        if failed {
            println!("LAYER {}: FAIL (expected; negative control)", args.layer);
            std::process::exit(1);
        }
        println!("LAYER {}: FAIL (negative control did not fail)", args.layer);
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
