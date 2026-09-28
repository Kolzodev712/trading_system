# Prop Trading Productionization and Research Architecture Plan

**Status:** Implementation blueprint  
**Date:** 2026-09-28\
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

Research logic currently lives under paths such as:

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

This mixes:

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

That separation should be corrected before additional research infrastructure is added.

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
|   `-- strategy_core/                 NEW
|
|-- src/
|   |-- exchange_engine/
|   |-- market_data_engine/
|   |-- trading_engine/
|   |-- indicator_engine/
|   |-- asset_engine/
|   |-- gateway/
|   `-- runner/
|
|-- research/
|   |-- research_engine/               NEW
|   `-- research_cli/                  NEW
|
|-- audit/
|   |-- donchian_daily_v1/
|   `-- btc_price_ma_long_flat_v1/
|
`-- data/
```

Exact directory naming may be adjusted to fit workspace conventions, but the dependency boundaries are mandatory.

## 6.1 External repositories and responsibility

| System | Owns | Does not prove |
| --- | --- | --- |
| `strategy_core` / `research_engine` in `trading_system` | Frozen model and fast historical economic evaluation | That the live execution stack works |
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

Both research and production must call the same pure model.

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

---

# 9. research_cli responsibility

Create a small CLI crate over research_engine.

The current research commands should eventually leave `runner`.

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

Pure Candidate #2 model currently represented by:

```text
src/trading_engine/.../btc_price_ma_long_flat_v1.rs
```

The pure model should move into strategy_core.

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

should eventually be invoked from research_cli.

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

Candidate #2 currently exists as a pure/offline research model, not a live strategy factory implementation.

Create a runtime adapter in trading_engine.

Conceptual dependency:

```text
strategy_core::BTCPriceMALongFlatV1
             |
             v
trading_engine runtime adapter
             |
             v
StrategyIntent
```

The runtime adapter should own only live lifecycle state.

It must never contain a duplicate formula for the strategy.

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

Exchange-specific parsing stays outside strategy_core.

---

# 19. Extend strategy runtime input

The live `Strategy` interface is currently orderbook-oriented with a default daily seed callback.

Introduce an explicit completed-candle callback with a default no-op implementation.

Conceptually:

```text
async fn on_completed_candle(...)
```

Existing strategies must continue compiling unchanged.

StrategyManager should route a completed candle only to strategies assigned to that pair.

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
StrategyManager
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

The live adapter should expose/retain at least:

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

Run the actual Candidate #2 runtime -> risk -> executor -> `TradingClient` -> `k-paper` -> exchange state/reconciliation path, with the Trading System UI connected to the same runner. `ExecutionMode = Exchange` and `venue = Paper / local simulation`; start entries paused and explicitly resume for a test. The simulator's market stream/orderbook is **not** a completed-daily-candle source by itself: supply controlled completed candles through the same runner-owned candle interface used in Shadow, while the Paper market-data adapter supplies executable orderbook/freshness state. Compare model decisions with the frozen offline expectations without turning `k-paper` into a research backtester.

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

Desired:

```text
trading_core
     ^
     |
strategy_core
   ^       ^
   |       |
   |       +------------- research_engine
   |                           ^
   |                           |
trading_engine             research_cli
   |
   v
exchange_engine
```

The exact low-level crate dependency direction may need adjustment around existing shared types, but preserve these architectural truths:

```text
strategy_core does not depend on trading_engine

research_engine does not depend on trading_engine

trading_engine may depend on strategy_core

research_engine may depend on strategy_core

independent audit does not import strategy_core/research_engine logic under test
```

If `CompletedCandle` is sufficiently generic, place it in `trading_core` rather than strategy_core to avoid dependency inversion.

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

---

# 55. Increment 0 — baseline lock

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

Preserve old public re-exports temporarily if needed to keep compilation manageable.

Acceptance:

- Candidate #2 frozen decision logs still reproduce;
- no economic outputs change;
- trading_engine compiles;
- research tooling compiles.

Stop and report.

---

# 57. Increment 2 — extract research_engine

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

## Increment 5A — Trading System UI Shadow observability

After the runner's authoritative status/proto fields exist, update the separate `trading-system-ui` repository and generated client. Show Candidate #2's **rejected-as-alpha / operational-baseline** distinction, last completed candle, signal/target, Shadow execution state, venue/environment, pair readiness and reason, freshness and errors. Keep model status visible when execution is paused. Connect to the actual runner; the old Rust TUI is not the deliverable.

Acceptance: proto copies and generated types match; UI unit/build checks pass; a disconnected/stale stream is visibly unready; Shadow displays zero exchange orders while still showing model decisions. Stop and report both repository revisions.

## Increment 5B — existing Paper connectivity preflight

Run the existing `k-paper` server and `trading_system` Paper adapters with the documented Spot/levels configuration: market WS, trading WS, REST balance/open-order/trade reconciliation and asset rules. Connect the browser UI to the same runner and verify venue labeling. Record exact working endpoints and identify missing Candidate #2 futures operations and scenario controls.

This is a wiring smoke test, **not** the Stage C futures or fault gate. Do not bypass account-mode, risk or readiness failures by merely setting the adapter's instrument type to Futures. Stop and report.

---

# 61. Increment 6 — CEX recovery hardening

Implement:

- Bybit open-order pagination;
- valid linear selector;
- persistent submission state;
- unknown outcome recovery by orderLinkId;
- restart recovery;
- immediate reconciliation triggers.

Add the Paper-side query/fault hooks needed to verify lost-ack recovery by client order ID without assuming a missing open order was rejected. Keep Bybit's `orderLinkId` pagination and error handling venue-specific. Use isolated tests now; the full integration matrix follows after futures/risk/ledger work.

Acceptance:

- unknown outcome cannot produce duplicate exposure;
- unresolved reservation survives restart;
- exchange order found by client ID is imported;
- truncated open-order pages are impossible.

Stop and report.

---

# 62. Increment 7 — futures state correctness and readiness

Implement:

- authoritative futures-position synchronization model;
- PositionSyncPending;
- no double application of fills + position events;
- per-pair readiness;
- market-data freshness;
- one executing strategy owner per account/instrument;
- startup account-mode/leverage fail-closed behavior.

In `k-paper` and the Paper adapter, implement the **minimum explicit linear-perpetual one-way BTCUSDT test contract** required for Candidate #2: order/fill -> position and account updates, REST position snapshot/recovery, signed quantity, reduce-only exit enforcement, required leverage/margin readiness, instrument sizing, fees and controlled funding where the ledger tests need it. Version/document Paper's intentional differences from Bybit. Do not reuse spot base/quote reservation as if it were futures margin. Feed controlled completed daily candles to the runner separately from Paper orderbook data.

Acceptance:

- all event-order tests pass;
- stale/unready market data blocks exposure increases;
- reductions still possible where designed;
- second executing BTCUSDT strategy rejected;
- Paper position and REST/WS account state agree with independently inspected fills; one-way/reduce-only and restart-with-existing-position tests pass for the simulated contract.

Stop and report.

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

Stop and report.

## Increment 9A — full-stack K-paper qualification

Run Stage C and the section 49 matrix with Candidate #2's actual runtime model and futures Paper adapter, not a manually fabricated strategy order. Use deterministic completed-candle inputs and separate market-orderbook scenarios. Assert target, risk decision, placed order, unknown/working/filled/reserved state, position, reconciliation, ledger and UI state. Restart the runner while `k-paper` keeps its state. Exercise pause/cancel/flatten/resume through the browser and confirm results against the Paper server.

Acceptance: no unintended duplicate exposure in any scripted case; failed/unknown outcomes fail closed; all required status and operator actions have recorded evidence; remaining Paper/Bybit semantic differences are listed for Stage D. Stop and issue a Paper readiness report. **Do not enter Bybit demo until this gate passes.**

---

# 65. Increment 10 — Bybit demo

After Increment 9A passes, run Candidate #2 through:

```text
Completed candle
    ->
strategy_core model
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

# 67. First task Cursor should execute

Cursor should **start with Increment 0 and Increment 1 only**.

Do not start runtime wiring yet.

Immediate instruction:

```text
Read this entire file.

Then inspect current master rather than relying on old line numbers.

1. Record the current repository SHA.
2. Run the frozen Candidate #1 and Candidate #2 audit/reference checks that are available locally.
3. Run focused strategy/research tests.
4. Create a new strategy_core workspace crate.
5. Move/extract only the pure BTCPriceMALongFlatV1 model into strategy_core.
6. Preserve exact frozen signal behavior.
7. Update imports/re-exports as required without moving research/backtest logic yet.
8. Add/retain parity tests proving the extracted model generates the same frozen Candidate #2 N=14/N=28/N=70 decision transitions.
9. Do not change any research rules, datasets, PnL, gates, or audit expectations.
10. Do not live-register Candidate #2.
11. Do not implement Shadow mode yet.
12. Do not change CEX execution in this increment.

At completion report:
- current starting SHA;
- files added/changed;
- dependency graph changes;
- exact tests run;
- exact results;
- any unrelated existing failures;
- evidence that Candidate #2 frozen decisions are unchanged.

Then stop.
```

---

# 68. Non-negotiable invariants

## Strategy purity

```text
strategy_core never calls TradingClient
```

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

At the end of this program, `trading_system` should have four clearly separated internal systems:

```text
strategy_core
    pure deterministic strategy models

research_engine
    historical/economic evaluation

trading_engine
    live strategy lifecycle, target/risk/execution integration

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
