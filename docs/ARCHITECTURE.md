# Current architecture

**As of:** 2026-09-29  
**Scope:** Implemented decoupling, three-repository roles, Paper qualification, and bounded Bybit Demo qualification.  
**Not claimed:** production complete, mainnet ready, or Candidate #2 as validated alpha.

This file is the current architecture reference. The increment record is [PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md](PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md) (sections 6, 65, 67). A narrative of the same facts is [Trading-System-Analysis.txt](Trading-System-Analysis.txt). Engine write-ups under `architecture/` describe in-process services and may lag crate layout; they are not a production or mainnet status.

## Completed at stated scope

| Claim | Status |
| --- | --- |
| Research / live / operator decoupling | Implemented |
| Three git repositories with distinct authority | Implemented |
| Increments 0–9A Paper qualification | Accepted locally |
| Increment 10 bounded Bybit Demo execution | Accepted (synthetic signal) |

No further Demo run is required to keep these docs current.

## Three repositories

| Repository | Owns | Does not prove |
| --- | --- | --- |
| `trading_system` | Pure models (`strategy_core`), historical research (`research_engine` / `research_cli`), live runner, risk, journal, ledger, reconciliation, source proto/gRPC, Candidate #2 composition app (`btc_price_ma_app`) | That a browser view is authoritative; that Candidate #2 is validated alpha; mainnet |
| `k-paper` | Local exchange for deterministic Paper/fault work | Bybit API parity, alpha, or Demo/mainnet economics |
| `trading-system-ui` | Browser presentation, login UI, operator confirmations | Exchange state, recovery decisions, or mutating-RPC authorization (Envoy/runner token) |

Browser path: UI → Envoy gRPC-Web → runner `TradingCliService`. The UI does not read `bot.json` and does not talk to k-paper or Bybit. The Rust TUI remains a separate client, not the planned operator layer.

## Implemented decoupling

```text
strategy_core          pure ModelState + CompletedCandle → decision
        ^                      ^
        |                      |
research_engine/cli     btc_price_ma_app  (composition binary)
                               |
                               v
                    generic runner + trading_engine
                               |
                               v
              exchange / market data / risk / gateway
                               |
              +----------------+----------------+
              |                |                |
           k-paper         Bybit Demo      (mainnet: not approved)
```

Invariants that shipped:

- `strategy_core` has no exchange, Tokio, gRPC, Mongo, or research-accounting dependencies.
- `runner` and `trading_engine` do not depend on `strategy_core`, `research_engine`, `research_cli`, or `btc_price_ma_app`.
- Candidate #2 is injected in-process by `btc_price_ma_app --mode strategies`. The default `runner` binary does not instantiate it.
- Research binaries do not link the live runner.
- Shared types such as `CompletedCandle` live in `trading_core`.

Python `strategy_dsl` / `architecture/10-strategy-engine.md` is a **separate proposed IR path**, not the Candidate #2 production model.

## Paper qualification (Increment 9A)

Accepted locally on `trading_system` `9dab54e`, k-paper `590f3ad` / record `6d03acc`, UI `10496e7` (source review plus reported matrix; that review did not rerun the lab). Scope: section 49 rows through Candidate #2 → Paper, frozen N=28 adapter comparison, grpc and browser CancelWorking. This is a controlled futures/fault laboratory qualification, not Demo and not alpha.

## Demo result (Increment 10)

Accepted as a **bounded Bybit Demo operational qualification** on committed pack `docs/inc10-evidence-measured-e923fa8/` (`042eac9`; runner `e923fa8`, UI `923dd30`, k-paper `6d03acc`). Independently flat before enablement; one 0.001 BTC buy and one 0.001 BTC reduce-only sell matching the ledger and Demo history; journal entry path on the buy; ended flat with entries paused and the flatten latch held. The run used a **synthetic** fresh-entry fixture on the real clock. 90 ms is exchange-timestamp to local receive, not full order-execution latency.

That verdict does **not** validate Candidate #2 as a strategy and does **not** approve mainnet.

## Explicitly open

1. Journal and recover FlattenAndPause’s reduce-only submission (today `trading_client().open_order` with no submission-journal append; Demo sell `orderLinkId` was empty).
2. Propagate a CancelWorking failure from FlattenAndPause (today `let _ = self.trading_control.cancel_working(...)`).
3. Validate Candidate #2 as a strategy. The Demo run tested execution using a synthetic signal.
4. Separate mainnet readiness and approval work. Do not document the system as production complete or mainnet ready.

Increment 11 (research-framework cleanup for Candidate #3) is not started until the owner asks.

## Operator controls (implemented, not mainnet)

`PauseEntries`, `CancelWorking`, `FlattenAndPause`, and `Resume` are distinct from legacy `SetTradingEnabled`. Mutating RPCs on non-Paper venues require `TRADING_OPERATOR_TOKEN` at the runner/proxy boundary; Envoy injects the token. Unset token is Paper/lab only. Flatten RPC success is not treated as flat.

## Related component docs

In-process engines: [architecture/README.md](../architecture/README.md). Venue labels: [VENUE_MODES.md](VENUE_MODES.md). Commands: [COMMAND_PLANE.md](COMMAND_PLANE.md). Data plane / Zenoh opt-in: [PLATFORM_DATA_PLANE.md](PLATFORM_DATA_PLANE.md).
