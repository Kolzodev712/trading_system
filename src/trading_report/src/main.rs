use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use trading_report::{collect_report, write_outputs, ReportConfig};

#[derive(Parser)]
#[command(name = "trading-report", about = "Read-only five-item trading report")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Collect {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Collect { config, out } => {
            let cfg = ReportConfig::load(&config)?;
            let outputs = collect_report(&cfg).await.context("collect report")?;
            write_outputs(&out, &outputs)?;
            println!(
                "wrote {} (verdict={} actionable_for_analysis={})",
                out.display(),
                outputs.report.verdict.as_str(),
                outputs.report.actionable_for_analysis
            );
            Ok(())
        }
    }
}
