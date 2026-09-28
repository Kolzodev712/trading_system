# Prop Trading Productionization and Research Architecture Plan

**Status:** Increments 0–9 are accepted locally. Increment 9A's remaining section 49 faults, N=28 decision comparison, grpc CancelWorking and browser CancelWorking are now reported locally and wait owner re-review. Increment 10 / Bybit demo stays blocked. Candidate #2 remains an operational baseline, not validated alpha.  
**Date:** 2026-09-28  
**Repository:** `trading_system`  
**Audience:** Cursor / implementation agent / repository owner  
**Primary objective:** turn the existing trading platform into an operational prop-trading system while keeping strategy research reproducible, independently verifiable, and architecturally separated from live execution.

---

# 1. Why this document exists

The repository now contains enough working infrastructure to do two different jobs:

1. run a real trading platform against a CEX; and
2. perform historical strategy research.

Those two responsibilities have started to become mixed inside `trading_engine`.

That was acceptable while proving the first research workflow, but it is not the desired long-term architecture.

The next stage is therefore **not simply "add Candidate #2 to the live factory."**

The next stage is:

1. separate pure strategy logic from both research and live execution;
2. move historical research/backtesting into its own crate(s);
3. keep independent audit logic outside both production and research implementations;
4. productionize Candidate #2 as the first operational strategy;
5. run Candidate #2 in Shadow and expose its state through the separate `trading-system-ui` repository;
6. smoke-test the existing Paper adapters, then finish the remaining CEX correctness/risk/recovery gaps;
7. exercise the actual runner, risk, executor, exchange and market-data adapters against `k-paper` as a controlled futures/fault laboratory;
8. add the corresponding operator safety workflows to `trading-system-ui`;
9. only after the local full-stack gates pass, run Candidate #2 on Bybit demo;
10. continue research for Candidate #3 in parallel;
11. do not open the sealed 2026 holdout until a future candidate passes its frozen development screen.

This file is the implementation roadmap and the source of truth for the order in which that work should happen.

---

# 2. High-level mission

The goal is not merely:

> "make a strategy place orders."

The goal is:

> Build a trading platform in which a strategy hypothesis can be researched, independently checked, deployed into a controlled live runtime, observed against real execution, and rejected or promoted without changing the meaning of the strategy between research and production.

The long-term flow should be:

```text
Research hypothesis
        |
        v
Frozen strategy specification
        |
        v
Pure strategy model
        |
        +------------------------------+
        |                              |
        v                              v
Research engine                  Trading runtime
historical replay                live completed data
execution simulation             account state
fees / funding                   risk
benchmarks                       execution
gates                            reconciliation
        |                              |
        v                              v
Independent audit                Shadow -> K-paper -> Demo -> Live
```

The execution path in the right branch is operated through the separate browser `trading-system-ui`. It receives authoritative runner state and issues runner commands through gRPC-Web; it does not evaluate research candidates or trade directly with either venue. `k-paper` is a separate local exchange server for full-stack testing, not the historical research engine.

The system should eventually make it difficult to accidentally confuse:

- a profitable historical result,
- a validated edge,
- a correctly implemented signal,
- a production-ready strategy,
- and a safely executable strategy.

Those are different claims and should remain different states in code and operations.

---

# 3. Current research status

Two strategy candidates have been formally tested.

## 3.1 Candidate #1 — DonchianDailyV1

Frozen research definition:

- Bybit BTCUSDT USDT-linear perpetual
- completed daily candles
- 20-day entry channel
- 10-day exit channel
- long and short
- fixed 0.001 BTC
- strict breakouts
- no pyramiding
- no same-candle reversal
- initially flat after warmup

Independent reconstruction confirmed the 29 round trips were actually implied by the candle data.

Development period:

```text
2023-01-21T00:00:00Z
through
2026-01-01T00:00:00Z exclusive
```

Key results:

```text
price PnL before funding:       -29.2864 USDT
funding:                         -8.971378711447 USDT
zero-cost net:                  -38.257778711447 USDT
Cost A net:                     -41.402876785815 USDT
```

Disposition:

```text
REJECTED
```

Meaning:

The trading rule itself lost money in the development period. Fees were not the primary explanation.

Candidate #1 remains frozen as a rejected baseline.

Do not:
- modify its parameters;
- add filters;
- wire it live;
- reopen its research;
- reinterpret its result.

Existing independent audit:

```text
audit/donchian_daily_v1/reference_check.py
```

---

## 3.2 Candidate #2 — BTCPriceMALongFlatV1

Frozen primary research definition:

- Bybit BTCUSDT USDT-linear perpetual
- long / flat only
- fixed 0.001 BTC
- N = 28 primary
- N = 14 and N = 70 diagnostics
- completed daily trade-price candles
- signal:

```text
log(close_t) - mean(log(last N completed closes including close_t))
```

Rule:

```text
spread > 0:
    target +0.001 BTC

spread <= 0:
    target 0
```

No:
- shorting;
- pyramiding;
- stop loss;
- take profit;
- funding filter;
- volatility filter;
- parameter search.

Development period is the same 2023-2025 period used above.

Primary Cost A result:

```text
gross price PnL:       +37.07496940 USDT
fees:                   -4.382487175704 USDT
funding:               -12.0804120949901 USDT
net:                   +20.6120701293059 USDT
max sampled drawdown:   30.6689062419564 USDT
completed trades:       55
```

Fixed +0.001 BTC perpetual comparator:

```text
net:                    +48.360542973936 USDT
max sampled drawdown:    44.3887151092231 USDT
```

The strategy therefore made money and reduced drawdown relative to full passive exposure.

However, it used materially less market exposure.

Independent Cost A notional-exposure comparison:

```text
candidate notional exposure sum:  932903.30905
fixed-long exposure sum:          1712235.90266

f ~= 0.544845080984876
```

A passive position scaled to the same average notional exposure would have produced approximately:

```text
+26.3490039531067 USDT
```

Candidate #2 produced:

```text
+20.6120701293059 USDT
```

Therefore Candidate #2 failed to demonstrate that its timing decisions added enough value beyond simply holding a smaller passive BTC exposure.

Frozen development gates:

| Gate | Result |
| --- | --- |
| Cost A net positive | pass |
| retain at least 80% of fixed-long profit | **fail** |
| drawdown <= 70% of full benchmark drawdown | pass |
| beat notional-matched passive | **fail** |
| higher-cost net positive | pass in stored result |
| delayed-execution net positive | pass in stored result |
| at least 20 completed round trips | pass |
| N=14 Cost A positive | pass |
| N=70 Cost A positive | pass |

Research disposition:

```text
REJECTED AS VALIDATED ALPHA
```

This disposition must not be changed.

Candidate #2 is **not** being promoted because it proved a superior trading edge.

It is being chosen as the first operational production strategy because it is:

- deterministic;
- simple;
- long/flat;
- slow;
- based on completed daily candles;
- independently audited;
- profitable over the development period;
- easy to reason about operationally;
- useful for exercising the complete production path.

That means Candidate #2 may be:

```text
OPERATIONALLY DEPLOYED
```

while still being:

```text
REJECTED AS VALIDATED ALPHA
```

Those statuses must remain separate.

Independent audit:

```text
audit/btc_price_ma_long_flat_v1/reference_check.py
```

Audit outputs:

```text
data/btc_price_ma_long_flat_v1/audit/zero_cost_and_cost_a/
```

The audit independently matched:

- N=14 decisions;
- N=28 decisions;
- N=70 decisions;
- primary zero-cost fills/funding/trades/PnL;
- primary Cost A fills/funding/trades/PnL;
- N=14 Cost A;
- N=70 Cost A;
- fixed-long Cost A;
- the economic gates derived from those ledgers.

One non-economic floating-point diagnostic difference was observed in an N=70 geometric-mean display value at approximately 1e-11 USDT scale. It did not change spread sign, direction, target, fill, or economic result.

---

# 4. Holdout policy

The 2026 holdout has not been opened for either development redesign.

Keep it sealed.

Do not run Candidate #2 on the research holdout merely because it is being productionized.

Live/shadow data arriving after deployment is separate operational evidence.

A future Candidate #3 may access the frozen holdout only after:

1. the candidate hypothesis is written;
2. exact rules are frozen;
3. benchmarks are frozen;
4. economic rejection gates are frozen;
5. the implementation is complete;
6. the 2023-2025 development test passes;
7. the development result is independently reconstructed.

Do not repeatedly inspect the holdout while designing a strategy.

---

# 5. Architectural decision: separate research from the live trading engine

## 5.1 Current problem

Before Increments 1–5, research logic lived under paths such as:

```text
src/trading_engine/src/cex/strategies/rust_strategies/
    btc_price_ma_long_flat_v1.rs
    btc_price_ma_long_flat_v1/offline.rs
    btc_price_ma_long_flat_v1/experiment.rs
    btc_price_ma_long_flat_v1/gates.rs

    donchian_daily_v1/offline.rs
    donchian_daily_v1/backtest.rs
    donchian_daily_v1/aux.rs
```

That mixed:

- strategy math;
- offline replay;
- historical datasets;
- execution simulation;
- funding;
- fees;
- benchmarks;
- research gates;
- reporting;

with the live trading engine.

The frozen model/evaluator/CLI were extracted. Increment 5R then removed the direct `trading_engine` dependencies on `strategy_core` and `research_engine` and the `runner` dependency on `research_cli`; focused tests and a Cargo tree check must still establish the full boundary and Shadow parity.

---

# 6. Target architecture

Use the same repository and Cargo workspace for now.

Do **not** create a second repository yet.

Crate boundaries provide enough separation while preserving:

- atomic commits;
- one CI graph;
- exact model version parity;
- simple refactors;
- shared data types;
- no cross-repository dependency/version coordination.

A future separate research repository may make sense if the research platform becomes independently large, e.g. notebooks, Parquet lakes, distributed jobs, ML tooling, feature stores, experiment databases, etc.

That is not necessary yet.

Target conceptual structure:

```text
trading_system/
|
|-- libs/
|   |-- trading_core/
|   |-- trading_networking/
|   |-- trading_trace/
|   `-- strategy_core/                 EXTRACTED
|
|-- src/
|   |-- exchange_engine/
|   |-- market_data_engine/
|   |-- trading_engine/
|   |-- indicator_engine/
|   |-- asset_engine/
|   |-- gateway/
|   |-- btc_price_ma_app/              NEW; thin composition binary and adapter
|   `-- runner/
|
|-- research/
|   |-- research_engine/               EXTRACTED
|   `-- research_cli/                  EXTRACTED
|
|-- audit/
|   |-- donchian_daily_v1/
|   `-- btc_price_ma_long_flat_v1/
|
`-- data/
```

Exact directory naming may be adjusted to fit workspace conventions, but the dependency boundaries are mandatory. The Candidate #2 composition binary links the existing generic `runner` library, `trading_engine`'s `Strategy` trait and `strategy_core` **in one process**. It supplies a Candidate #2 strategy adapter/factory to a generic runner injection point. The runner and `trading_engine` crates must not depend directly **or transitively** on `strategy_core`, `research_engine`, `research_cli` or the new app crate. Research binaries never link the live runner/engine. Shared low-level types such as `CompletedCandle` can stay in `trading_core`.

The existing runner binary continues to serve existing modes/strategies. It must give a clear unsupported-strategy error if invoked with Candidate #2 without the composition binary. No second process, IPC protocol, dynamic plugin loader or separate control plane is required for this dependency boundary.

## 6.1 External repositories and responsibility

| System | Owns | Does not prove |
| --- | --- | --- |
| `strategy_core` / `research_engine` in `trading_system` | Frozen model and fast historical economic evaluation | That the live execution stack works |
| Candidate #2 composition app in `trading_system` | Builds the concrete adapter and injects it into the generic runner in the same process | Independent risk or exchange authority |
| `k-paper` (separate repo) | Local exchange state, market/private streams, orders/fills and controllable test scenarios | Alpha, Bybit API parity or real futures economics without further implementation |
| `trading_system` runner | Authoritative strategy, risk, order, reconciliation, readiness and control state; source proto/gRPC contract | That a browser's local view is authoritative |
| `trading-system-ui` (separate repo) | Browser presentation, operator workflow, authentication UI and deliberate confirmations | Exchange state or recovery decisions of its own |
| Bybit demo | Integration against Bybit's actual demo API and event behavior | Real-money economics |

The existing paper path is real integration, not a stub: `market_data_engine` connects to `k-paper` `/ws/market`, `exchange_engine` uses `/ws/trade` and REST balance/open-order/trade snapshots, and `asset_engine` reads `/exchange_info`. These feed the normal runner and exchange-state manager. The current `sample_config/k-paper/bot.json` is a **Spot** example and the local development guide exercises levels mode. Do not treat that smoke path as an already working BTCUSDT linear-perpetual Candidate #2 path.

The browser path is `trading-system-ui` -> gRPC-Web proxy (currently Envoy) -> runner `TradingCliService`. The UI's MongoDB-backed auth service is for its operator login. The UI does not read `bot.json` or connect to `k-paper` or Bybit directly. The old Rust `cli`/TUI remains a separate client; references to the Trading System UI in this plan mean the **browser repository**.

## 6.2 Current capabilities versus required work

- `k-paper` currently has a single in-memory account, market/trading WebSockets, REST balances/open orders/trades/exchange info, simple limit/market matching, fees, a seeded synthetic book and startup market scenarios. Its account reserves quote/base as a spot exchange; its fill path updates balances but does not construct authoritative futures positions. The server has no REST position snapshot or durable exchange state. A type's optional `instrument_type` field and the runner adapter's configured `InstrumentType` do not make it a faithful futures simulator.
- The existing scenario presets move prices. They do not currently script accepted orders with lost acknowledgements, private-stream disconnects, duplicate/out-of-order events, REST/WS disagreement or restart recovery. `partial_fills` is present in config but is not a completed deterministic partial-fill scenario framework.
- `trading-system-ui` already shows orders, balances, positions, strategies, indicators and market data and can invoke existing global/strategy toggles and strategy-configuration RPCs. The shared proto currently gives `GetTradingStatus` only a boolean, and `MonitoringData`/`StrategyStatus` lack the proposed execution mode, pair readiness, reservations and recovery detail. `PauseEntries`, `CancelWorking`, `FlattenAndPause` and `Resume` are not existing UI commands.
- The existing `trading_system` Paper execution client allows one in-flight place, cancel and amend request of each kind. That is sufficient to smoke-test a single slow strategy only if requests are serialized; the future multi-strategy/concurrent-request lab needs correlation by request ID and an explicit concurrency test.
- The UI has login/auth code, but a browser login by itself does not authorize direct calls to the runner gRPC endpoint. Before exposing any capital-bearing control path, enforce authentication/authorization and network access at the runner/proxy boundary too; never rely on hiding a UI button.

These gaps are explicit prerequisites below. Do not claim a futures integration test, deterministic fault coverage or production control-plane readiness from the existing Spot example alone.

---

# 7. strategy_core responsibility

Create a small crate whose responsibility is:

> Pure strategy/domain logic.

It must contain no exchange transport, runtime orchestration, backtesting, persistence, or network behavior.

Candidate #2 belongs here as a pure model.

Desired conceptual API:

```text
ModelState + CompletedCandle -> ModelDecision + NewModelState
```

For Candidate #2, strategy_core owns:

- strategy ID/version;
- N=28 primary rule;
- direction enum;
- pure model state;
- log-spread calculation;
- geometric mean diagnostic;
- transition calculation;
- deterministic model tests.

It must NOT depend on:

- TradingClient;
- Bybit;
- Binance;
- Tokio runtime behavior;
- REST;
- WebSocket;
- Mongo;
- gRPC;
- live account state;
- funding fetchers;
- backtest accounting;
- research gates;
- files on disk.

Both `research_engine` and the Candidate #2 adapter in the composition app must call the same pure model. The generic runner and `trading_engine` must not import it.

Do not maintain a second production implementation of the signal.

---

# 8. research_engine responsibility

Create a separate crate whose responsibility is:

> Historical strategy research and economic evaluation.

It should own reusable abstractions for:

- HistoricalDataset;
- DatasetMetadata / DatasetManifest;
- completed candle loading;
- replay windows;
- research target streams;
- ExecutionSimulator;
- fee assumptions;
- execution adjustment assumptions;
- funding cash flows;
- mark-to-market;
- PnLLedger;
- equity/PnL samples;
- drawdown;
- calendar attribution;
- trade attribution;
- passive benchmarks;
- research scenarios;
- candidate specifications;
- preregistered gates;
- run manifests;
- reports.

The dependency direction should be:

```text
strategy_core
      ^
      |
research_engine
```

Forbidden:

```text
research_engine -> trading_engine
```

Research must not instantiate the live trading engine to decide whether a strategy historically made money.

The reverse dependency is also forbidden: neither `trading_engine` nor `runner` may import `research_engine` or `research_cli`, including through compatibility re-exports and deprecated runner binaries.

---

# 9. research_cli responsibility

Create a small CLI crate over research_engine.

The research commands have parity in `research_cli`; Increment 5R removed the deprecated `runner` forwarding binaries.

Target usage may look conceptually like:

```bash
cargo run -p research_cli -- dataset download ...
cargo run -p research_cli -- candidate run candidate_003 ...
cargo run -p research_cli -- report ...
```

Do not overdesign the CLI in the first extraction.

The initial goal is merely to stop accumulating offline research binaries in the live runner.

---

# 10. Independent audit remains separate

The independent audit must **not** move into research_engine.

Keep:

```text
audit/
```

as a deliberately independent implementation, currently Python.

Reason:

```text
research implementation
        |
        X
        |
must not prove itself correct
```

Audit code may duplicate important signal/accounting math when independence requires it.

This duplication is intentional.

The independent checker should never import:

- the Rust strategy model;
- research_engine's signal implementation;
- research_engine's economic calculation under test.

It may reuse another already-independent audit helper only where explicitly documented, as Candidate #2 currently does with the Donchian independent ledger reconstruction.

---

# 11. Migration map for existing research code

Do this conservatively. Preserve behavior first, refactor second.

## Move toward strategy_core

Pure Candidate #2 model was represented by:

```text
src/trading_engine/.../btc_price_ma_long_flat_v1.rs
```

The pure model is in `strategy_core`; Increment 5R removed the temporary `trading_engine` re-export.

Do not change the frozen mathematical behavior while moving it.

Tests before and after extraction must produce the same transitions.

Pure model code for future candidates should also live there.

## Move toward research_engine

Candidate #2:

```text
offline.rs
experiment.rs
gates.rs
```

Donchian reusable research infrastructure:

```text
donchian_daily_v1/offline.rs
donchian_daily_v1/backtest.rs
donchian_daily_v1/aux.rs
```

Generic functionality should move into research_engine.

Candidate-specific configuration can remain in candidate modules under research_engine.

Do not force all existing files to become generic in one patch.

First separate them from live trading_engine.

Then generalize obvious common abstractions with tests.

## Move toward research_cli

Current offline research binaries such as:

```text
btc_price_ma_experiment
donchian daily replay/evaluation tools
```

have matching entry points in `research_cli`; Increment 5R removed the old runner forwarding binaries. Recheck both replacement commands in the focused verification gate.

Maintain compatibility long enough to prove parity.

Remove old entry points only after the new CLI reproduces frozen results.

---

# 12. Research artifacts and data

Do not make committing large historical datasets the permanent research architecture.

Current committed data exists because it was necessary for the independent audit and current handoff.

Going forward:

```text
Git:
    code
    candidate specifications
    schemas
    small manifests
    audit logic

external/local data store:
    candles
    minute execution observations
    marks
    funding
    large ledgers
    repeated experiment outputs
```

Every dataset/run must have hashes.

Do not delete the currently committed frozen Candidate #1/#2 artifacts as part of the extraction.

Migration of current data storage is a separate later cleanup.

---

# 13. Research run manifest

Every new candidate run should record at minimum:

- candidate ID;
- candidate version;
- candidate specification hash;
- strategy model version;
- repository commit SHA;
- venue;
- product;
- symbol;
- interval;
- development start/end;
- dataset hashes;
- execution-data hashes;
- funding-data hash;
- mark-data hash;
- fee scenario;
- execution adjustment;
- assumed delay;
- benchmark identity;
- run timestamp;
- output hashes.

This must answer:

> Exactly which model, data and assumptions produced this result?

without inspecting a developer's shell history.

---

# 14. Candidate specification / preregistration

Before Cursor implements Candidate #3 or any later strategy, create a machine-readable and human-readable specification containing:

- candidate ID/version;
- hypothesis;
- proposed source of profit;
- external evidence/research supporting why it is worth testing;
- venue/instrument;
- signal inputs;
- exact entry/exit/target rules;
- size;
- allowed direction;
- execution assumptions;
- cost assumptions;
- development period;
- benchmark(s);
- economic gates;
- robustness gates;
- rejection criteria;
- holdout protocol.

Once the first performance run occurs, that version is immutable.

Any changed trading rule becomes a new version/candidate.

---

# 15. Current CEX baseline — old findings that are already fixed or partially fixed

Before implementing anything, inspect current `master`.

Do not blindly implement findings from the old audit.

The following capabilities are now present in current source and should be preserved/tested rather than duplicated.

## 15.1 Orderbook snapshot and sequence handling

Current market-data state now:

- distinguishes Snapshot vs Delta;
- replaces the book on snapshot;
- tracks the Bybit update ID;
- requires delta continuity;
- drops the local book on a detected gap;
- waits for a new snapshot;
- publishes the maintained full book, not the raw delta.

Bybit semantics verified against current documentation:

- a new snapshot means replace/reset local orderbook;
- `u` is the update ID;
- for full-depth synchronization `u` is consecutive and resets on service restart;
- `seq` is a cross sequence and is not guaranteed to be consecutive;
- `u=1` can indicate reset/restart and local state must be replaced/resynchronized.

Do not change the implementation to require:

```text
seq == previous_seq + 1
```

That would be incorrect.

## 15.2 Open-order bootstrap and periodic reconciliation

Current exchange_engine now fetches REST open orders during:

- bootstrap;
- periodic reconciliation.

ExchangeState replaces its current REST-known open-order set.

Trading-level reconciliation now also imports active exchange orders missing from TradingState and removes stale local orders missing from exchange state.

Preserve this.

## 15.3 Futures-aware balance validation

Current risk code includes futures-specific balance semantics:

- reductions do not require base-asset inventory;
- risk-increasing futures orders require quote margin;
- leverage may adjust the margin requirement;
- futures reductions can be sent without the previous spot-style balance error.

Preserve this.

## 15.4 reduceOnly reductions

Current executor marks futures orders that reduce filled exposure with:

```text
reduceOnly = true
```

Preserve and test this behavior.

## 15.5 Client IDs and pre-ack reservation

Current executor:

- generates an `orderLinkId`;
- puts it in exchange params;
- records a PendingSubmission before sending;
- includes unresolved submission quantity in effective exposure;
- prevents a pair from sending another new exposure while an unresolved submission exists.

Current Bybit adapter maps `orderLinkId` into Bybit's `order_link_id`.

Preserve this direction.

## 15.6 Amendment risk

Current executor now applies risk checks before amendments.

Preserve this.

## 15.7 Strategy-mode risk config

Current runner can obtain:

```text
risk_limits = strategies_config.risk_limits()
```

and pass it to RiskManager.

However, absent limits still fall back to `RiskLimits::default()`.

This is not enough for exchange execution and is addressed below.

---

# 16. Remaining production work

Candidate #2 should not reach real exchange execution until the following remaining issues are addressed.

---

# 17. Production Candidate #2 runtime adapter

Increment 4 put the working `BtcPriceMaLongFlatV1` adapter in `trading_engine`; Increment 5 registered it for Shadow. Increment 5R moved the adapter and its tests into `src/btc_price_ma_app/`. The composition app depends on `strategy_core`, `trading_engine` and the generic runner library, implements the existing `trading_engine::cex::state::Strategy` interface and returns generic `StrategyIntent`. Verify signal and Shadow parity before accepting the move; the engine must not depend on the app.

Add an injection point to the runner's existing strategy setup so a thin app binary can supply the concrete factory/strategy. Keep the runner's built-in factory for existing strategies. Candidate #2 configuration, validation and construction belong in the app; keep the shared runner source, manager, execution gate, journal and gRPC status generic. Remove runner branches on the literal `BTCPriceMALongFlatV1` name; express initial Shadow enablement through generic registration/configuration while preserving its current behavior. The default runner binary must fail clearly for Candidate #2 until the app binary is used.

Do not introduce IPC, a second process or a duplicate model formula. If a generic runner/engine interface needs a small additive change, preserve existing strategies, the Shadow guard and current proto behavior; explain and test the change before proceeding.

---

# 18. Normalize completed candles

Introduce a shared normalized completed-candle type.

Suggested shape:

```text
CompletedCandle {
    venue
    product
    instrument
    interval

    start_time_ms
    end_time_exclusive_ms

    open
    high
    low
    close
    volume
}
```

Do not make strategies parse Bybit response structures.

Exchange-specific parsing stays outside `strategy_core`. Increment 5R moved the public Bybit kline parser from the research re-export into the runner-local `bybit_public_klines` module. Run its focused tests to verify behavior.

---

# 19. Extend strategy runtime input

The live `Strategy` interface is currently orderbook-oriented with a default daily seed callback.

Introduce an explicit completed-candle callback with a default no-op implementation.

Conceptually:

```text
async fn on_completed_candle(...)
```

Existing strategies must continue compiling unchanged.

StrategyManager should route a completed candle only to strategies assigned to that pair. This interface and `CompletedCandle` are already generic on current master; keep them and inject the moved adapter through the app. No model import belongs in the manager or runner.

Do not rewrite Donchian in the same patch unless absolutely necessary.

---

# 20. Runner-owned completed daily candle source

The strategy itself must not poll REST.

The runner/platform data side should own the daily-candle source.

For Candidate #2, a 60-second REST poll is acceptable because the strategy acts once per completed day.

Flow:

```text
Bybit public kline REST
        |
        v
existing shared parser
        |
        v
completed candle filter
        |
        v
dedupe / continuity validation
        |
        v
CompletedCandle
        |
        v
StrategyManager -> injected Candidate #2 adapter in composition app
```

Requirements:

- only completed daily candles;
- chronological order;
- duplicate timestamp idempotence;
- conflicting duplicate detection;
- no synthetic gap filling;
- no use of forming daily close;
- one completed candle may update the model at most once.

---

# 21. Runtime bootstrap behavior

At startup fetch enough completed daily candles to reconstruct model state.

For N=28, require at least N+1 valid completed closes for:

- current state;
- previous state;
- latest transition detection.

Fetch additional margin rather than exactly N+1.

Bootstrap must reconstruct model state only.

It must not replay historical trades.

Explicitly separate:

```text
model reconstruction
```

from:

```text
execution eligibility
```

---

# 22. Candidate #2 runtime state

The live adapter (after Increment 5R, in the composition app) should expose/retain at least:

- strategy ID/version;
- symbol;
- lookback;
- recent completed closes;
- last processed candle;
- readiness;
- readiness reason;
- prior model direction;
- current model direction;
- desired target;
- latest log spread;
- latest geometric mean;
- latest transition candle;
- latest transition type;
- entry freshness state;
- recovery state.

Do not store the authoritative exchange position inside the strategy.

TradingState / ExchangeState remain authoritative for account state.

---

# 23. Entry freshness policy

A process restart during a long regime must not automatically chase an old signal.

Introduce explicit config:

```text
max_entry_age_secs
```

Do not invent the production value in code.

For exchange execution, require an explicit configured value.

A flat account may enter only if:

```text
latest model transition == flat -> long
AND
current_time - transition_candle_end <= max_entry_age
```

If the model is long but the transition is stale:

```text
do not enter
```

Wait for a future valid transition.

---

# 24. Exit freshness policy

Exits do not expire.

If:

```text
model target = 0
actual position > 0
```

continue targeting zero until account exposure is closed.

The system must distinguish:

- missing an old opportunity; and
- carrying unwanted live risk.

Only the first is allowed to expire.

---

# 25. Restart cases

Tests must explicitly cover:

### account long + model long

Hold / maintain exposure.

### account flat + model long, transition stale

Do not enter.

### account flat + model long, transition fresh

Eligible to enter.

### account long + model flat

Target zero.

### unexpected account short

Candidate #2 is long/flat only.

Do not silently convert or reverse.

Set pair/strategy state to something equivalent to:

```text
RecoveryRequired
```

and block new risk-increasing execution.

---

# 26. Separate evaluation from execution permission

A true shadow mode requires strategy evaluation to continue while exchange execution is forbidden.

Current architecture historically tied global trading enablement to strategy evaluation.

Refactor the semantics so that:

```text
strategy enabled
    -> may evaluate

execution permission
    -> may submit exchange orders
```

Those must be separate controls.

---

# 27. ExecutionMode

Introduce a runtime concept equivalent to:

```text
ExecutionMode::Shadow
ExecutionMode::Exchange
```

Do not encode Bybit demo/mainnet inside this enum.

Exchange environment and execution permission are separate dimensions.

Valid combinations:

```text
Shadow + Bybit mainnet public data
Exchange + k-paper local simulation (after the required instrument semantics are implemented)
Exchange + Bybit demo
Exchange + Bybit mainnet
```

Venue/environment, execution permission, strategy evaluation/enablement and entry pause are separate dimensions. A Paper venue label alone must never imply Shadow; `Exchange + Paper` really sends orders to `k-paper`. The final Bybit mainnet combination requires additional live interlocks.

---

# 28. Shadow mode requirements

When:

```text
strategy enabled = true
execution mode = Shadow
```

the system must:

- receive live completed candles;
- update Candidate #2;
- emit/record desired targets;
- expose model state;
- record what execution would have been requested;
- never call exchange order placement/amendment APIs.

Add a mock-client test that fails immediately if any trading method is called during Shadow mode.

---

# 29. Shadow decision journal

Persist an append-only journal.

Minimum fields:

- strategy ID/version;
- strategy/config hash;
- candle start/end;
- close;
- geometric mean;
- log spread;
- previous direction;
- resulting direction;
- desired target;
- decision timestamp;
- account filled position;
- working quantity;
- reserved submission quantity;
- effective position;
- entry freshness/eligibility;
- execution mode;
- reason;
- recovery/readiness status.

Runtime journals should not be committed to Git.

Use a gitignored runtime-data path.

---

# 30. Observability / gRPC status

Expose enough state that an operator can distinguish:

```text
model wants long
risk permits long
execution is enabled
account is actually long
```

At minimum expose:

- strategy ID;
- enabled;
- execution mode;
- readiness;
- readiness reason;
- last completed candle;
- last processed candle;
- log spread;
- geometric mean;
- model direction;
- desired target;
- actual filled position;
- working quantity;
- unresolved/reserved quantity;
- effective exposure;
- fresh entry eligible yes/no;
- unresolved submission yes/no;
- last reconciliation error;
- last execution error.

## 30.1 Authoritative operator contract

Extend the runner-owned proto with typed, per-account/per-instrument status rather than trying to infer production readiness from the existing `GetTradingStatus.enabled` boolean or a generic `StrategyStatus.last_signal` string. Include:

- runner mode, venue and environment (`K-PAPER / LOCAL`, `BYBIT / DEMO`, `BYBIT / MAINNET`) separately from `ExecutionMode` (`Shadow` or `Exchange`), strategy enabled and entries paused; the UI should not infer runner mode from a nonempty strategies list;
- strategy ID/version/config identity and the **independent** research disposition (`REJECTED AS VALIDATED ALPHA` for Candidate #2) versus operational role/status (`BASELINE`, `SHADOW`, `PAPER`, `DEMO`, etc.);
- last completed and processed candle, close, geometric mean, log spread, model direction, previous transition, desired target and entry freshness;
- risk-allowed target, authoritative filled position, compatible working remainder, unresolved-submission reservation, effective exposure, and the reason for a blocked action;
- pair readiness and reason, order/position reconciliation status and timestamp, unresolved submission identifiers/status, last execution/reconciliation errors, public/private stream health and market-data age;
- timestamps/sequence or freshness metadata for the status snapshot itself, including the specific account/instrument, so the UI can label a disconnected or stale view **unknown**, never `Ready`.

The backend remains the only authority for state and control results. The browser renders backend status and never calculates readiness, risk permission or effective exposure as a competing state machine. Research metadata is read-only provenance; the UI is not a research dashboard or a substitute for `research_cli`.

## 30.2 Browser UI delivery

In the separate `trading-system-ui` repository, add a strategy view that answers four questions at a glance: what the model wants, what execution/risk permits, what is actually on the exchange, and what is unhealthy or unresolved. Display target/filled/working/reserved/effective quantities distinctly, along with the last signal candle and recovery reason. In Shadow, show the model decisions and an unambiguous **no exchange execution** state; do not display simulated fills that did not occur.

Make venue/environment and `ExecutionMode` unmistakable in a persistent header on every control screen. For example, `BYBIT MAINNET + SHADOW` and `BYBIT DEMO + EXCHANGE` are different states. Do not collapse them into one green “Trading On” indicator. On gRPC stream loss, stale snapshots or partial data, show degraded/unknown rather than retaining a healthy appearance.

`trading_system/libs/trading_core/proto/trading_cli.proto` owns the API. For each additive backend change, sync `trading-system-ui/proto/trading_cli.proto`, regenerate `src/gen/`, then update typed RPC wrappers, store/hooks, views and tests. Verify a UI build and an end-to-end connection to the matching runner version. Do not read exchange configuration in the browser to guess its venue.

---

# 31. Bybit open-order pagination

Current Bybit `fetch_open_orders()` requests:

```text
limit = 50
cursor = None
```

and returns one page.

Current Bybit V5 docs state:

- page limit is 1..50;
- pagination uses `nextPageCursor`;
- for linear products, one of symbol/baseCoin/settleCoin is required;
- `orderLinkId` can query a specific order.

Implement pagination.

Requirements:

- use a valid linear selector;
- continue until next cursor is empty;
- enforce a maximum-page safety bound;
- reject pagination loops;
- deduplicate by order ID;
- do not silently truncate.

Tests:

- zero orders;
- one order;
- 50;
- 51;
- >100;
- repeated cursor;
- duplicate order across pages;
- page error.

---

# 32. Unknown submission recovery

The existing pre-ack reservation is only the first half.

When a submission outcome is uncertain, do not immediately clear the reservation and do not send replacement exposure.

Introduce a persistent submission state machine equivalent to:

```text
Prepared
Submitted
Unknown
Acknowledged
Rejected
Terminal
RecoveryRequired
```

Each unresolved submission should retain:

- client order ID / orderLinkId;
- strategy ID;
- pair;
- side;
- amount;
- price;
- creation time;
- last check time;
- state.

---

# 33. Recover unknown submission by orderLinkId

Use Bybit order lookup by `orderLinkId`.

Possible outcomes:

```text
working order found
    -> import order
    -> clear reservation

filled order found
    -> reconcile order/fill/position
    -> clear reservation when authoritative state is known

rejected/cancelled found
    -> clear reservation

still ambiguous
    -> keep reservation
    -> block new exposure
```

Do not interpret:

```text
not immediately visible
```

as:

```text
rejected
```

Recent closed orders may be available on the realtime endpoint; older records require appropriate history lookup.

---

# 34. Persist unresolved submissions

Unknown submissions must survive process restart.

Use a small durable journal.

Do not require Mongo for this critical path.

Persist enough state before/around submission that after a crash the process can discover:

- what it intended to submit;
- what client ID was used;
- whether the outcome is unknown.

On startup:

```text
load unresolved submissions
        |
        v
query exchange
        |
        v
reconcile
        |
        v
only then mark pair Ready
```

---

# 35. Futures fill / position synchronization

The private order/execution stream and futures position stream may arrive in either order.

Avoid double-counting exposure.

Preferred authority:

```text
futures position quantity:
    exchange Position update / REST position snapshot

execution events:
    order lifecycle + economic fill ledger
```

Do not make a fill event and position event independently add the same futures exposure.

If a fill is known but an authoritative position update has not arrived:

```text
PositionSyncPending
```

Block additional risk-increasing execution and trigger immediate position reconciliation.

Risk reductions should remain possible.

Tests:

- fill then position;
- position then fill;
- duplicate fill;
- duplicate terminal order;
- partial fill;
- reconnect after fill;
- REST reconciliation following fill.

Invariant:

```text
no event ordering may create duplicate economic exposure
```

---

# 36. Immediate reconciliation

The periodic reconciliation interval remains useful but is not sufficient for uncertain state.

Trigger immediate reconcile after:

- unknown order submission;
- fill with position sync pending;
- private WS reconnect;
- public market-data reconnect where execution readiness is affected;
- order/position contradiction;
- operator recovery command.

---

# 37. One executing strategy per account + instrument

There is no portfolio allocator yet.

Until one exists, enforce:

```text
one executing strategy per account + instrument
```

Multiple research/shadow strategies may eventually observe the same instrument if they do not execute.

Do not allow two exchange-executing strategies to independently size against the same BTCUSDT account position.

Fail configuration validation instead.

A future portfolio/strategy allocator can relax this rule.

---

# 38. Market-data freshness gate

Orderbook integrity now handles snapshots/gaps.

Execution also needs freshness.

Add explicit config:

```text
max_market_data_age_ms
```

Do not invent a default for mainnet execution.

Before risk-increasing execution require:

- a valid maintained orderbook;
- current time minus orderbook timestamp <= configured max age;
- pair execution readiness = Ready.

After:

- broadcast lag;
- orderbook sequence gap;
- public WS disconnect;

mark execution data unready until valid state returns.

Reduction/flatten paths may have a separately defined emergency policy.

---

# 39. Startup readiness state

Do not equate process-running with execution-ready.

Introduce explicit per-pair readiness, conceptually:

```text
Initializing
WaitingForMarketData
ReconcilingOrders
ReconcilingPosition
ResolvingSubmission
WaitingForStrategyData
Ready
PositionSyncPending
RecoveryRequired
```

New exposure requires:

```text
Ready
```

Risk reductions may be allowed in selected degraded states.

Document and test those exceptions.

---

# 40. Position mode / leverage readiness

The runner currently sets futures position mode/leverage at startup.

For exchange execution, failure must fail closed.

Candidate #2 assumes the configured one-way futures account semantics.

Shadow mode may continue despite inability to configure trading state because it cannot submit.

Exchange mode must not be Ready until required account mode/leverage configuration is confirmed or successfully applied.

---

# 41. Risk configuration

Strategy mode now supports risk limits, but absent config may still become an empty `RiskLimits::default()`.

For:

```text
ExecutionMode::Exchange
```

do not silently accept no risk limits.

Require explicit strategy-mode risk config.

Shadow mode may be allowed without executable limits because it cannot place orders.

---

# 42. Extend production risk limits

Existing limits are not enough for an unattended executor.

Add explicit limits equivalent to:

- max absolute position per pair;
- max total position;
- max order notional;
- max total notional;
- max active orders per pair;
- max total active orders;
- max market-data age.

Prospective exposure checks must include:

```text
filled position
+ working remainder
+ unresolved submission reservation
```

Do not validate only current filled state.

---

# 43. Daily-loss safety is not yet trustworthy

Current risk metrics still contain a TODO for realized PnL and currently calculate realized PnL as zero.

Therefore daily-loss protection must not be advertised as correct production protection yet.

Choose explicitly:

1. implement correct realized/equity accounting; or
2. reject exchange-mode config that relies on daily-loss limits until supported.

Do not retain a misleading safety control.

---

# 44. Execution ledger

Create a durable economic execution ledger from exchange evidence.

At minimum store:

- strategy/decision attribution;
- execution ID;
- order ID;
- client order ID;
- instrument;
- side;
- quantity;
- fill price;
- fee;
- fee currency;
- exchange timestamp;
- internal receive timestamp.

Also record, as available:

- funding;
- position snapshots;
- external deposits/withdrawals/balance changes that affect account PnL interpretation.

Order acknowledgement is not an economic fill.

Use actual executions for live PnL.

---

# 45. Correct realized PnL

Implement one-way futures PnL accounting from the execution ledger.

Track:

- position quantity;
- weighted entry;
- partial reductions;
- full close;
- realized price PnL;
- fees;
- funding;
- unrealized PnL.

For Candidate #2, crossing through zero into short must not occur as an intended strategy action.

Tests:

- open long;
- partial close;
- full close;
- winning close;
- losing close;
- fees;
- funding;
- duplicate execution;
- replay after restart.

---

# 46. Trading controls

Do not overload one global boolean.

Required operator concepts:

```text
PauseEntries
CancelWorking
FlattenAndPause
Resume
```

## PauseEntries

- block new risk-increasing execution;
- continue strategy evaluation;
- continue data processing;
- allow risk-reducing target zero;
- do not automatically flatten.

## CancelWorking

- cancel working orders;
- reconcile cancel/fill races;
- reconcile resulting position;
- remain paused.

## FlattenAndPause

- latch execution override target = 0;
- cancel conflicting working entries;
- reconcile fills;
- issue reduce-only exit;
- verify flat;
- remain paused.

The original strategy target must not immediately reopen.

Only explicit Resume clears the flatten override.

## 46.1 Operator workflow through Trading System UI

Implement the four controls as explicit runner commands and report accepted/in-progress/completed/failed state from the runner. Add browser actions only after the backend semantics and proto exist:

- `PauseEntries` displays that evaluation continues while exposure increases stop; it is separate from the old global enable/disable toggle.
- `CancelWorking` shows which orders are pending cancellation and requires reconciliation before presenting a final result.
- `FlattenAndPause` shows the exact account, instrument, venue, current position and working orders and requires deliberate confirmation; show progress until authoritative flat state is verified. The browser must not declare “flat” on a successful RPC alone.
- `Resume` requires explicit action, current readiness/risk checks and server-side permission; clearing the UI dialog cannot clear the backend's flatten override.

Commands must be idempotent or reject duplicate in-flight operator requests; log operator identity, request ID, scope and final outcome. Reject stale/ambiguous account or venue context and fail closed on disconnected status. Protect mutating RPCs at the runner/proxy boundary before mainnet; the UI auth service alone is not sufficient. Manual inspection and confirmation are required for mainnet enable/flatten. Show distinct restrictions in Shadow and Paper where a command has no external-exchange effect.

Existing UI changes to `max_position` and other operator configuration must remain bounded by server-side production risk limits, with the effective limit and change result visible to the operator. UI state alone cannot grant higher execution permission.

## 46.2 Future simulation lab

Only after the core paper fault suite is automated, an optional development-only UI page may request specific `k-paper` scenarios through a **runner-owned simulation-control API**. It must be server-gated to `Paper` and a local/test environment, separate from the production control API; the browser must never connect directly to `k-paper` or expose “drop next ACK” for Bybit. This page is useful for manual observation, not a prerequisite for the first paper or demo gate.

---

# 47. Shutdown semantics

Make shutdown policy explicit.

Supported policies may include:

```text
cancel_working
flatten_and_cancel
```

Do not silently flatten on every process exit.

Do not assume a cancel request proves that no fill occurred.

Shutdown behavior must be visible in config and logs.

---

# 48. Production deployment stages

Candidate #2 must advance through operational stages independent of its research-alpha status.

## Stage A — historical runtime parity

Feed frozen completed candles through the new runtime adapter.

Required result:

```text
runtime Candidate #2 model transitions
==
frozen offline N=28 transitions
```

No exchange calls.

## Stage B — live Shadow

Use Bybit mainnet public data.

```text
strategy evaluation = ON
exchange execution = OFF
```

Persist decisions.

The system is now genuinely running continuously.

## Stage C — k-paper full-stack rehearsal

Run the actual Candidate #2 composition binary (model adapter + generic runner **in one process**) -> risk -> executor -> `TradingClient` -> `k-paper` -> exchange state/reconciliation path, with the Trading System UI connected to that runner's gRPC server. `ExecutionMode = Exchange` and `venue = Paper / local simulation`; start entries paused and explicitly resume for a test. The simulator's market stream/orderbook is **not** a completed-daily-candle source by itself: supply controlled completed candles through the same runner-owned candle interface used in Shadow, while the Paper market-data adapter supplies executable orderbook/freshness state. Compare model decisions with the frozen offline expectations without turning `k-paper` into a research backtester.

An initial **Spot/levels connectivity smoke test** can happen early. It does not pass this stage for Candidate #2. Before claiming a BTCUSDT linear-perpetual rehearsal, implement and verify the minimum one-way futures contract in `k-paper` and its `trading_system` adapter: signed/authoritative position snapshots and updates, reduce-only exits, account/margin and fee/funding behavior needed by this path, instrument rules, and the REST/WS recovery queries used by production. Where Paper intentionally differs from Bybit, document the difference and leave a demo-specific gate.

Acceptance requires an entry, partial/complete fill, exit, restart of **the runner while the Paper server remains alive**, order/position reconciliation, ledger totals and operator pause/cancel/flatten/resume through the UI, plus the deterministic fault matrix in section 49. Compare runner state to the Paper server's independent order/account view. A restarted `k-paper` currently loses its in-memory account; persistence or a controlled restore fixture is needed before an exchange-server-restart test can claim continuity. Stop and record unresolved simulator limitations.

## Stage D — Bybit demo

Only after Stage C and CEX hardening pass:

```text
strategy = enabled
ExecutionMode = Exchange
Bybit environment = demo
entries start paused
```

Operator explicitly enables execution through the same Trading System UI. Repeat venue-specific API, account-mode, recovery, risk and control checks against Bybit demo; K-paper cannot prove Bybit's private event ordering, pagination, rate limits, or futures economics. Demo is an operational correctness test, not a profitability proof.

## Stage E — tiny mainnet operational validation

Not part of automatic implementation completion.

Requires explicit owner approval after:
- demo stability;
- correct recovery;
- correct live PnL;
- known fees;
- measured execution;
- verified kill/flatten controls;
- UI environment clarity, stale-data behavior and protected operator commands;
- explicit capital/position budget.

Candidate #2 remains labelled rejected as superior alpha.

---

# 49. K-paper deterministic fault matrix and Bybit demo validation

Build the missing `k-paper` scenario/fault controls as **test-only**, scriptable, seedable and repeatable. Its existing market shock presets are useful for price-path tests but do not produce the transport/account failures below. Keep failure injection in `k-paper`, the expected recovery logic in `trading_system`, and the observation/controls in `trading-system-ui`.

For reproducible fixtures, control scenario scheduling and simulated timestamps/IDs as well as market price randomness; current wall-clock timestamps and generated UUIDs otherwise make an exact event trace vary between runs. Scope faults to a test session/account/connection and reset them after each fixture so one test cannot contaminate another.

| Scenario to add/drive locally | Required runner outcome |
| --- | --- |
| Normal accept -> ack -> fill -> position/account update | Target converges; no excess working/reserved quantity |
| Accept order, suppress/delay reply or return timeout | Submission remains Unknown and reserved; no retry creates a second order; REST lookup by client ID or equivalent authoritative query recovers it |
| Explicit rejection, transient send failure and duplicate client ID | Rejection releases only a known-rejected reservation; unknown outcome remains reserved |
| Partial fills, then remaining fill or cancel/fill race | Filled + working + reserved exposure never double-counted; cancel outcome reconciled |
| Duplicate execution/terminal events and events delivered in both position-before-fill and fill-before-position orders | Idempotent ledger and authoritative position; `PositionSyncPending` until reconciled |
| Private/public disconnect, stale or gapped book, delayed/out-of-order events, REST/WS disagreement | New exposure blocked; reconcile on reconnect; no stale “Ready” state |
| Runner restart with resting order, position or unresolved submission while Paper state persists | Recover by identity and snapshot before resuming entries; no duplicate exposure |

Provide the Paper server/adapter with the minimal query surface for recovery by client order ID, including filled/terminal orders when a lost acknowledgement could otherwise be ambiguous. Do not equate absence from `/open_orders` with rejection. Support exact scenario steps, fault trigger timing and expected event trace in integration fixtures; random dev books are unsuitable for the final gate. A small, versioned test matrix should run without browser clicks in CI; then manually observe representative cases and invoke safety actions through the UI.

For each case assert the Paper account/order state, runner journal/ledger, gRPC status and browser view of mode/readiness/error. Record scenario seed/fixture, runner and `k-paper` revisions and result. The invariant is **no unintended duplicate exposure**, with reductions possible only under the explicitly designed degraded-state policy. Run realistic Bybit-demo smoke/recovery cases separately where the venue permits; do not claim the local simulator reproduces Bybit's actual protocol.

---

# 50. Real execution measurements

Once Shadow/Demo execution infrastructure exists, record:

- strategy decision time;
- target;
- market reference;
- order submission time;
- acknowledgement time;
- fill time;
- fill price;
- fill quantity;
- fee;
- funding;
- partial-fill behavior;
- decision-to-submission latency;
- submission-to-ack latency;
- submission-to-fill latency;
- implementation shortfall/slippage versus reference.

This data becomes input to future research assumptions.

Do not retroactively alter Candidate #1/#2 frozen research results.

Use observed execution data for future candidate specifications.

---

# 51. Candidate #3 research

Candidate #3 should be a meaningfully different hypothesis, not another casual tweak to Candidate #1/#2.

What has already been learned:

## From Candidate #1

Simple daily long/short Donchian 20/10 breakout on this development window was economically poor.

Do not infer:

```text
all trend following is bad
```

but do not simply tune 20/10 until it fits history.

## From Candidate #2

Long/flat price-to-geometric-average timing:

- remained profitable;
- reduced full-exposure drawdown;
- did not beat a simpler passive exposure control matched to average notional.

Therefore future research must continue to ask:

> What does the signal add beyond changing exposure?

Candidate #3 must have its own written source-of-profit hypothesis before implementation.

Cursor must not invent Candidate #3 trading rules in this program.

---

# 52. Research workflow going forward

Each candidate:

```text
1. hypothesis
2. source-backed rationale
3. exact frozen specification
4. development implementation
5. deterministic decisions
6. economic evaluator
7. relevant benchmark
8. preregistered rejection gates
9. independent audit
10. holdout only if development passes
11. operational deployment only if appropriate
```

Most candidates are expected to fail.

Do not optimize until something passes by changing rules after viewing results.

---

# 53. Dependency graph target

For this table, `A -> B` means A may depend on B:

| Crate | Dependency direction |
| --- | --- |
| `research_cli` | -> `research_engine` -> `strategy_core` |
| Candidate #2 app crate | -> `strategy_core`, `trading_engine`, `runner` library; its thin binary composes the generic runner and concrete adapter in one process |
| `runner` | -> `trading_engine` and neutral shared/live platform crates; never -> `strategy_core`, `research_engine`, `research_cli` or Candidate #2 app |
| `trading_engine` | -> neutral shared/live platform crates; never -> `strategy_core`, `research_engine`, `research_cli` or Candidate #2 app |
| independent audit | Does not import the implementation under test |

The prohibitions apply to **direct and transitive** normal, build and development dependencies under relevant feature sets. Put neutral `CompletedCandle` and `StrategyIntent` types and the existing generic `Strategy` interface on the live platform side; the concrete app implements that interface. Shared `trading_core` types are allowed. The final Candidate #2 **app binary** necessarily links the model and runner, but the `runner` crate and its default binary do not. Preserve the existing generic legacy strategy factory and modes; moving unrelated legacy strategies out of `trading_engine` is outside this correction.

At the pre-5R checkpoint `44f3b02`, `trading_engine` listed `strategy_core` and `research_engine`, and `runner` listed `research_cli`. Commit `a641304` removed these edges and the forwarding binaries, moved the Candidate #2 adapter into `btc_price_ma_app`, and copied the public Bybit kline parser into the runner. Verify the full transitive dependency tree after these edits; a manifest review alone does not close the gate.

External integration is contract-driven:

```text
trading-system-ui -> runner gRPC-Web contract -> trading_system
trading_system market/exchange/asset adapters -> k-paper WS/REST protocol
```

Neither separate repository is a `strategy_core` dependency. The runner owns proto semantics and Paper adapter mappings; `k-paper` owns simulator behavior; the UI owns rendering and operator interactions. Pin matching revisions/contract versions in cross-repo integration runs and document any compatibility break.

---

# 54. Implementation order

Do NOT implement this whole document as one giant patch.

Proceed incrementally.

Every increment:

1. inspect current code first;
2. make the smallest coherent change;
3. add tests;
4. run focused tests;
5. run relevant package tests;
6. report exact commands/results;
7. report unresolved blockers;
8. stop before next increment.

When an increment changes a cross-repo contract, report **separate changes and tests** for `trading_system`, `k-paper` and/or `trading-system-ui` as applicable. A green unit test in one repository cannot satisfy an end-to-end acceptance gate. Implement optional simulator features and the Simulation Lab only after the minimum pre-demo gates are met.

## 54.1 Cursor cross-repository handoff

This document is the authority for **the Candidate #2 research/runtime separation through Bybit demo and preparation for Candidate #3 research**. Read the repository-specific implementation guides when their increment begins:

| Increment | Repositories and guide |
| --- | --- |
| 0–5 | `trading_system`: functionally implemented on current master; check the status table in section 67 |
| 5R (accepted locally for 5B) | `trading_system` `f9d1299`; focused gate, graph, Shadow shutdown/restart, clean start and frozen parity reported |
| 5A (accepted locally for 5B) | UI `d8c7b271` / guide `2636d5e`; Envoy first-update, stale, reconnect, disabled controls and Shadow order display reported |
| 5B (accepted locally; bounded connectivity only) | `trading_system` `043ca64` (evidence `1f00fdc`) + UI `66632ed` + k-paper `e5d0a59`; default runner `--mode levels` against sample K-paper config |
| 6–7 | `trading_system` recovery/futures logic and K-paper protocol/futures support; change producer and adapter together |
| 8–9 | `trading_system` risk/ledger and UI controls/economic display; sync the proto and generated UI client |
| 9A | All three repositories: deterministic Paper qualification and browser operator checks |
| 10 | `trading_system` and UI on Bybit demo; K-paper remains the deterministic failure lab |

Check out the relevant repositories with write access before starting a cross-repo increment. Record starting and ending SHAs for each; pin matching revisions and run both producer/consumer checks before marking the increment complete. After 5R, use the Candidate #2 app binary for Shadow/Paper/demo rather than assuming the default runner binary can instantiate it. Implement backend state and the source proto first, then sync the UI proto and generated code. When a precise simulator accounting rule, API shape or security mechanism is not fixed here, document the bounded choice and its acceptance tests within that increment rather than silently guessing or postponing the safety gate.

`trading_system` Increments 0–6 are accepted locally for proceeding (see section 67). Increment 7 is next. This plan does not authorize implementation of a future portfolio allocator, strategy registry/graveyard, broader instrument/venue expansion, a self-hosting operations program, Candidate #3's trading rule or mainnet deployment. Those need separate decisions/specifications when the evidence and need exist.

---

# 55. Increment 0 — baseline lock

**Status:** Done. Baseline SHA `6ea3ac77f8c7890e25627705f778cb4049eb2cdc` (master immediately before Increment 1). Frozen Candidate #1/#2 audits and research tests were recorded; no research outputs were changed.

Before structural changes:

- record current master SHA;
- run current Candidate #1 audit;
- run current Candidate #2 reference tests/audit;
- run relevant trading_engine tests;
- record failures that are already present and unrelated;
- do not "fix" research outputs.

Acceptance:

- frozen Candidate #1 numbers unchanged;
- frozen Candidate #2 numbers unchanged;
- audit scripts still reproduce current outputs.

---

# 56. Increment 1 — extract strategy_core

**Status:** Functional extraction done in `4b51b207662c888a35f5935712d15c63504727a3`; reverse live-crate dependency edges were removed in `a641304` and locally verified for 5B in the 5R record at `f9d1299`.

Create `strategy_core`.

Move only pure Candidate #2 model behavior first.

Do not move live runtime or research evaluator yet.

Requirements:

- pure model API;
- no trading_engine dependency;
- same N=28 behavior;
- same N=14/N=70 behavior for research;
- same config-hash semantic fields;
- unit tests copied/adapted;
- frozen decision parity test against Candidate #2 decision logs.

The temporary live-crate re-exports were removed in 5R.

Acceptance:

- Candidate #2 frozen decision logs still reproduce;
- no economic outputs change;
- trading_engine compiles;
- research tooling compiles.

Stop and report.

---

# 57. Increment 2 — extract research_engine

**Status:** Done in `b462f8bda1a6516719e7221df07229ec9ae0feac`.

Create `research_engine`.

First move behavior without redesigning it.

Move/rehome:

- historical dataset structures;
- completed series/window helpers;
- offline replay infrastructure;
- execution simulator;
- fee/funding/mark accounting;
- drawdown/equity samples;
- research scenarios;
- benchmark logic;
- Candidate #2 gates;
- auxiliary public-data dataset loaders/downloaders as appropriate.

The first goal is behavioral parity, not ideal generic APIs.

Candidate #1 and Candidate #2 frozen backtests must remain identical.

Acceptance:

- Candidate #1 exact outputs unchanged;
- Candidate #2 exact outputs unchanged;
- independent audits still pass;
- research_engine does not depend on trading_engine.

Stop and report.

---

# 58. Increment 3 — create research_cli

**Status:** Research CLI parity done in `d488c31a140d2bb8f51ac2c16c44dfda032059a8`; deprecated runner forwarding binaries were removed in `a641304`.

Move offline research command entry points out of the live runner.

Initially support the already-used workflows.

Do not delete old runner commands until parity is demonstrated.

Acceptance:

- new research CLI can reproduce frozen Candidate #1/#2 runs from the same files;
- manifests/hashes preserved;
- old paths may be deprecated only after parity.

Stop and report.

---

# 59. Increment 4 — Candidate #2 runtime model adapter + completed candles

**Status:** Behavior done in `01f284b059bc9bda1f0116394dd0c7acf6d10cbe`. The adapter moved to `btc_price_ma_app` in `a641304`; the app factory still rejects Candidate #2 in Exchange mode. Recheck frozen parity in the 5R gate.

Implement:

- normalized CompletedCandle;
- Strategy completed-candle callback;
- StrategyManager dispatch;
- runner-owned daily candle source;
- Candidate #2 runtime adapter;
- bootstrap reconstruction;
- duplicate/gap/unfinished handling;
- entry freshness state;
- restart state.

No exchange execution yet.

Do not add Candidate #2 to automatic live trading.

Acceptance:

- frozen historical N=28 runtime transitions exactly match frozen offline decisions;
- duplicate completed candle produces no duplicate model update;
- forming candle ignored;
- missing required history => not ready;
- stale historical long signal does not create a fresh-entry event.

Stop and report.

---

# 60. Increment 5 — Shadow mode

**Status:** Shadow behavior done in `fbf7bfad535474e907d11c767795fa8f686b0651`. Candidate #2 is registered by the composition app in Shadow only; `ShadowGuardClient` and the executor skip mechanically prevent exchange order calls; sample config is `sample_config/bybit-shadow/`. 5R moved this behavior behind the composition binary; local Paper+Shadow runtime and restart evidence is recorded at `f9d1299`. Authenticated Bybit integration remains a later gate.

Implement:

- ExecutionMode;
- evaluation separate from exchange permission;
- shadow decision journal;
- observability/status;
- mechanical prevention of exchange calls in Shadow.

Then make Candidate #2 runnable continuously in Shadow using Bybit mainnet public data.

This is the first operational milestone.

Acceptance:

- strategy updates from completed daily candles;
- target/status visible;
- zero exchange order calls;
- restart retains/reconstructs model semantics;
- journal records decisions.

Stop and report.

## Increment 5R — remove live/research coupling (local acceptance complete for 5B)

**Code status:** Implemented in `a64130422f8aee3e25d6d9b1a089c7015c5964b6`; `62959135d13e43369229be1f8cc78d7198191f2c` added package test/graph recipes and adapter tests; `9a0dd97d7a67595a793002ac1d40f9645c759828` wires a local `just gate-5r` and strengthens the adapter/guard assertions. Source review supports the one-process app, removal of live/research Cargo edges and Exchange refusal. Later recorded command, graph and Paper+Shadow runtime results below complete the local gate for 5B.

**Starting point:** `master` `44f3b02a72f8a60ec41ed33e0ad8516651dec8af` recorded Increments 0–5. Record the actual starting SHA when work begins; do not overwrite a newer commit. Do not redo the frozen research or Shadow logic. This is a boundary correction with parity gates, not permission for Exchange execution.

1. Inspect `src/trading_engine/Cargo.toml`, `src/runner/Cargo.toml`, `src/trading_engine/src/cex/strategies/{factory.rs,rust_strategies/}`, `src/trading_engine/src/cex/mod.rs`, `src/runner/src/cex/{strategies.rs,klines.rs,completed_dailies.rs}` and the two deprecated research forwarding binaries. Record the direct/transitive Cargo edges and a pre-change Shadow run/test baseline.
2. Add one Candidate #2 app crate (suggested location `src/btc_price_ma_app`) containing the existing model adapter and a thin binary. The adapter may depend on `strategy_core` and implement the existing `trading_engine::cex::state::Strategy` trait. The app binary composes it with the `runner` library **in the same process**; it must reuse the existing risk, executor, completed-candle source, Shadow guard, journal and gRPC service. Keep Candidate #2 rejected in Exchange mode.
3. Give `runner` a small strategy-factory injection point (closure/trait/registry argument) and retain the built-in factory for existing strategies. The default runner binary stays independent of the Candidate #2 app and returns an explicit unsupported-strategy error for Candidate #2. Remove the two runner string-name branches that enable Candidate #2 in Shadow; use generic per-strategy configuration/registration for initial enablement and preserve snapshot-restore behavior.
4. Delete the Candidate #2 adapter, model/research re-exports and Donchian research re-exports from `trading_engine` once consumers use their proper crate. Keep Candidate-specific validation, construction and the rejected-alpha label in the app while preserving generic snapshot/journal fields. The two optional scalar `lookback` and `max_entry_age_secs` values may pass through generic factory arguments; this bounded transport does not make the engine depend on the model. Avoid adding more Candidate-specific branches or model logic to live crates. Move the public Bybit kline parser used by `src/runner/src/cex/klines.rs` to a neutral/shared or runner-local location with behavior tests; do not keep an engine -> research dependency for this parser. Remove `src/runner/src/bin/{btc_price_ma_experiment,donchian_daily_replay}.rs` and the `research_cli` dependency only after verifying their equivalent `research_cli` commands and documenting the replacement invocation. Leave unrelated generic `prop_replay` behavior intact.
5. Keep the existing Shadow sample config semantically equivalent, but document the new app command that launches it. Preserve the same strategy ID, config hash, completed-candle/entry-freshness decisions, journal fields, gRPC view and zero exchange-order calls. Existing levels, Donchian and BuyLowSellHigh startup must still work through the default runner.

**Acceptance:** Frozen Candidate #1/#2 audits and N=14/N=28/N=70 decision logs remain unchanged; focused `strategy_core`, `research_engine`, `research_cli`, `trading_engine`, `runner` and new app tests plus relevant workspace checks pass. Compare deterministic Shadow decisions/journal and snapshot/restart behavior before and after; the guarded client sees zero order calls. Verify `cargo tree -p trading_engine` and `cargo tree -p runner` across normal/build/dev dependencies and relevant feature sets contain none of `strategy_core`, `research_engine`, `research_cli` or the Candidate #2 app. Prove the app binary does contain `strategy_core` and `runner`, and direct default-runner Candidate #2 configuration fails clearly. Record exact commands, results, changed files, starting/ending SHAs and any pre-existing failures. This acceptance record must be closed before 5B; initial 5A code already landed without a recorded 5R gate. Stop if any focused check fails.

**Review of `a641304` (2026-09-28):** GitHub Actions run [36403594087](https://github.com/Kolzodev712/trading_system/actions/runs/36403594087) reports a successful root `just test`, but its log says **0 tests passed**: the command is `cargo test --lib --all-features` at the workspace root, so it does not exercise the new app or the package unit tests. `just check` likewise selects the root package and does not compile the app. The manifest and code review supports the intended graph, but this CI result does not establish 5R parity. The Clippy, Docker, Indicator Config, Integration Tests and Platform config smoke failures match the same failures in the immediately preceding [run 36402167284](https://github.com/Kolzodev712/trading_system/actions/runs/36402167284); track these baseline CI faults separately and compare any new failures to that run.

**Required 5R verification follow-up:**

1. At `a641304` (or a subsequent fix SHA), run `cargo test -p btc_price_ma_app --all-targets` and `cargo build -p btc_price_ma_app --bin btc_price_ma_app`; exercise the factory refusal in Exchange and the frozen N=28 adapter parity test. Run focused package tests for `strategy_core`, `research_engine`, `research_cli`, `trading_engine` and `runner` (use `cargo test -p <crate> --all-targets` for each), plus the existing Shadow guard, journal and restart tests. Record exact test counts and outcomes, not just a root `just test` result.
2. Run `cargo tree -p trading_engine -e normal,build,dev --all-features` and `cargo tree -p runner -e normal,build,dev --all-features`; confirm no `strategy_core`, `research_engine`, `research_cli` or `btc_price_ma_app` anywhere in either tree. Run the matching `cargo tree -p btc_price_ma_app -e normal,build,dev --all-features` and confirm both `strategy_core` and `runner` are present. Check the default runner's explicit rejection of Candidate #2 and the app's Shadow-only construction.
3. Produce a deterministic Shadow before/after comparison at the same config and candles: strategy ID/config hash, decisions, journal fields, snapshot/restart behavior and zero guarded exchange order calls. If a test does not yet cover one of these, add a focused assertion or harness. Keep the old research command parity and frozen decision logs unchanged.
4. Run the focused package checks with explicit packages or an appropriate `--workspace` invocation and retain exact local output/test counts at a pinned SHA. `just gate-5r` is now a runnable local recipe, but its existence alone is not a passing result or a repository-enforced CI check. GitHub Actions is intentionally disabled here. Do not describe the gate as remotely enforced; if the owner requires pre-merge automation, add a separate required check. A green root `just test` with zero tests is not 5R evidence. If a focused check fails, fix it before continuing. Close both open gates before 5B.

**Review of 5R follow-up `6295913` (2026-09-28):** `just test` and `just check` now select workspace packages, and `just test-5r-graph` exists, but no command outputs, test counts or Cargo trees were recorded with the commit. The same commit deleted `.github/workflows/ci.yml`; `just ci` does not call `test-5r-graph`, and there is no replacement automated gate visible in this repository. If removal of GitHub Actions was intentional, supply an enforceable alternative or explicitly revise the acceptance requirement with the owner; do not call a local recipe a CI gate. The two new adapter tests replay the same frozen log into two fresh adapters and pass zero as an input to the journal; they establish deterministic replay/serialization, not a real runner restart or zero outbound order calls. Keep 5R open until the relevant package tests, graph check and guarded runtime evidence are run and reported.

**Review of `9a0dd97` (2026-09-28):** `just gate-5r` now selects the six relevant packages, builds the app, runs `test-5r-graph`, checks formatting and applies focused Clippy; local `just ci` invokes it. Bybit's `demo` configuration selects the dedicated demo REST/private-stream hosts and matches the new venue label. The adapter asserts baseline role, research disposition and deterministic intent after replay; the guard test counts attempted forbidden calls and its inner dummy panics if reached. These are useful source-level checks, but the commit includes no actual gate output/test counts/Cargo trees, and no GitHub Actions run exists for this SHA. The adapter test still compares two fresh replays of the frozen log rather than a persisted runner restart. The journal test intentionally supplies a nonzero would-have-requested count; it does not prove zero exchange calls. Record a deterministic runner Shadow run across process restart, comparing decisions/journal/snapshot with the pre-5R baseline and checking actual venue open orders and the guarded mutation path. A pinned, passing local `just gate-5r` transcript with the parity evidence can close this phase's verification; do not claim remote enforcement where none exists.

**Verification of `e6fac236` (2026-09-28):** Starting SHA `e6fac236caee7312f9c971000f2d1a478cc76f30`. `just gate-5r` exit 0. Package counts: `btc_price_ma_app` 14, `research_cli` frozen_parity 2, `research_engine` 50, `runner` 25, `strategy_core` 3+3 frozen, `trading_engine` 93; app build and focused Clippy ok. `cargo tree -p trading_engine|runner -e normal,build,dev --all-features` (runner also `--features redis`) contain none of `strategy_core`, `research_engine`, `research_cli`, `btc_price_ma_app`. `btc_price_ma_app` tree contains `strategy_core` and `runner`. Frozen N=14/N=28/N=70 decision SHA-256s were unchanged after the gate (`a543c2ae…`, `658d506b…`, `1fd63eff…`). GitHub Actions is still disabled; these are locally reported results summarized in this plan, not a repository-attached raw transcript or a remotely enforced check.

The adapter test `persisted_restart_replays_same_frozen_candles` and the dummy-client `fetch_open_orders() == []` assertion **do not** close the runtime gate: the first drops one in-process adapter and reconstructs another from the frozen log; the dummy always returns empty.

Runtime Shadow evidence at this SHA (not the unit tests): `sample_config/bybit-shadow` dies at exchange start with Bybit `retCode=10003` `API key is invalid`; no journal. Substitute path: `btc_price_ma_app --mode strategies` against Paper + `execution_mode=shadow`, same Candidate #2 strategies file, public daily candles. `grpc_ctl restart cancel_working` wrote `config/cex/runtime-snapshot.json` with `BTCPriceMALongFlatV1:BTC/USDT` `enabled=true`, then the process forced-exit after the 30s shutdown timeout. Relaunch logged `Restored runtime snapshot (1 strategies, trading=false)`. Journal stayed append-only (4 → 6 lines); strategy_id, config_hash `sha256:6e02ddef41a418dc26f5ea8acfd592417846362211b3f70de3ab553d41c3559a`, direction `long`, target `0.001`, candle start `1790467200000` / close `84437.1` matched. `grpc_ctl status` after load: `orders=0`, strategy enabled. Independent venue read `GET http://127.0.0.1:19999/open_orders` → `[]`. Runner logs: `k-paper: sent place_order` = 0, `shadow mode forbids` = 0, executor `would-be order(s) … not sent` > 0. Production `ShadowGuardClient::wrap` still discards its counter; the live executor never calls `open_order` because `may_submit` is false. Shutdown then called `cancel_all_orders` on `services.trading_client()` (unwrapped) and logged success — that is an outbound mutation path around the guard. 5R remains open for the Shadow mutation bypass and restart-state gaps. The sample's literal placeholder API credentials explain the Bybit authentication failure; do not require live credentials merely to qualify the local 5B connectivity preflight.

**Review of documentation-only `f9d1a405` (2026-09-28):** Source inspection confirms the runtime violation: `strategy_runner_with` wraps the CEX client for Shadow, but shutdown obtains `services.trading_client()` and calls `cancel_all_orders` on the raw client. The Paper venue returned no open orders in the reported run, yet the unguarded cancellation itself violates the zero-mutation invariant. Put shutdown order mutations through the same guard or explicitly skip them in Shadow; cover `RequestRestart` and ordinary shutdown with a backing client that records every mutation. The new dummy-client empty-list assertion only proves read forwarding. Also, `RuntimeSnapshot::load` restores a saved `enabled=false`, then the `initially_enabled` loop sets it back to true. Test the disabled case and give saved operator state precedence. The observed 30-second forced exit must be explained or fixed before calling restart graceful. The Bybit sample contains literal `xxxxxxxxxxxxxxxxxxxx` API credentials, so `10003` cannot diagnose a code fault. The Paper+Shadow session is useful generic runtime evidence; authenticated Bybit behavior belongs at its later venue gate. The commit only edits this plan; it does not implement these corrections.

**Verification on `5a0c8c4` (2026-09-28):** Starting SHA `286ad4cf545f065a4f65c9a27c4dc55ac9ee1da1`. Shadow shutdown no longer uses `services.trading_client()`; `cancel_working_orders_on_stop` returns immediately in Shadow. Production wrap uses `wrap_counted` and logs `Shadow guard forbidden mutation attempts`. Snapshot restore overlays `initially_enabled` (saved `enabled=false` wins). gRPC uses `serve_with_shutdown`; after the runner shutdown signal the gRPC task is aborted so a live Monitor stream cannot pin the process.

`just gate-5r` exit 0 twice after the edits. Counts: `btc_price_ma_app` 14, `research_cli` frozen_parity 2, `research_engine` 50, `runner` 29 (was 25), `strategy_core` 3+3 frozen, `trading_engine` 93. Frozen N=14/28/70 hashes unchanged (`a543c2ae…`, `658d506b…`, `1fd63eff…`). Trees: engine/runner 0 forbidden crates; app has `strategy_core` and `runner`.

Instrumented tests (recording backing client): `shadow_ordinary_shutdown_records_zero_inner_mutations`, `shadow_request_restart_save_load_keeps_disabled_and_zero_mutations`, `exchange_shutdown_records_cancel_all_on_backing_client`, `initially_enabled_applies_when_snapshot_absent`.

Paper+Shadow runtime (`btc_price_ma_app --mode strategies` against k-paper `:19999`, Candidate #2 strategies, `execution_mode=shadow`): ordinary SIGINT and `grpc_ctl restart` both logged `skipping cancel_all_orders`, `forbidden mutation attempts: 0`, `All components shut down successfully` (not the 30s timeout). Independent `GET :19999/open_orders` → `[]`. `k-paper: sent place_order` = 0. Executor `would-be` > 0 while enabled. After disable + RequestRestart, snapshot `enabled=false`; relaunch restored that over `initially_enabled=true` (`grpc_ctl status` `enabled=false`). SIGINT with the browser Monitor stream connected also returned immediately after aborting the gRPC task.

The 30s forced-exit cause: `TradingGrpcServer` previously called `Server::serve` with no shutdown, so `wait` always timed out. `serve_with_shutdown` is not enough while a long-lived Monitor RPC is open; aborting the gRPC task after the runner shutdown signal is what makes UI-connected stop finish without the 30s wait.

Dummy adapter/open-order unit tests are still not this evidence. Bybit sample placeholder keys are not a code defect. Stop for review before 5B.

**Correction to the prior review of `5a0c8c4` (2026-09-28):** The claimed cold-start regression was incorrect. I missed a construction-time `if config.initially_enabled { set_strategy_enabled(true) }` immediately after `add_strategy`, before the snapshot-present branch. A clean start was enabled in that revision. The later `f9d1299` refactor removes that earlier call and centralizes both cold-start and snapshot enablement in `load_and_apply_strategy_enablement`; its clean-start and persisted-disable checks establish the intended behavior. Completed-candle reconstruction is independent of the strategy enabled flag, so moving the enablement call after the initial candle poll does not skip model reconstruction. This correction supersedes the preceding claim.

**Verification of cold-start enablement on `f9d1299` (2026-09-28):** Starting SHA `1f1868909a519101bd521773b43462e9edded60a`. `strategy_runner_with` now always calls `load_and_apply_strategy_enablement` (load snapshot if present, then `apply_strategy_enablement`). Global trading restore remains gated on a `mode == "strategies"` snapshot. Construction-time `set_strategy_enabled(true)` was removed so this is the sole startup path. Regression `clean_start_load_path_honors_initially_enabled_without_snapshot` exercises that helper with a missing file; the persist-disable test restores through the same helper.

`just gate-5r` exit 0. Counts: `btc_price_ma_app` 14, `research_cli` frozen_parity 2, `research_engine` 50, `runner` 30 (was 29), `strategy_core` 3+3 frozen, `trading_engine` 93. Frozen N=14/28/70 hashes unchanged (`a543c2ae…`, `658d506b…`, `1fd63eff…`). Trees: engine/runner 0 forbidden crates; app has `strategy_core` and `runner`.

Paper+Shadow runtime (`btc_price_ma_app --mode strategies` against k-paper `:19999`, Candidate #2 strategies `initially_enabled=true`, `execution_mode=shadow`): after deleting `config/cex/runtime-snapshot.json`, `grpc_ctl status` showed `BTCPriceMALongFlatV1:BTC/USDT enabled=true` with no snapshot file and no `Restored runtime snapshot` log. `GET :19999/open_orders` → `[]`. `k-paper: sent place_order` = 0. Executor `would-be` > 0 while enabled. After disable + `RequestRestart`, snapshot `enabled=false`; shutdown skipped cancel_all, forbidden 0, not the 30s timeout. Relaunch logged `Restored runtime snapshot (1 strategies, trading=false)` and `grpc_ctl status` `enabled=false`. Dummy adapter tests are still not this evidence. Stop for review before 5B.

**5R acceptance review after `f9d1299` (2026-09-28):** The reported `just gate-5r` results and Cargo graphs, unchanged frozen decision hashes, an instrumented Shadow shutdown/restart path, an independent K-paper open-order read, a clean Candidate #2 start with `initially_enabled=true`, and a saved-disable restart satisfy the local 5R gate for moving to the bounded 5B Spot/levels preflight. The fixes are visible in source; the command and browser outcomes are Cursor's recorded local results, not an independently rerun or remotely enforced CI transcript. The Bybit sample's placeholder keys do not block local 5B. The prior adapter/dummy-client tests alone would not have sufficed; the reported runner and Paper session supply the runtime evidence. Keep Exchange execution and Bybit demo gated by later increments.

## Increment 5A — Trading System UI Shadow observability

**Status:** Accepted locally for 5B. The timestamp/UNKNOWN fail-open fix is in UI `d8c7b271` (guide `2636d5e`); Envoy browser checks against the matching Paper+Shadow runner cover first update, stale/disconnected state, reconnect, persisted disable and observed orders. 5B is ready but not started.

After the runner's authoritative status/proto fields exist, update the separate `trading-system-ui` repository and generated client. Show Candidate #2's **rejected-as-alpha / operational-baseline** distinction, last completed candle, signal/target, Shadow execution state, venue/environment, pair readiness and reason, freshness and errors. Keep model status visible when execution is paused. Connect to the actual runner; the old Rust TUI is not the deliverable.

Acceptance: proto copies and generated types match; UI unit/build checks pass; a disconnected/stale stream is visibly unready; Shadow shows the **observed** exchange order state while model decisions remain visible. Read actual runtime enabled/paused state. Test the browser through Envoy against the matching runner and stop with both repository revisions and exact results.

**5A correction gate (review of backend `6295913` and UI `66025f4`):**

1. The UI's `useMonitoringStream` sets `streamConnected` before receiving a snapshot, and a normally ended stream can spin without marking it disconnected. `useStatusFreshness` ignores `snapshotUnixMs` and calls a disconnected main-screen snapshot “live” indefinitely. Use receipt/age and completeness checks, mark normal completion and timeout stale, display unknown/degraded state on every screen, and prevent control actions based on stale context. Test first-message delay, silent stream, clean end, failure, reconnect and main-screen staleness.
2. `exchangeOrdersLabel("shadow", 3)` currently returns “0 exchange orders (shadow)”. Never replace an observed nonzero count with zero. Display the actual authoritative count and flag any Shadow violation; prove zero outbound order calls in a guarded runner path. The new journal test that supplies zero as an input does not prove this.
3. The backend sets `operational_role` from `ExecutionMode`, so Candidate #2 in Shadow reports `SHADOW`, not the separate `BASELINE` role required by this plan. Publish the app's generic metadata independently of execution mode and verify the UI shows both “rejected as validated alpha” and “baseline” alongside Shadow mode.
4. `with_execution_status` is wired only in the CEX strategies runner. Spot/levels and DEX monitoring still emit default `UNKNOWN` venue/environment, so 5B's K-paper levels/browser label cannot pass. Populate authoritative venue/environment/mode for every runner path and test a Paper Spot/levels snapshot. Derive Bybit DEMO/TESTNET labels from the actual selected environment; a `testnet` boolean alone must not assert DEMO.
5. The proto copies match at `6295913`/`66025f4`. Add backend field assertions, run `npm run generate`, `npm test`, `npm run build`, and exercise the actual browser -> Envoy -> runner path with pinned revisions. Record exact results and remaining gaps. Stop and report the correction before 5B.

**Restart control follow-up:** A persisted snapshot `enabled` value now overrides `initially_enabled` on restore. Cold start without a snapshot still uses `initially_enabled`. Verified with RequestRestart save/load of `enabled=false` while config `initially_enabled=true`.

**Review of backend `9a0dd97` and UI `0476c58` (2026-09-28):** The CEX strategy, CEX levels and DEX gRPC paths now publish independently derived venue/environment/mode. Candidate #2 publishes `BASELINE` from its model snapshot, separate from Shadow execution. Bybit testnet is `TESTNET`; `DEMO` follows the actual `demo` host setting. The UI starts the stream on the main screen, waits for its first update, expires silent/old data, marks stream end/failure stale, disables several control paths when not live, and displays the actual strategy `active_orders` count with a Shadow-violation marker. Unit/component tests cover these cases in source. One actionable bug remains: `computeStatusFreshness` treats a missing, zero or invalid `snapshotUnixMs` as live once other fields are set, because it only checks snapshot age when conversion succeeds. It also accepts a truthy `executionMode` value of `UNKNOWN`. Reject incomplete/invalid timestamp and unknown mode as `unknown` (controls disabled); add tests for those fields and a real stream payload. The strategy order count comes from runner trading state; corroborate the Shadow zero-order claim against an exchange-side read and guard evidence, not the UI text alone. No recorded `npm run generate`, `npm test`, `npm run build`, or browser -> Envoy -> matching runner session accompanies these commits. Capture that evidence with exact SHAs; keep 5B blocked until it passes.

**Verification of UI `d8c7b271` / backend `e6fac236` (2026-09-28):** `d8c7b271` treats missing, zero, or invalid `snapshotUnixMs` and execution mode `UNKNOWN` as `unknown` and not actionable; tests include generated `MonitoringData()` defaults. At that UI SHA: `npm run generate` exit 0; `npm test` 7 files / 49 passed (stderr `ECONNREFUSED 127.0.0.1:3000` on one happy-dom fetch); `npm run build` exit 0 (`DexMonitor-BjbWxYF8.js`). Browser `http://127.0.0.1:5173/` → Envoy `:8081` → `btc_price_ma_app` `:50051`: first update **PAPER LOCAL + SHADOW**, Stream live, Candidate #2 **BASELINE** / **REJECTED AS VALIDATED ALPHA**, observed orders **0**; injected `snapshotUnixMs=0` → **STATUS UNKNOWN** and Global Trading disabled; `RequestRestart` then process exit → **STATUS STALE · last PAPER LOCAL + SHADOW** and Disable Strategy disabled; silence (paused apply, 8s+) same stale/disabled; relaunch reconnects live with the same hash/labels/order count 0. This is a Paper+Shadow composition session, not Bybit sample Shadow. 5B is still blocked on the Shadow shutdown and restart-state corrections. Valid Bybit credentials are not required for the local 5B preflight.

**Affected browser recheck after backend shutdown/restart fixes (2026-09-28, UI `2636d5e`):** First update **PAPER LOCAL + SHADOW**, Stream live, C2 **BASELINE** / **REJECTED AS VALIDATED ALPHA**, orders **0**, persisted disable shown as Enable Strategy / ⏸. Process SIGINT with the Monitor stream open finished without the 30s timeout → **STATUS STALE · last PAPER LOCAL + SHADOW**, Enable Strategy disabled. Relaunch reconnects Stream live with the same labels, persisted disable, orders 0, `GET :19999/open_orders` `[]`.

## Increment 5B — existing Paper connectivity preflight

**Status:** Accepted locally as a Spot/levels connectivity preflight. Increment 6 is next; this is not an order-execution or futures qualification.

Run the existing `k-paper` server and `trading_system` Paper adapters with the documented Spot/levels configuration: market WS, trading WS, REST balance/open-order/trade reconciliation and asset rules. Connect the browser UI to the same runner and verify venue labeling. Record exact working endpoints and identify missing Candidate #2 futures operations and scenario controls.

This is a wiring smoke test, **not** the Stage C futures or fault gate. Do not bypass account-mode, risk or readiness failures by merely setting the adapter's instrument type to Futures. Stop and report.

**Verification (2026-09-28):** Starting SHAs `trading_system` `2b6ca6027a8403c1aca29d208d8911fafef40334`, UI `66632edc810c22686db146061574b0a854d4bf6c`, k-paper `e5d0a590b95c0f266edd9faab15c365a12c15ccc`. Default runner (not the Candidate #2 composition app). `execution_mode` defaulted to Exchange (sample bot omits the field).

K-paper: `PAPER_EXCHANGE_PORT=19999 PAPER_EXCHANGE_PROFILE=dev cargo run` → `profile=Dev scenario=None market_engine=true instruments=5`, listen `127.0.0.1:19999`.

Independent venue reads: `GET /balances` 6 assets; `GET /open_orders` `[]`; `GET /trades?limit=5` empty list; `GET /exchange_info` `serverTime`/`rateLimits`/`symbols`. `/ws/market` streamed `orderbook` `BTC-USDT` with 5 bids. `/ws/trade` first frame `connected`; `get_account_snapshot` replied `balance` on the same connection.

Runner: `runner --mode levels --config sample_config/k-paper/bot.json --levels-config sample_config/k-paper/levels.json`. Logs: asset `GET /exchange_info` 5 symbols; bootstrap reconciliation **6 balances, 0 orders, 0 trades, no discrepancies**; `k-paper: connected to ws://127.0.0.1:19999/ws/trade`; `k-paper market data: connected to ws://127.0.0.1:19999/ws/market` (BTC/USDT, ETH/USDT); gRPC `[::]:50051`; `GET http://127.0.0.1:8080/health` 200 `venue_mode=Cex cex_ready=true md_fresh=true`. `grpc_ctl status`: `runner_mode=levels venue=PAPER environment=LOCAL execution_mode=exchange`, Spot/Paper, `orders=0`, `orderbooks=2`, `indicators=4`. Independent `GET /open_orders` still `[]`.

Browser UI `66632ed` → Envoy `:8081` → runner `:50051`: Connect `/grpc`, Stream live, **PAPER LOCAL + EXCHANGE** on header/card/footer (same fields as gRPC, not endpoint text), Runner: Levels. Monitor: BTC/USDT live book (bid/ask ~50629/50729), balances (USDT 10000000, BTC 100, …), SMA/EMA values, four Spot level rows, open orders empty, recent trades none, Trading OFF.

Not in this preflight (documented limits, not 5B defects): no REST position snapshot; no terminal-order lookup by client ID; no Candidate #2 linear perpetual, reduce-only, leverage/margin, or fault-injection scenario controls; Paper fill accounting remains spot base/quote. Global trading stayed off, so levels did not post. MongoDB supplied indicator definitions (file skipped as source of truth). Periodic reconcilers are 180s; only bootstrap was observed. This is not Bybit demo.

The 5B gate is closed for its stated connectivity scope. The process kills in the session were deliberate replacements of leftover servers before the recorded run, not preflight failures. Exercise populated order/trade and ambiguous-submission paths in Increment 6 and later qualification gates; the empty lists above prove only the read/reconciliation wiring exercised here.

---

# 61. Increment 6 — CEX recovery hardening

**Status:** Accepted locally. Section-61 review findings are closed. Matching k-paper `ebf4c903b7e393ce9843467f91d378697d022250`; UI proto/status contract unchanged (`028ea16cb53f83871df1ee4053b7caeb36fbb9a8`). Isolated tests and one focused Spot strategy-runner → Paper lost-ACK/restart check were run locally; neither the section 49 matrix, futures qualification nor Bybit demo was run.

Implemented paths:

- Bybit open-order pagination (`limit` 1..=50, `nextPageCursor`, max-page bound, dedupe by order id). Remaining cursor, repeated cursor, or page error fails the list; truncated pages are not returned as success.
- Valid linear selector: symbol per configured pair, otherwise `settleCoin`.
- JSONL submission journal (`runtime-data/submission-journal.jsonl`): Prepared/Submitted are persisted and fsynced before send. Append or load/parse failure **halts new exposure** and does not send. A missing journal on the executor likewise halts.
- `TradingClient::fetch_order_by_client_id` (Bybit `orderLinkId` realtime then history; k-paper REST). Missing lookup is `Inconclusive`, not a rejection.
- Restart recovery imports a found working order without resending. Cumulative `filled_amount` is applied before the reservation is released (working partial fills and canceled/expired/rejected with fills). Repeat lookup is idempotent via `fill_applied`.
- Immediate WS-vs-REST and trading-level reconciliation use `ImmediateTrigger`: a request made while the worker is busy is stored and runs when the current cycle finishes, rather than waiting for the 180-second interval.
- k-paper: `GET /orders/by_client_id/:id` including terminal orders; `POST /test/suppress_place_ack` with optional `clientOrderId` (omit to suppress the next place regardless of id). Lab hooks `TRADING_TEST_CLIENT_ORDER_ID`, `TRADING_TEST_FORCE_PLACE`, `TRADING_TEST_LIMIT_PRICE`.

**Tests (local, not remote CI):**

| Command | Result |
| --- | --- |
| `cargo test -p exchange_engine --lib` | 251 passed, 0 failed, 1 ignored (includes pagination and `request_while_busy_runs_before_next_interval`) |
| `cargo test -p trading_engine --lib` | 105 passed, 0 failed (unwritable journal, torn JSONL recovery halt, **restart with damaged journal halt**, partial working fill, partial canceled fill, persist-before-crash, missing lookup retains reservation) |
| `cargo check -p runner` | ok |
| `cargo test -p paper-exchange-core` | 7 passed, 0 failed (`suppress_next_place_ack_any_without_client_id` plus prior lookup/suppress tests) |
| `cargo check -p paper-exchange-server` | ok |
| `scripts/inc6_lost_ack_restart.sh` | PASS (see live check below) |

**Live focused Paper lost-ACK/restart (2026-09-28):** Spot `runner --mode strategies` against k-paper `:19997` with Paper kept alive. `POST /test/suppress_place_ack` `{}` armed the next place. Runner 1 used `TRADING_TEST_CLIENT_ORDER_ID=inc6-lost-ack-1`, `TRADING_TEST_FORCE_PLACE=1`, `TRADING_TEST_LIMIT_PRICE=5000`, qty `0.002`. Independent `GET /open_orders` showed one New buy (`42f0ba7d-…`, same client id) while the journal was `prepared` then `submitted`. Runner 1 was SIGKILL'd; Paper still had 1 order. Runner 2 relaunched with trading off and no force-place: `imported_working=1`, journal `acknowledged`, Paper still 1 order with the same id, `GET /orders/by_client_id/inc6-lost-ack-1` the same row. `grpc_ctl status`: `orders=1` `qty=0.002 status=New`, global trading false. No replacement order.

**Prior review findings at `c16a7be` / `ed9568b` and how they were closed at these SHAs:**

1. Pre-send persistence no longer fails open. Journal append/load errors halt new exposure; unwritable path, torn JSONL, and **restart reopen of a damaged journal** (`restart_with_damaged_journal_halts_new_exposure`) are tested. Recovery failure sets `halt_new_exposure`; no replacement send.
2. Recovered partial fills are applied before unblocking. Working remainder plus fill, and canceled-with-fill, are tested including missed and repeated events.
3. Immediate reconcile stores a pending request; a request during busy work runs before the next interval (`request_while_busy_runs_before_next_interval`).
4. Next-place suppress (no client id) plus controlled lab ID proved one Spot runner restart while Paper retained the accepted order.

Increment 6 is accepted locally. Continue Increment 7.

---

# 62. Increment 7 — futures state correctness and readiness

**Status:** Accepted locally. Matching k-paper SHA recorded with this increment. UI proto unchanged (`028ea16`). Candidate #2 remains an operational baseline, not validated alpha.

Implemented:

- Futures fills update `fill_applied` only; `set_position` from REST/WS is quantity truth (`PositionSyncPending` until the snapshot).
- Per-pair increase block while pending or while market data is older than required `max_market_data_age_ms` (Exchange+futures; no invented default). Reductions remain allowed unless halted/unresolved.
- Second executing strategy on the same pair is rejected at config validate.
- Bybit and Paper Exchange futures: `set_position_mode` / `set_leverage` fail-closed at startup; leverage must be explicit.
- k-paper one-way USDT-linear BTCUSDT: IM = notional/leverage in USDT, reduce-only cannot open/enlarge a short, `GET /positions`, signed qty, taker fee from `linear.taker_fee_bps`, optional `apply_funding`. Default `config/instruments.yaml` stays spot; linear fixture is `config/linear-btcusdt.yaml`.
- Completed dailies remain the runner candle source, separate from Paper books.

**Tests (local, not remote CI):**

| Command | Result |
| --- | --- |
| `cargo test -p paper-exchange-core` | 15 passed (linear IM, reduce-only flatten, reduce-only cannot short, one-way naked sell rejected; spot tests unchanged) |
| `cargo check -p paper-exchange-server` | ok |
| `cargo test -p trading_engine --lib` | 106 passed (`futures_fill_does_not_double_count_with_position_event`) |
| `cargo test -p exchange_engine --lib` | 251 passed, 1 ignored |
| `cargo test -p runner --lib` | 31 passed (`second_executing_strategy_on_same_pair_is_rejected`) |

Paper vs Bybit (intentional): isolated one-way only; no hedge/cross; no liquidation engine; funding is `apply_funding` with fixture `funding_rate`, not an 8h clock; IM is notional/leverage with no Bybit IM table; mark for UPnL is last fill; Paper process restart still loses the account (runner restart while Paper stays up is Increment 6/9A).

Stop and report. Continue Increment 8.

---

# 63. Increment 8 — production risk and controls

Implement:

- explicit risk required for Exchange mode;
- max position/notional/order limits;
- prospective effective-exposure risk;
- PauseEntries;
- CancelWorking;
- FlattenAndPause;
- Resume;
- explicit shutdown policy.

Expose authoritative command state/results over runner gRPC, then sync the proto/generated UI client and add `PauseEntries`, `CancelWorking`, `FlattenAndPause` and `Resume` workflows in `trading-system-ui`. Show venue/environment and exact action scope in confirmation, enforce access on the backend/proxy boundary, and verify the commands against the Paper server. Keep the old global toggle's semantics explicit during migration.

Do not claim daily-loss safety until realized PnL is correct.

Acceptance:

- risk-increasing actions are blocked correctly;
- exits remain possible;
- flatten override cannot be undone by strategy target without Resume;
- cancel/fill race reconciles safely;
- UI reflects server-confirmed progress/failure and cannot represent a timed-out flatten as complete; Shadow/Paper/Demo labels are unambiguous.

**Local Increment 8 record (2026-09-28):** `trading_system` (this commit) + `trading-system-ui` matching proto/client. Package tests: `cargo test -p trading_core --lib grpc::trading_control` 3 passed; `cargo test -p trading_engine --lib` 108 passed; `cargo test -p runner --lib` 31 passed; `cargo test -p gateway --lib grpc::access` 3 passed. UI `npm test` 52 passed; `npm run build` exit 0. Paper live `scripts/inc8_operator_paper.sh` against k-paper `:19996`: PauseEntries `entries_paused=true`; FlattenAndPause `state=in_progress` with Paper sell `2946feab-…` still open (`filledAmount` 0.577 of 100) so RPC success was not treated as flat; Resume cleared latch. Daily-loss remains unlabeled as safety until Increment 9 PnL. `TRADING_OPERATOR_TOKEN` unset is local-lab only.

Stop and report.

---

# 64. Increment 9 — execution ledger and live PnL

Implement:

- durable execution ledger;
- fees;
- position accounting;
- realized PnL;
- funding ingestion/accounting;
- unrealized PnL;
- daily equity/PnL tracking.

Then make daily-loss risk real or explicitly keep it disabled.

Use deterministic Paper fills, fees and funding fixtures to test the ledger, and expose backend-calculated economic status in the UI. Label these values as simulation in Paper mode; no Paper result changes the frozen Candidate #2 research disposition.

Acceptance:

- deterministic fill-ledger replay;
- restart reconstructs economic state;
- partial close calculations tested;
- daily-loss limit uses real economic PnL.

**Local Increment 9 record (2026-09-28):** `trading_system` (this commit) + `trading-system-ui` matching proto/client. `k-paper` `c2466dc64a274a5b164fb7bcf2005bacb71b7c18` unchanged (Paper fill `fee` already mapped; `apply_funding` remains a fixture, not an 8h clock). Package tests: `cargo test -p trading_engine --lib` 118 passed (ledger open/partial/full/win/loss/fees/funding/duplicate/restart; `daily_loss_uses_ledger_realized_pnl`; missing ledger fail-closed); `cargo test -p trading_core --lib grpc::trading_control` 3 passed; `cargo test -p runner --lib` 31 passed; `cargo test -p gateway --lib` 3 passed; `cargo test -p exchange_engine --lib execution_to_trade_copies` 1 passed. UI `npm test` 54 passed; `npm run build` exit 0. Daily-loss is real when `daily_loss_limit_*` is set and the execution ledger is attached; unset daily-loss stays explicitly disabled. Paper/local `economic_source=simulation`. Candidate #2 remains an operational baseline, not validated alpha.

Stop and report.

## Increment 9A — full-stack K-paper qualification

Run Stage C and the section 49 matrix with Candidate #2's actual runtime model and futures Paper adapter, not a manually fabricated strategy order. Use deterministic completed-candle inputs and separate market-orderbook scenarios. Assert target, risk decision, placed order, unknown/working/filled/reserved state, position, reconciliation, ledger and UI state. Restart the runner while `k-paper` keeps its state. Exercise pause/cancel/flatten/resume through the browser and confirm results against the Paper server.

Acceptance: no unintended duplicate exposure in any scripted case; failed/unknown outcomes fail closed; all required status and operator actions have recorded evidence; remaining Paper/Bybit semantic differences are listed for Stage D. Stop and issue a Paper readiness report. **Do not enter Bybit demo until this gate passes.**

**Reported local Increment 9A evidence (2026-09-28; subject to review hold below):** `trading_system` (this commit) + k-paper `09266a313fc244259b39e421f4d0126053a219ee` + `trading-system-ui` matching guide (SHA in that repo). Candidate #2 `btc_price_ma_app --mode strategies` against Paper linear BTCUSDT (`PAPER_EXCHANGE_CONFIG=config/linear-btcusdt.yaml`, dummy book separate from runner-owned fixture candles `scripts/inc9a_long_candles.json`, lookback=2 lab window not frozen N=28). Commands: `cargo test -p btc_price_ma_app --lib` 14 passed (Paper Exchange allowed, Bybit Exchange rejected); `cargo test -p runner --lib` 32 passed (`fixture_file_loads_completed_bars`); `cargo test -p trading_engine --lib` 118 passed; `cargo test -p trading_core --lib grpc::trading_control` 3 passed; `cargo test -p gateway --lib` 3 passed; `cargo check -p exchange_engine` ok. `cargo tree -p runner --edges normal -i strategy_core` and the same for `trading_engine` match no packages. Live `bash scripts/inc9a_matrix.sh` exit 0 against k-paper `:19995`:

| Section 49 row | Result |
| --- | --- |
| Normal accept → ack → fill | C2 TargetPosition 0.001; Paper buy `b8e56d2e-…` then fill `199ee277-…` qty 0.001 fee 0.03016774 USDT; journal prepared/submitted/acknowledged; ledger one fill |
| Lost ACK + runner restart, Paper kept | `POST /test/suppress_place_ack {}`; client `inc9a-lost-ack-1` limit 49000 New `d0241998-…`; SIGKILL; Paper same row; relaunch `imported_working=1`, journal acknowledged, grpc `orders=1` same id, no replacement `place_order` |
| Explicit reject | One-shot hook and isolated Paper core test exist; the script resets this hook before a Candidate #2 placement, so the runner outcome is unproven |
| Duplicate fill | Hook and ledger unit-level idempotency exist; no duplicate fill is driven through the Candidate #2 → Paper → runner path in the script |
| Private/public disconnect | Both sockets close and reconnect logs were reported; the script treats absence of a disconnect log as a warning and does not assert readiness, exposure block or post-reconnect reconciliation |
| Stale book | Injection has an isolated test, but the script does not assert the runner becomes stale or blocks a new increase; dummy OB can overwrite the aged snapshot |
| Runner restart with position | After fill, disable, SIGKILL; Paper still `positions=[{qty:0.001}]` + trade; relaunch bootstrap REST position `pos=0.001`, `imported_working=0`, no replacement |
| Operator pause/flatten/resume | grpc PauseEntries `entries_paused=true`; FlattenAndPause `state=in_progress` then TSM `positions=0` matching Paper `[]` after qty=0 position emit; Resume clears latch, global unchanged |

Browser UI `http://127.0.0.1:5173/` → Envoy `:8081` → composition `:50051`: **PAPER LOCAL + EXCHANGE**, **SIMULATION** header PnL, C2 **REJECTED AS VALIDATED ALPHA**, Stream live. Pause entries confirmation showed venue/mode and `Positions: BTCUSDT qty=0.001`; after confirm grpc `entries_paused=true` and Paper still 0.001. Flatten confirmation: “A successful RPC is not a flat account”; after confirm grpc `flatten_latched=true` `pos=0`, Paper `positions=[]` plus independent sell trade `64cbf4e0-…`. Resume confirmation: “Does not enable the legacy global toggle”; latch cleared. With global still on and the fixture still long, C2 re-entered 0.001 after Resume — expected operational baseline, not a flatten miss.

Remaining Paper/Bybit differences (Stage D, not claimed here): Paper MD is snapshot-only (`sequence: None`), so Bybit sequence-gap drops are not reproduced; dummy book is not the frozen N=28 research series; REST futures positions are still applied as recon truth (a false REST override is not armed because it would poison TSM); Paper process restart still loses the in-memory account; funding is `apply_funding` fixture not an 8h clock; isolated one-way, no liquidation engine.

**Owner review of Increment 9A (2026-09-28; `trading_system` `b1361112d32e34f03b48dd6c77ab769bbf8b90a3`, `k-paper` `1c603c332817c08597897597fcc4127b46315d83`, UI `dea5d720712b9f3466f696212f0793db06fbd146`):** The normal fill, restart-with-position, lost-ACK recovery and manual pause/flatten/resume observations are valuable and retain their stated scope. An exit-0 run of `scripts/inc9a_matrix.sh` does **not** establish the full section 49 matrix: it arms reject and then resets before a place, arms duplicate-next-fill and stale-book without triggering/asserting their runner outcomes, and only warns if disconnect evidence is missing. Its `paper_json` helper substitutes `[]` for an HTTP failure; ledger output is optional rather than an assertion of totals. The script does not drive a partial-fill/cancel race, transient send failure or duplicate client ID, duplicate/out-of-order private events, or REST/WS disagreement. UI evidence includes PauseEntries, FlattenAndPause and Resume, but not CancelWorking. The short lookback=2 fixture does not itself compare C2 decisions with the frozen N=28 offline expectation required by Stage C.

**Finish Increment 9A before the 10 gate:** For each missing section 49 case, drive the fault through the actual Candidate #2 composition and assert the independent Paper order/position view, target and risk result, journal/ledger state, gRPC readiness, and no unintended extra exposure. Reset faults between fixtures; fail on Paper HTTP errors and absent required records. Add an explicit partial-fill/cancel race and browser CancelWorking check, plus one N=28 decision-equivalence run through the completed-candle interface. Where Paper cannot model a Bybit sequence gap, use a bounded adapter/runtime fixture and leave the actual venue semantics for Demo. Exercise REST/WS disagreement against authoritative REST (for example, a bad WS update) or in a bounded adapter fixture; do not poison the Paper REST truth. Record those coverage boundaries explicitly. Re-run the matrix and report pass/fail per row with revisions and evidence. No separate new increment is needed.

**Increment 10 control preflight:** Before enabling demo mutations, ensure operator authorization is enforced at a trusted server or proxy boundary. The current runner allows mutation when `TRADING_OPERATOR_TOKEN` is unset; the UI reads `VITE_OPERATOR_TOKEN` into browser code, and `x-operator-id` comes from client storage. A browser-distributed token and client-supplied identity do not establish protected per-operator demo controls. Verify unauthorized commands are denied and the displayed/audited operator identity is authenticated. This can be completed as the first guarded work inside Increment 10 after the 9A gate; do not enable demo entries until it passes.

**Revised Increment 9A report (2026-09-28; this commit; pending owner re-review):** `trading_system` (this commit) + k-paper `1feee383052a0b37e9f155e9190ce3b086efee75` + UI `10496e72094f8136cb51b237ae2787fbace3d764`. Candidate #2 `btc_price_ma_app --mode strategies` against Paper linear BTCUSDT (`PAPER_EXCHANGE_CONFIG=config/linear-btcusdt.yaml`, dummy book separate from runner-owned fixture candles `scripts/inc9a_long_candles.json`, lookback=2 lab window). `scripts/inc9a_matrix.sh` exit 0 against k-paper `:19995` (`INC9A_WORKDIR=/tmp/inc9a-matrix`). Package fixtures in that script: `frozen_n28_runtime_transitions_match_offline` passed; `delta_merges_only_when_sequence_continues` passed; `aged_timestamp_is_stale`, `timeout_place_error_stays_unknown`, `rest_position_overwrites_lying_ws_update`, `record_trade_uses_trade_id` passed. k-paper `cargo test -p paper-exchange-core --lib` 23 passed. Paper GET/POST failures fail the script; journal/ledger records are required.

| Section 49 row | Result |
| --- | --- |
| Normal accept → ack → fill | Pass. Journal `prepared submitted acknowledged`; ledger unique fills ≥ 1; Paper REST trades nonempty |
| Lost ACK + runner restart, Paper kept | Pass. `POST /test/suppress_place_ack {}`; client `inc9a-lost-ack-1` limit 49000 New `b62fca58-…`; SIGKILL; Paper same row; relaunch `imported_working=1`, grpc `orders=1` same id, no replacement `place_order` |
| Explicit reject | Pass. `POST /test/reject_next_place`; runner `Known place rejection`/`lab_reject`; journal `rejected`; Paper orders stayed 0 |
| Duplicate fill | Pass. `POST /test/duplicate_next_fill`; unique ledger fills 2→3; Paper and grpc `pos=0.001`, not `0.002` |
| Private/public disconnect | Pass. Disconnect logged; `reconnecting trade WS` required; Paper still `0.001`; grpc `pos=0.001` after reconnect |
| Stale book | Pass. `POST /test/stale_orderbook`; runner `Stale market data blocks exposure increase`; Paper `open_orders=[]` and `positions=[]` |
| Runner restart with position | Pass. After fill, disable, SIGKILL; Paper kept position+trade; relaunch no replacement place; Paper still `0.001` |
| Operator pause/flatten/resume | Pass. grpc PauseEntries `entries_paused=true`; FlattenAndPause latch then TSM `positions=0` matching Paper; Resume clears latch |
| Partial fill then cancel/fill race | Pass. Far limit qty 0.002; `lab_fill_open` `PartiallyFilled` 0.001; journal acknowledged; unique ledger +1; grpc `CancelWorking` of `d064b1dd-…`; Paper working `[]`, position remained `0.001`; grpc `orders=0` `pos=0.001` |
| Position-before-fill event order | Pass. Unique ledger fills 4→5; grpc `pos=0.001` |
| Transient send failure (drop) | Pass. `POST /test/drop_next_place`; runner `open_order timeout`; journal `unknown`; Paper did not book `inc9a-drop-1` |
| Duplicate client ID | Pass. Second place with `inc9a-dup-cid` `Known place rejection`; journal `rejected`; Paper extra working orders 0 |
| REST/WS disagreement | Pass. `POST /test/emit_ws_position` qty 1 reached grpc `pos=1`; Paper REST unchanged; reconnect restored grpc `pos=0.001` |
| Frozen N=28 decision comparison | Pass. `cargo test -p btc_price_ma_app --lib frozen_n28_runtime_transitions_match_offline` through the completed-candle interface (not the lookback=2 live lab) |
| Sequence-gap book | Bounded fixture only: `cargo test -p market_data_engine --lib delta_merges_only_when_sequence_continues`. Paper MD is snapshot-only (`sequence: None`) |

Browser UI `http://127.0.0.1:5173/` → Envoy `:8081` → composition `:50051`: **PAPER LOCAL + EXCHANGE**. Cancel working confirmation: venue/mode `PAPER LOCAL + EXCHANGE`, scope account, `Working orders: 1`, `Positions: none reported`, consequence text that cancel/fill races must reconcile. Confirm sent Paper `cancel_order` `05dc0701-…` (`inc9a-ui-cancel` limit 49000); CancelAck; independent `GET /open_orders` `[]`; grpc `orders=0` `positions=0` `entries_paused=true`. Prior Pause/Flatten/Resume browser evidence is unchanged.

Runner fixes shipped with this report: known place errors persist `rejected` and release reservation; timeouts stay `unknown`; Paper fill events merge into the working order instead of marking the whole order `Filled`; `cancel_all_orders` unions REST `/open_orders` so CancelWorking reaches a partial remainder. `paper_get`/`paper_post` fail closed.

Remaining Paper/Bybit differences (Stage D, not claimed here): Paper MD is snapshot-only, so Bybit sequence-gap drops are not reproduced on the live Paper path; dummy book is not the frozen N=28 series (N=28 is the adapter/offline comparison above); REST futures positions remain recon truth (lying WS was used instead of poisoning REST); Paper process restart still loses the in-memory account; funding is `apply_funding` fixture not an 8h clock; isolated one-way, no liquidation engine.

Candidate #2 remains an operational baseline, not validated alpha. Keep Increment 10 / Bybit demo on hold pending owner re-review of this report. Do not implement the operator-token/Vite control change until that 10 preflight.

---

# 65. Increment 10 — Bybit demo

After Increment 9A passes, run Candidate #2 through:

```text
Completed candle
    ->
Candidate #2 app -> strategy_core model
    ->
runtime target
    ->
risk
    ->
executor
    ->
Bybit demo
    ->
order/execution/position
    ->
reconciliation
```

Exercise the demo-applicable parts of the section 49 fault matrix.

Collect execution measurements.

Operate and inspect the runner through `trading-system-ui` with an unmistakable `BYBIT DEMO + EXCHANGE` indicator. Verify Bybit-specific order/account-mode behavior and the relevant faults that demo can exercise; retain the Paper matrix for deterministic failures demo cannot induce. Start paused, then enable explicitly only after readiness and UI/control checks pass.

Do not promote to mainnet automatically.

Stop and report a demo readiness report.

---

# 66. Increment 11 — research framework cleanup for Candidate #3

After the production path is operational enough:

- formalize CandidateSpecification;
- run manifests;
- candidate directory convention;
- dataset registry/manifest;
- external data storage convention;
- report templates.

Do not implement Candidate #3's trading rule until a separate strategy research specification is supplied.

---

# 67. Current Cursor instruction

The Increment 9A record above is **reported local evidence pending owner re-review**, not a remotely enforced CI result. The first incomplete matrix at `b136111` remains on file; this revision drives and asserts the previously unasserted section 49 rows. The pinned implementation SHAs are listed in section 64; the sibling guides record their own revisions. The accepted 5B connectivity record remains `trading_system` `043ca64523f42212cd8a2616ba6201d61d7c878a` (evidence `1f00fdc80f7bbbb39b836da9445e041eb18e6c77`).

| Increment | Status | Gate |
| --- | --- | --- |
| 0–5 | Done | Prior SHAs and audit evidence above |
| 5R | Accepted locally for 5B | Dependency boundary, focused packages, frozen parity and Paper+Shadow runtime/restart evidence reported; no remote CI claim |
| 5A | Accepted locally for 5B | Matching browser/runner Paper+Shadow live/stale/reconnect, persisted disable and observed order state reported |
| 5B | Accepted locally for Spot/levels connectivity | Market/trade WS, REST bootstrap and UI labels; no order posting, futures or fault qualification |
| 6 | Accepted locally | Fail-closed journal including restart of a damaged record, partial-fill recovery with missed/repeated events, pending immediate recon, and one focused Paper lost-ACK/restart check |
| 7 | Accepted locally | Futures PositionSyncPending, no fill+position double count, MD freshness required for Exchange futures, one strategy per pair, fail-closed leverage/mode, Paper one-way USDT-linear BTCUSDT |
| 8 | Accepted locally | Explicit Exchange risk, prospective effective exposure, PauseEntries/CancelWorking/FlattenAndPause/Resume, operator token on mutating RPCs, UI confirmation does not treat flatten RPC as flat |
| 9 | Accepted locally | Durable execution ledger, fees/funding/weighted PnL, restart replay, daily-loss from ledger or fail-closed if configured without it, UI simulation label for Paper |
| 9A | Reported complete locally; pending owner re-review | Section 49 rows driven through C2→Paper with journal/ledger/grpc asserts; frozen N=28 adapter comparison; grpc and browser CancelWorking recorded; stop before Bybit demo |
| 10 | Blocked | Start demo only after 9A acceptance; protect mutating operator commands before enabling demo entries |

Cursor: Stop for owner review. Do not start Increment 10 / Bybit demo until 9A is accepted; the operator authorization preflight is a guarded first action of 10 before any demo entries.

---

# 68. Non-negotiable invariants

## Strategy purity

```text
strategy_core never calls TradingClient
```

## Live/research dependency boundary

```text
runner and trading_engine never depend directly or transitively on
strategy_core, research_engine, research_cli or the Candidate #2 app
```

The separate app **crate**, not a separate process, is the composition root. It may depend on both the generic live crates and `strategy_core`. A shared type crate must not depend on the strategy/research crates.

## Research separation

```text
research_engine does not depend on trading_engine
```

## Audit independence

```text
audit does not use the implementation it is proving
```

## Target/execution separation

```text
strategy chooses desired exposure
executor decides how to reach it
```

## Risk authority

```text
risk may block strategy intent
strategy may not bypass risk
```

## Effective exposure

```text
effective exposure =
authoritative filled position
+ compatible working order remainder
+ unresolved submission reservation
```

No category may be counted twice.

## Unknown exchange outcome

```text
unknown != rejected
```

Unknown exposure remains reserved until resolved.

## Candle determinism

```text
one completed candle
-> at most one model update
```

## Forming-candle rule

```text
unfinished daily candle
-> never changes Candidate #2 signal
```

## Ownership

```text
one executing strategy per account/instrument
```

until a portfolio allocator exists.

## Paper is execution testing

```text
k-paper scenario result != research alpha evidence
```

Paper prices, matching, fees and funding are test fixtures. The frozen 2023-2025 economic comparison and independent audit remain the research record.

## Runner is the control authority

```text
UI presents runner state and requests commands
runner decides readiness, risk, order/account state and command outcome
```

Unknown or stale control-plane state cannot be presented as healthy. Simulation fault controls are server-gated to Paper/local use. UI login alone cannot authorize a capital-bearing runner RPC.

## Holdout

```text
2026 research holdout remains sealed
```

until a future candidate passes the frozen development process.

---

# 69. What success looks like

At the end of this program, `trading_system` should have these clearly separated parts:

```text
strategy_core
    pure deterministic strategy models

research_engine
    historical/economic evaluation

research_cli
    offline research entry points

Candidate #2 app
    thin single-process composition root and model adapter

trading_engine
    generic strategy interface, target/risk/execution integration;
    no dependency on strategy_core or research crates

runner
    generic platform orchestration and operator API;
    no dependency on strategy_core or research crates

audit
    independent verification
```

Two separate repositories complete the operational path:

```text
k-paper
    deterministic exchange/futures/fault laboratory through the real adapters

trading-system-ui
    browser control plane over the runner's authoritative gRPC contract
```

Candidate #2 should be:

```text
Research status:
    rejected as superior alpha

Operational status:
    running in Shadow
    then qualified through k-paper with visible operator controls
    then proven on Bybit demo
```

The platform should be collecting actual execution evidence while Candidate #3 research continues.

That is the transition from:

```text
a trading codebase with strategies
```

to:

```text
a research-driven prop trading platform
```

where new ideas can be tested, rejected, deployed, measured and replaced without coupling research correctness to live execution code.
