# Indicator Config Generator

## Problem / context

Build-time Python tooling compiles to `config/indicators.json`, loaded by `indicator_engine` at startup (Mongo first, then file). Levels use `"price": "Binding"` with `level_pricing` in the indicators config. See `python/indicator_config_generator/README.md` and `docs/LEVELS_GUIDE.md`.

## Decision

Implement in four vertical slices (MVP → composite → CLI/CI → Rust loader). **Strategy-specific indicators dropped** for this workstream.

1. **Load order:** MongoDB first, then `config/indicators.json`; **Mongo wins** on duplicate IDs (file fills gaps only).
2. **Naming contract:** IDs `{type}_{period}_{pair}` (e.g. `sma_20_BTCUSDT`); levels keep pair-agnostic `indicator_name: sma_20` resolved by existing levels resolver.
3. **Schema:** Shared `kind` discriminator (`simple` | `composite`) in JSON; Python pydantic + Rust `config.rs` aligned with `IndicatorExpression` serde tags.
4. **Composite:** `register_composite` + dependency propagation in `IndicatorCalculator`; Mongo + file loaders register composites after simples.
3. **Composite (slice 2):** Extend Python expression AST → JSON using existing `IndicatorExpression` schema; wire `register_indicator` / new `register_composite` in calculator with topological registration order.
4. **CLI/CI (slice 3):** `indicator-generate` / `indicator-validate` via uv; CI step fails on drift (`git diff --exit-code config/indicators.json`) after regen.
5. **Remove duplication (slice 4):** Drop hard-coded SMA/EMA loop in `init.rs` once JSON covers configured pairs; keep Mongo as optional overlay for API-added indicators only.

## Alternatives

- **Hand-written JSON only:** Fastest MVP, no Python; rejected—proposal goal is maintainable DSL and validation at generation time.
- **Rust proc-macro / build.rs codegen:** No Python dep; rejected—worse ergonomics for traders; Python workspace already exists.
- **Runtime Python embedding:** Rejected—violates no-runtime-Python principle.
- **Mongo as sole source of truth:** Already partially implemented; rejected as primary—file-based config is reviewable in git and matches levels/strategies layout.

## Consequences

- **Schema drift risk** between Python pydantic models and Rust serde remains; mitigated by golden JSON fixtures + Rust integration test that deserializes and registers.
- **Dual registration paths** (file, Mongo, legacy hard-code) during migration—must define precedence and log duplicates clearly.
- **Composite not in calculator** today—slice 2 is required before composite JSON is useful; MVP can ship without composite.
- **Strategy-specific indicators** (proposal dual-source) deferred to Proposal 02—avoid merging in `strategy_engine` until then.
- Build adds `uv sync` + generate step (~seconds); acceptable per proposal success criteria.

## Verification

- Python: unit tests for validators (unique IDs, missing deps, cycles, period > 0); snapshot test of generated JSON for a fixed pair list.
- Rust: `serde` round-trip test `config/indicators.json` fixture; integration test registers indicators and resolves `sma_20` for a pair after load.
- CI: `uv run indicator-validate` + regen check; `cargo test -p indicator_engine`.
- Manual: run bot with `config/levels.json` referencing `sma_20`; confirm level prices bind to loaded indicator values.

## Implementation plan

| Phase | Deliverable | Key files |
|-------|-------------|-----------|
| **1a** | Pydantic models + JSON envelope `{ "version": 1, "indicators": [...] }` | `python/indicator_config_generator/src/models.py`, `exporters.py` |
| **1b** | `SimpleIndicator` builder (SMA/EMA), pair batch helper | `builder.py` |
| **1c** | Validators (unique id, period, source enum) | `validators.py` |
| **1d** | Default generator script + committed `config/indicators.json` for current bot pairs | `generator.py`, `config/indicators.json` |
| **2** | Expression AST + composite export; topological sort | `expressions.py`, extend models |
| **2b** | Rust: `register_composite`, dependency-ordered load | `calculator.rs`, `service.rs`, new `config.rs` |
| **3** | CLI `indicator-generate` / `indicator-validate` | `cli.py` |
| **3b** | CI/Makefile target `generate-indicators` | root `Makefile` or `.github/workflows` |
| **4** | `IndicatorService::load_from_config(path)` in runner init; remove hard-coded SMA/EMA loop | `service.rs`, `runner/src/init.rs` |

**MVP exit criteria:** `levels.json` with `"indicator_name": "sma_20"` works without `init.rs` hard-coded registration; JSON is the documented source for system-wide simple indicators.

## Implementation results

- Python package `indicator_config_generator` (models, builder, validators, CLI entry points).
- `config/indicators.json` committed (BTCUSDT, ETHUSDT: `sma_20_*`, `ema_12_*`).
- Rust `config.rs` + `load_from_config` (skip IDs already in calculator).
- `load_from_mongo` loads simple + composite; runner init: Mongo → file; hard-coded SMA/EMA loop removed.
- Calculator: composite map, dependency index, propagate updates on simple indicator changes.
- `indicator-generate` upserts to MongoDB (`indicators` collection) when `config/bot.json` mongo settings exist, or via `--mongo-uri` / `--mongo-database`; use `--no-mongo` for file-only.

[cursor-hub workflow] step=design-review
