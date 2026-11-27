# Trading report v1: five-item decision brief

**Status:** implemented for flat Paper Spot collect, plus Mongo persist/`read-json` and a one-shot 15-minute timer.  
**Source baseline:** `trading_system` `d2d13bc` (2026-10-02) plus this scheduling change.  
**First use:** a person runs a read-only command and copies its Markdown report into a conversation. Scheduled Mongo delivery stores that same JSON; `read-json` is suitable for a later AI consumer. There is no AI API call or automatic trading decision.

## Goal and boundary

Build one `trading_report` workspace crate with a library and a one-shot CLI. It takes snapshots from separately running Levels and Strategies processes, independent venue REST, and a small operator mandate. It writes a machine-readable JSON report and a Markdown rendering of the same facts. The report answers only the five questions below. It **does not** generate a trade recommendation, calculate alpha, place/cancel orders, change configuration, or authorize exposure.

The report is a read-only observer. Runner and trading engine keep their existing economic authority. Journal and ledger remain durable execution records; venue REST remains the independent account check. The reporting crate is not a production control plane and is never imported by `runner`, `trading_engine`, `gateway`, `strategy_core`, `research_engine`, `research_cli`, or `btc_price_ma_app`.

One report can contain two runner subjects. Each subject is evaluated separately by account, venue, environment, instrument and execution mode. A combined view must not imply that two independent runners share a risk allocator. If source/account identity is ambiguous, say so and withhold an actionable verdict.

## The five mandatory items

| # | Question | Minimum content | Primary source and validation |
| --- | --- | --- | --- |
| 1 | Where and when? | Report UTC time, source observation times, runner label/mode, venue, environment, account alias, instrument, executable revision if supplied, and config identity. | `GetTradingStatus` + `GetMonitoringData`; reporter configuration binds endpoints, account aliases and read-only venue credentials. Detect mismatch with configured identity. Do not invent a build SHA or pretend an alias was venue-verified. |
| 2 | What is the market doing? | Best bid/ask, spread, depth relevant to the configured instrument, source book timestamp and calculated age; completed-candle fields when the runner exposes them or the selected trading rule requires them. | Monitoring orderbook and `StrategyStatus` completed-candle fields. An absent or stale book is unavailable, not a zero-price book. A Levels playbook requiring additional candles must supply them through an explicit read-only source or the report is not actionable for that playbook. |
| 3 | What exposure exists? | Independent venue working orders and Spot balances or Futures positions, alongside runner orders/positions, with observed-at times and explicit differences. | A read-only venue adapter makes fresh, complete REST requests. For Spot, base inventory comes from balances; an unsupported `/positions` response is not proof of flatness. Futures uses REST positions. Open-order pagination must finish; a partial page walk is unverified. Never substitute the runner cache for a failed venue read. |
| 4 | What is the system trying and allowed to do? | Per-runner global enable, entries paused, flatten latch, exposure halt/reason; strategy signal, target, model reconstruction and entry freshness where present; Levels IDs, enabled state, price/size, owned orders and fill count; effective engine caps and usage, plus the operator's stated risk mandate. | `GetTradingStatus`, `GetMonitoringData`, `GetLevels` in Levels mode, and a narrow read-only risk snapshot added at the runner/gateway boundary. The mandate is user supplied and labelled *advisory* unless also enforced by the runner. Never recompute the engine's pre-trade decision in the reporter. |
| 5 | Can this snapshot be trusted? | Per-source success/age, venue-versus-runner comparison, unresolved submission count/state, relevant halt/reconciliation reasons and a verdict: `verified`, `stale`, `unverified`, or `disagreement`. | Check gRPC, complete venue REST, book timestamps and the latest journal state per client order ID. Any required source error, inconsistent identity, missing risk information, stale required market input or unresolved exposure prevents an actionable verdict. Include specific reason codes and source errors. |

The Markdown view must preserve the same verdict and uncertainty as JSON. Its headings follow these five items. Both views name the exact instrument/account they cover. When multiple subjects are present, show each subject's verdict and an overall worst-case verdict; never net unrelated accounts or base/quote assets into a single position number.

## Existing data and the small gaps

- `libs/trading_proto/proto/trading_cli.proto` already defines `GetTradingStatus`, `GetMonitoringData`, `GetLevels`, `StrategyStatus`, `Orderbook`, balances, orders and positions. Use typed gRPC calls directly. Do **not** shell out to `grpc_ctl status`: its display path turns a missing monitoring payload into `unwrap_or_default()` and ignores `GetLevels` failures.
- `src/gateway/src/grpc/service/monitoring.rs` assembles runner state and economic fields. It currently filters displayed open orders to the last 24 hours. A still-working older order must remain visible in the reporting path. Remove that display filter for open orders or expose an unfiltered read-only working-book view, with a regression test.
- Monitoring currently converts failures to empty indicator/trade/orderbook lists and writes an empty PnL string if ledger replay fails. The reporter must distinguish unavailable from genuinely empty. For mandatory book or risk fields, add a narrow completeness/health signal at the source if absence cannot otherwise be distinguished; do not infer success from a timestamp assigned after collection.
- `RiskManager::get_limits()` exists, but effective Limits/usage are not part of `MonitoringData`. Add a read-only `GetRiskSnapshot` (or equivalent monitoring field) sourced from the **live** RiskManager and trading state, including dynamic config updates, working/reserved exposure and relevant daily-loss state. The reporter displays caps and usage; the engine still decides whether an order is permitted. If a precise headroom figure cannot be exposed, report caps and usage and label headroom `unknown` rather than duplicating risk formulas.
- `src/trading_engine/src/cex/state/submission.rs` has append-only JSONL with latest row per client ID and unresolved states `Prepared`, `Submitted`, `Unknown`, `RecoveryRequired`. Read a minimal, strict projection (`client_order_id`, ownership, state and timestamps) through the path configured for **that** runner; fail on an unknown state or malformed row. Test the projection against engine-produced fixtures. Never concatenate two containers' files or treat a missing file as an empty, known journal. `ledger.rs` remains the durable fill record, but a separate ledger parser is outside these five v1 items; monitoring already reports economic fields where available.
- The existing event bridge handles hot and cold updates but is not a durable report source. V1 pulls current state and durable files on demand. No new bus, per-crate event hooks, daemon or streaming transport is required.
- Research CLI already writes `gates.json` and `experiment_report.md`. V1 may show the live strategy's `research_disposition`, `strategy_version` and `config_hash` from `StrategyStatus`. It does not parse full research runs or make research crates depend on the reporting crate.

## Crate layout and dependencies

Suggested layout (names may follow workspace conventions):

```text
src/trading_report/
  Cargo.toml
  src/lib.rs
  src/main.rs              # collect command, JSON + Markdown outputs
  src/config.rs            # runner/venue sources, account aliases, freshness, mandate
  src/model.rs             # versioned report DTO; typed availability/verdict
  src/runner_source.rs     # read-only gRPC client, one endpoint per runner
  src/venue_source.rs      # read-only Paper / Bybit Demo REST adapters
  src/journal_source.rs    # strict latest-state projection, one runner's JSONL
  src/verify.rs            # identity, freshness, venue/runner comparison
  src/render.rs            # Markdown rendering of the same DTO
```

`trading_report` can use the canonical `trading_cli.proto` client generated from the same source as `cli`; do not fork the schema. It may use neutral `trading_core` types and shared parsing/signing facilities where appropriate. It must not depend on `runner`, `trading_engine`, `research_engine` or the Candidate #2 app. The small journal projection above is a read-only schema contract, not a second recovery implementation. If an existing exchange adapter exposes reads only through a mutation-capable trait, keep that dependency behind a local `VenueSnapshotReader` interface and give the report process read-only venue credentials. Never import the workflow harness as a runtime control plane. Start with the operationally relevant Paper and Bybit Demo CEX Spot/linear read paths; unsupported venues are explicit `unverified`, not simulated.

The only production-side change is the narrow read-only risk/completeness/working-order exposure needed above. Wire it through `trading_core` proto, `gateway` monitoring/control service and `runner` construction. Do not add a dependency edge from live crates into reporting or research crates. No k-paper or UI change is required to render the first report.

## One-shot data flow

1. Load a reporting config containing named runner gRPC endpoints, account/instrument binding, venue read-only endpoint/credential reference, explicit freshness thresholds, journal path per runner and an operator mandate (allowed instruments/direction/leverage, max exposure and loss per trade/day, time horizon). Config rejects duplicate labels, ambiguous account binding and missing mandatory thresholds. Secrets are never placed in reports.
2. Call each runner's `GetTradingStatus`, `GetMonitoringData` and, in Levels mode, `GetLevels`; obtain the read-only risk snapshot. Keep each call's success/error and observation time. Require known venue/environment/mode; never use protobuf defaults as valid data.
3. Independently query full venue working orders plus Spot balances or Futures positions for each bound account/instrument. If the REST list is paginated, walk to completion and deduplicate identities. A REST timeout, unsupported endpoint or remaining cursor is an error, not `[]`.
4. Read the latest journal rows for each runner. Join by client ID/venue order ID only where correlation is certain; unresolved rows remain visible. Compute freshness and comparisons using decimal quantities, normalized symbol/side and explicit tolerances justified by venue precision. Reuse the platform's seconds/milliseconds normalization for orderbook age. No trade decision is made here.
5. Build a versioned immutable report in memory. Produce `report.json` and `report.md` atomically in an isolated output directory, plus a small manifest with source revisions/config hashes if available. An `unverified` report can still be rendered for diagnosis but must not be marked successful/actionable. No raw bot config, API key, operator token or full unsanitized log is copied to the output.

Example invocation, **proposed command, not yet implemented**:

```text
trading-report collect --config report-config.toml --out reports/2026-10-01T153000Z
```

The JSON report has `schema_version`, `generated_at_utc`, `subjects[]`, each of the five sections, `verdict`, `actionable_for_analysis` and `reasons[]`. Use decimal strings and `null`/availability tags for unknown values; do not turn missing data into zero. `actionable_for_analysis` means the brief has enough verified input for a human/AI analysis, **not** that the runner has approved an order or that the strategy has alpha.

## Verification and completion

One implementation pass is sufficient; no staged approval between small edits. Before calling it complete:

1. Unit-test report schema, missing/default fields, freshness, decimal/symbol normalization, complete versus truncated venue orders, Spot-wallet and Futures-position truth, journal latest-state handling, and per-subject verdict aggregation.
2. Run a two-runner Paper process test (Levels Exchange plus Strategies Shadow is sufficient) with separate gRPC ports/runtime paths. Compare independent Paper REST to the report. Assert no venue mutations originate from the reporting process.
3. Force Paper `/open_orders` failure and a stale book; both produce a rendered but non-actionable report with the right reason. Make a still-working order older than 24 hours visible, and confirm a dynamic risk-limit change is reflected. Missing `GetLevels`, missing monitoring data, or a mismatched venue/account binding must not become `[]` or `0`.
4. Run Bybit Demo **read-only** with a read-only credential and compare the signed REST snapshot to the report. No `Resume`, order create, cancel, flatten or Demo mutation is part of this gate. If that source is not configured, report it as unsupported/unverified and qualify Paper scope only.
5. Inspect the JSON, Markdown and manifest for secrets. Verify `cargo tree` has no `trading_report` edge in runner/trading_engine and no research-crate edge introduced into them. Record commands, results, source SHA and any unsupported venue.

## Mongo delivery (implemented)

Scheduled collection writes the existing `report.json` / `report.md` / `manifest.json` files and one immutable Mongo document that embeds the **exact** `report.json` UTF-8 bytes plus indexed metadata (`report_id`, `deployment_id`, timestamps, schema, verdict, `actionable_for_analysis`, subject identifiers, SHA-256). It does not reserialize the five-item report into a different JSON shape. File-only `collect` remains for development. A scheduled run is not delivered until Mongo accepts the write. `trading-report persist --from <dir>` replays a leftover directory without recollecting. `trading-report read-json` uses Mongo only.

See [TRADING_REPORT_OPERATOR.md](TRADING_REPORT_OPERATOR.md).

## Explicitly deferred

No model API calls, UI report viewer, continuous in-process daemon, alert subscriptions, every-book-tick capture, separate ledger replay, full research artifact ingestion, strategy optimization, discretionary signal generation, order submission or config mutation. Do not turn reporting into a new event-sourcing or control-plane project.

## Effort estimate and stop condition

The narrow Paper-first report is roughly **4–7 engineer-days** for a developer familiar with the workspace, including the read-only risk/completeness fix and Paper negative controls. Including Bybit Demo signed read-only comparison and its credential/provenance handling is roughly **6–10 engineer-days total**. This replaces the broader event-driven/AI scope previously estimated at multiple weeks; it is not a guarantee of calendar time. If a required field cannot be obtained reliably, stop with a specific `unverified` reason rather than add unrelated infrastructure or claim the five items are present.
