# Production platform readiness (shipped)

## Problem / context

Venue split and `dex_engine` scaffold are shipped, but the platform is inconsistent: in-process trading uses `trading_core` broadcast channels while `trading_networking` (Zenoh) and `trading_trace` only cover the CEX path in levels/strategies runners. DEX has no trace context, no external event lane, and `both` mode lacks documented semantics and unified health. Solana must not start until this layering is stable.

## Decision

Implemented in four phases **before** Solana (now [Proposal 01](../../proposals/01-solana-integration.md)):

1. **Phase A** — Docs (`PLATFORM_DATA_PLANE` / venue modes); DEX `ColdTraceContext` on `DexVenueUpdate`; `dex_engine` spans; **`runner::event_bridge`** centralizing MD→`HotEvent`, CEX→`ColdEvent`, DEX→new **`DexColdEvent`** when `ZENOH_ENABLED=1`; remove duplicate bridges from level/strategy runners; bridge also used from `dex_runner` when applicable.
2. **Phase B** — Stricter `BotConfig::validate()` (venue × runner mode matrix); example config CI parse; migrate imports to `venue::cex` with shim deprecation grep.
3. **Phase C** — Unified `/health` (CEX-critical when `cex`/`both`); lag + dex execute metrics; shared monitoring snapshot builder for gRPC.
4. **Phase D** — Config/smoke tests, shutdown audit for `both`, k-paper levels regression checklist.

**Defer:** Mongo optional tier and prod fail-fast trace (document only in D1 unless explicitly approved later).

## Alternatives

- **Internal bus = `trading_networking`** — Rejected: latency, serialization, CEX regression risk.
- **Skip DEX platform parity until Solana** — Rejected: duplicates work under chain integration pressure.
- **Single merged venue event enum** — Rejected: breaks CEX/DEX split.
- **Extend `ColdEvent` with DEX variants instead of `DexColdEvent`** — Rejected for now: keeps CEX wire schema stable; revisit if Zenoh consumers want one topic schema.

## Consequences

- One bridge module reduces drift between levels/strategies/dex runners.
- `DexColdEvent` adds a third cold-lane type; Zenoh transport may need a third publish path or multiplexed topic.
- Health 503 rules must be conservative to avoid false negatives on Paper reconnect blips (tune thresholds).
- Import migration is churn without behavior change; do per crate to limit review size.

## Verification

- `venue.mode: cex` + `--mode levels`: existing integration tests + manual k-paper unchanged.
- `ZENOH_ENABLED=1`: CEX and DEX events observable on Zenoh (manual or integration stub).
- Jaeger: spans for `dex.bootstrap` / `dex.execute` with observability compose.
- `cargo test` config validator + `DexColdEvent` payload roundtrip.
- `just clippy` clean; `/health` returns 503 when CEX WS down (simulated or mocked).
- Proposal 02 prerequisite checklist passes without reopening layering.

[cursor-hub workflow] step=design-review

## Implementation results

- **Phase A:** `docs/PLATFORM_DATA_PLANE.md`; README Zenoh wording fixed; `DexVenueUpdate` + `ColdTraceContext`; `dex_engine` spans (`dex.bootstrap`, `dex.execute`, `dex.stream_updates`); `DexColdEvent` + `runner::event_bridge`; Zenoh `dex_cold/*` keys; duplicate bridges removed from level/strategy runners; dex mode wired.
- **Phase B:** `BotConfig::validate` / `validate_for_runner`; `docs/VENUE_MODES.md`; config smoke tests; partial import migration (`gateway`, `exchange_engine::run` → `venue::cex`).
- **Phase C:** `PlatformReadiness` + `/health` 503; `platform_metrics` (`venue_mode`, `dex_execute_*`, `broadcast_lag_total`); `dex_engine::monitoring::collect_dex_snapshot` for gRPC.
- **Phase D:** CI job `platform-config-smoke`; shutdown via `event_bridge` + existing `ShutdownManager` on all bridge tasks.
- **Verification:** `cargo test -p runner --lib`, `cargo test -p trading_networking`, integration suite (46 passed). Mongo optional tier documented only (unchanged behavior).

**Cleanup (follow-up):** Migrated `src/` + `tests/` off `trading_core::traits` and `channels::exchange` to `venue::cex::{traits,channels}`; added `just check-venue-imports` + CI gate; shims kept at `traits/` and `channels/exchange.rs` for external callers only.
