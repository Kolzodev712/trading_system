//! Shared helpers for DeepBook setup scripts (`create_balance_manager`, `deposit_to_balance_manager`).

use std::path::{Path, PathBuf};

use dex_engine::config::DexEngineConfig;
use dex_engine::DeepBookConfig;
use trading_core::venue::VenueId;

pub fn load_deepbook_config(path: &Path) -> Result<DeepBookConfig, Box<dyn std::error::Error>> {
    let dex_engine = load_dex_engine_config(path)?;
    DeepBookConfig::from_dex_engine(&dex_engine)
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })
}

pub fn load_deepbook_config_for_trading(
    path: &Path,
) -> Result<DeepBookConfig, Box<dyn std::error::Error>> {
    let deepbook = load_deepbook_config(path)?;
    deepbook.validate_for_trading()?;
    Ok(deepbook)
}

fn load_dex_engine_config(path: &Path) -> Result<DexEngineConfig, Box<dyn std::error::Error>> {
    let bot: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let dex_engine: DexEngineConfig = serde_json::from_value(
        bot.get("dex_engine")
            .cloned()
            .ok_or("config missing dex_engine section")?,
    )?;
    if dex_engine.venue != VenueId::DeepBook {
        return Err("dex_engine.venue must be deepbook".into());
    }
    Ok(dex_engine)
}

pub fn parse_config_path(default: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut path = PathBuf::from(default);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                path = PathBuf::from(args.next().ok_or("missing value for --config")?);
            }
            "--help" | "-h" => return Err("help".into()),
            other if other.starts_with('-') => {
                return Err(format!("unknown flag `{other}` (try --help)").into());
            }
            other => {
                path = PathBuf::from(other);
            }
        }
    }
    Ok(path)
}
