# Current architecture

**As of:** 2026-09-29

This file is the current architecture reference: what is implemented and how it is wired. Increment status, open work, and next gates live in [PROGRESSION.md](PROGRESSION.md). A narrative of the same architecture is [Trading-System-Analysis.txt](Trading-System-Analysis.txt). Engine write-ups under `architecture/` describe in-process services and may lag crate layout; they are not a production or mainnet status.

The system is **not** production complete and **not** mainnet ready. Candidate #2 is an operational baseline, not validated alpha.

## Three repositories

| Repository | Owns | Does not prove |
| --- | --- | --- |
| `trading_system` | Pure models (`strategy_core`), historical research (`research_engine` / `research_cli`), live runner, risk, journal, ledger, reconciliation, source proto/gRPC, Candidate #2 composition app (`btc_price_ma_app`) | That a browser view is authoritative; that Candidate #2 is validated alpha; mainnet |
| `k-paper` | Local exchange for deterministic Paper/fault work | Bybit API parity, alpha, or Demo/mainnet economics |
| `trading-system-ui` | Browser presentation, login UI, operator confirmations | Exchange state, recovery decisions, or mutating-RPC authorization (Envoy/runner token) |

Browser path: UI → Envoy gRPC-Web → runner `TradingCliService`. The UI does not read `bot.json` and does not talk to k-paper or Bybit. The Rust TUI is a separate gRPC client; the browser UI is the operator console.

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

## strategy_core close-window API

`libs/strategy_core` is a pure model. It does not load datasets, drop forming bars, fetch klines, or submit orders. Callers supply **completed close windows**.

- `on_candle(state, lookback, window)` requires `window.len() == lookback`. For primary N=28, that is exactly 28 completed daily closes **including the current completed close**.
- `log_spread` / `geometric_mean` / `wants_long` use that same window: log(current close) minus the mean of log closes over the window.
- `Step::NotReady` is unused by `on_candle`; a short window is a hard error, not a soft warmup. Live/research adapters therefore keep a rolling buffer and only call `on_candle` once they have N completes.
- A historical series that first initializes N closes then steps the next complete needs **N+1** completed bars before the first ready live step (29 UTC dailies for N=28).
- Entry freshness (`max_entry_age_secs`, forming-bar drop, wall-clock `as_of`) lives in the adapter / `completed_dailies`, not in `strategy_core`.

## Paper and Demo venues

k-paper is the local futures/fault laboratory (linear BTCUSDT, independent REST, in-memory account lost on Paper process restart). Bybit Demo is a bounded operational lab with `demo:true`; it is not mainnet. Flatten RPC success is not treated as flat; independent venue + runner verification is required.

## Operator flatten

Operator reduce-only exits use one journaled path in `trading_engine` (`submit_operator_reduce_only_exits`), not `trading_core`. FlattenAndPause, restart `on_stop=flatten`, `TRADING_FLATTEN`, and the former gRPC TSM `open_order` fallback share it. The runner persists a stable client ID and `operator_flatten` intent (`Prepared` then `Submitted`) before send. Recovery looks up that client ID. A journaled flatten off `Acknowledged` becomes `Terminal` after a verified fill or a verified flat venue; an unknown first exit does not mint a second client ID. Journal append failure halts and does not send.

FlattenAndPause latches first, REST-discovers working orders (including those missing from local caches), then CancelWorking. A successful venue cancel acknowledgement is not an empty book. Remaining working orders are REST-verified before reduce-only. An unverified CancelWorking (including failed GET `/open_orders`, leftover working orders, or a fill racing cancel) is reported as `accepted=false`; working orders are not treated as cancelled; the pause/flatten latch stays held; reduce-only is not sent.

## Operator controls

`PauseEntries`, `CancelWorking`, `FlattenAndPause`, and `Resume` are distinct from legacy `SetTradingEnabled`. Mutating RPCs on non-Paper venues require `TRADING_OPERATOR_TOKEN` at the runner/proxy boundary; Envoy injects the token. Unset token is Paper/lab only.

## Trading report observer

`src/trading_report` / `trading-report` is a one-shot read-only observer of separately running Levels and Strategies processes. It is not imported by `runner` or `trading_engine` and does not live inside those containers. Optional Mongo persistence stores the exact `report.json` bytes. A host systemd timer starts a short-lived reporter container; there is no reporting daemon.

Operator commands and timer install: [TRADING_REPORT_OPERATOR.md](TRADING_REPORT_OPERATOR.md). Feature note: [features/trading-report-v1.md](features/trading-report-v1.md).

## Related component docs

Progression: [PROGRESSION.md](PROGRESSION.md). In-process engines: [architecture/README.md](../architecture/README.md). Venue labels: [VENUE_MODES.md](VENUE_MODES.md). Commands: [COMMAND_PLANE.md](COMMAND_PLANE.md). Data plane / Zenoh opt-in: [PLATFORM_DATA_PLANE.md](PLATFORM_DATA_PLANE.md).
