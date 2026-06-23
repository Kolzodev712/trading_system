//! Convention-based config paths per runner mode (`config/cex/` vs `config/dex/`).

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModeConfig {
    pub bot: String,
    pub levels: Option<String>,
    pub strategies: Option<String>,
    pub dex_levels: Option<String>,
    pub dex_strategies: Option<String>,
    pub dex_indicators: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModeConfigOverrides {
    pub bot: Option<String>,
    pub levels: Option<String>,
    pub strategies: Option<String>,
    pub dex_levels: Option<String>,
    pub dex_strategies: Option<String>,
    pub dex_indicators: Option<String>,
}

/// Resolve config file paths from `--mode`, with optional CLI overrides.
///
/// Layout (preferred):
/// - CEX modes → `config/cex/bot.json`, `config/cex/levels.json`, …
/// - DEX modes → `config/dex/bot.json`, `config/dex/levels.json` or `strategies.json`
///
/// Legacy flat paths under `config/` are still accepted as fallbacks.
pub fn resolve_mode_config(
    mode: &str,
    overrides: ModeConfigOverrides,
) -> Result<ResolvedModeConfig, String> {
    match mode {
        "dex" => Ok(ResolvedModeConfig {
            bot: overrides
                .bot
                .unwrap_or_else(|| first_existing(DEX_BOT_CANDIDATES)),
            levels: None,
            strategies: None,
            dex_levels: Some(
                overrides
                    .dex_levels
                    .unwrap_or_else(|| first_existing(DEX_LEVELS_CANDIDATES)),
            ),
            dex_strategies: None,
            dex_indicators: Some(
                overrides
                    .dex_indicators
                    .unwrap_or_else(|| first_existing(DEX_INDICATORS_CANDIDATES)),
            ),
        }),
        "dex-strategies" => Ok(ResolvedModeConfig {
            bot: overrides
                .bot
                .unwrap_or_else(|| first_existing(DEX_BOT_CANDIDATES)),
            levels: None,
            strategies: None,
            dex_levels: None,
            dex_strategies: Some(
                overrides
                    .dex_strategies
                    .unwrap_or_else(|| first_existing(DEX_STRATEGIES_CANDIDATES)),
            ),
            dex_indicators: Some(
                overrides
                    .dex_indicators
                    .unwrap_or_else(|| first_existing(DEX_INDICATORS_CANDIDATES)),
            ),
        }),
        "levels" => Ok(ResolvedModeConfig {
            bot: overrides
                .bot
                .unwrap_or_else(|| first_existing(CEX_BOT_CANDIDATES)),
            levels: Some(
                overrides
                    .levels
                    .unwrap_or_else(|| first_existing(CEX_LEVELS_CANDIDATES)),
            ),
            strategies: None,
            dex_levels: None,
            dex_strategies: None,
            dex_indicators: None,
        }),
        "strategies" => Ok(ResolvedModeConfig {
            bot: overrides
                .bot
                .unwrap_or_else(|| first_existing(CEX_BOT_CANDIDATES)),
            levels: None,
            strategies: Some(
                overrides
                    .strategies
                    .unwrap_or_else(|| first_existing(CEX_STRATEGIES_CANDIDATES)),
            ),
            dex_levels: None,
            dex_strategies: None,
            dex_indicators: None,
        }),
        other => Err(format!(
            "unknown mode `{other}` (expected levels, strategies, dex, or dex-strategies)"
        )),
    }
}

const CEX_BOT_CANDIDATES: &[&str] = &["config/cex/bot.json", "config/bot.json"];
const CEX_LEVELS_CANDIDATES: &[&str] = &["config/cex/levels.json", "config/levels.json"];
const CEX_STRATEGIES_CANDIDATES: &[&str] =
    &["config/cex/strategies.json", "config/strategies.json"];
const DEX_BOT_CANDIDATES: &[&str] = &[
    "config/dex/bot.json",
    "config/bot.dex.local.json",
    "config/bot.dex.example.json",
];
const DEX_LEVELS_CANDIDATES: &[&str] =
    &["config/dex/levels.json", "config/dex_levels.example.json"];
const DEX_STRATEGIES_CANDIDATES: &[&str] = &["config/dex/strategies.json"];
const DEX_INDICATORS_CANDIDATES: &[&str] = &["config/dex/indicators.json"];

fn first_existing(candidates: &[&str]) -> String {
    for path in candidates {
        if Path::new(path).is_file() {
            return (*path).to_string();
        }
    }
    candidates[0].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dex_mode_uses_dex_directory_by_default() {
        let resolved = resolve_mode_config("dex", ModeConfigOverrides::default()).unwrap();
        assert_eq!(resolved.bot, "config/dex/bot.json");
        assert_eq!(
            resolved.dex_levels.as_deref(),
            Some("config/dex/levels.json")
        );
    }

    #[test]
    fn dex_strategies_mode_uses_strategies_file() {
        let resolved =
            resolve_mode_config("dex-strategies", ModeConfigOverrides::default()).unwrap();
        assert_eq!(resolved.bot, "config/dex/bot.json");
        assert_eq!(
            resolved.dex_strategies.as_deref(),
            Some("config/dex/strategies.json")
        );
    }

    #[test]
    fn levels_mode_uses_cex_directory_by_default() {
        let resolved = resolve_mode_config("levels", ModeConfigOverrides::default()).unwrap();
        assert_eq!(resolved.bot, "config/cex/bot.json");
        assert_eq!(resolved.levels.as_deref(), Some("config/cex/levels.json"));
    }

    #[test]
    fn overrides_take_precedence() {
        let resolved = resolve_mode_config(
            "dex",
            ModeConfigOverrides {
                bot: Some("/tmp/bot.json".into()),
                dex_levels: Some("/tmp/levels.json".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(resolved.bot, "/tmp/bot.json");
        assert_eq!(resolved.dex_levels.as_deref(), Some("/tmp/levels.json"));
    }
}
