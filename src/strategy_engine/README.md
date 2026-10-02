# strategy_engine (future proposal)

This directory is **not** a Cargo workspace member. It is an unfinished IR
scaffold (`validate()` is `todo!()`, the runner is TODO-only). No other
workspace crate depends on it.

The active CEX strategy runner is `trading_engine` + `runner`. Do not compile
this crate until a later proposal lands. See `proposals/02-strategy-engine.md`
and `research/LIVE_STRATEGIES.md`.
