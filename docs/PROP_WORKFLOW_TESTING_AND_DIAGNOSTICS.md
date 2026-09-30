# Prop workflow testing and diagnostics

Status: **implementation required**. This document is an acceptance contract, not a claim that the suite exists or that Bybit Demo or live trading is qualified.

Audit baseline: `trading_system` `498f5903`, `k-paper` `c10148fe`, `trading-system-ui` `2c13bea2` (2026-09-30). Recheck heads before implementing. The trading runner and engine must continue to have no dependency on `strategy_core`, `research_engine`, `research_cli`, or the Candidate #2 composition app.

## Why the current suite is insufficient

- `tests/integration/internal/workflows.rs` tests manager and risk methods in memory. It does not launch a runner, exchange, control plane, journal, or ledger. Keep such tests, but classify them as component integration; do not call them end-to-end evidence.
- `tests/integration/external/...` is organized by service and exchange, mostly `#[ignore]`. `just test-integration` and `just ci` do not execute those ignored cases. The external helper's Paper branch is `todo!()`, and its Bybit config has `demo: false`. Do not use that builder for a mutating Bybit workflow.
- The real Paper/prop qualifications are in `scripts/paper_ack_amend_gate.sh`, `paper_signoff_gate.sh`, `inc9a_matrix.sh`, and related scripts. They exercise meaningful behavior, but use fixed ports, fixed `/tmp` paths, textual log checks, and manual evidence review. They are not a repeatable default gate. Preserve their evidence packs while migrating their assertions.
- `src/runner/src/bin/test_runner.rs` describes a no-order mode, but the inspected startup path does not install a `test_open_order` guard. Do not use its description as a safety guarantee; either prove and enforce it or retire the target.

## Test layers and commands

Keep component tests close to each crate. Add scenario-oriented workflow targets under `tests/workflows/` (or a small dedicated test harness package if process orchestration makes the root integration target unwieldy). The test name and result must identify workflow, venue, instrument, and execution mode.

| Layer | What it proves | Required command behavior |
| --- | --- | --- |
| Fast component and parity | Risk, level state, submission journal, unique fills, frozen strategy decisions, offline/research-to-runtime parity, Shadow guard | Runs with `just test-fast`; no processes, credentials, or network. |
| Paper process workflows | Actual runner binary + k-paper + gRPC + independent Paper REST, with fresh state and deterministic faults | `just test-paper` runs without human steps, creates its own run directory and ports, exits nonzero on any invariant failure. Required in the local release gate. |
| UI control workflow | UI through Envoy to a running Paper runner; venue/mode labels, auth, controls, confirmations, stale status | A small automated browser suite; run against the same isolated stack. Vitest remains for transforms/components. |
| Bybit Demo read-only | Correct endpoint/environment, account snapshot, market data, auth denial, reconciliation while paused | Separate `just test-demo-readonly`; no mutating RPCs or REST requests. |
| Bybit Demo bounded mutation | Common supported workflows on the real Demo API within an explicitly authorized exposure/time budget | Separate opt-in command, never in default CI. Preflight, limits, cleanup, and independent end-state checks are mandatory. No mainnet/testnet fallback. |

`just ci` must no longer suggest end-to-end coverage when only manager tests ran. Add `just ci-local` (or revise `ci`) as the mandatory local release gate: it runs fast and Paper workflows and fails if the pinned k-paper binary or any required dependency is unavailable. Report the exact layers run and `PASS`, `FAIL`, or `NOT_RUN` for each. Never convert an unrun or unsupported scenario into a pass.

## Reuse behavior across venues

Write a **test-only** `VenueHarness` interface and one shared set of scenario functions. Implement adapters for Paper and Bybit Demo. The harness drives the actual runner through gRPC and reads venue truth independently through the venue's REST API. It must not substitute the runner's exchange cache for independent truth.

The adapter supplies venue startup/connection, an account snapshot, order lookup by client ID, capability flags, and optional deterministic fault controls. The shared scenarios assert normalized economic facts: signed position or Spot base wallet, working order IDs and quantities, fills by unique execution ID, journal state, level ownership/catalog, risk reservation, and operator latch. Do not generalize exchange-specific wire details into production traits solely to make tests look uniform.

| Shared scenario | Paper | Bybit Demo |
| --- | --- | --- |
| Paused startup, venue/mode/auth, snapshot reconciliation | Deterministic | Read-only default |
| Level rest, amend, disable, delete; another level untouched | Deterministic | Mutating opt-in where the venue supports the operation |
| Spot buy, sell, partial/full fill; wallet = runner = catalog/ledger | Forced fills | Bounded real fills only; no assumption that a resting order fills |
| Futures long/short, reduce/flip, margin and cap checks | Deterministic | Bounded supported subset, with real venue rules |
| Lost ACK, duplicate fill, failed REST, stale book, restart | Fault injection | Only reproducible faults; otherwise `NOT_SUPPORTED`, never a green result |
| Candidate #2 close-window/Hold, Paper Shadow zero mutations, Paper Exchange journal/ledger/restart/flatten | Fixture + real runner composition | Paused/read-only first; bounded mutation separately authorized |

Keep strategy signal quality and profitability out of this suite. The workflow suite proves technical execution and safety, not validated alpha. A new strategy should supply its own frozen model/parity fixtures and reuse the same runner, exchange, risk, recovery, and operator workflow tests. Preserve the runner/engine dependency boundary.

## Required Paper workflows

Migrate existing Paper scripts into executable scenarios rather than merely invoking them from `cargo test`. The initial mandatory set is:

1. Levels Spot: post two owned orders; delayed wrong ACK cannot cross-attach; partials and duplicate reports count once; price-only amend at a cap keeps the same ID; disable and delete REST-verify owned orders only; buy then sell changes wallet and runner once; restart while disabled restores inventory and ownership; unknown cost basis is reported as unknown; a stale book and a REST failure prevent new exposure.
2. Levels Futures: rest and fill, independent REST position equals runner; test adverse same-side working exposure, sell/short or flip and reduce-only behavior with explicit venue rules; disable removes working orders without silently flattening the position.
3. Prop composition: frozen candle and transition parity, forming-bar exclusion and fresh/stale Hold; Shadow stop/restart makes zero venue mutations; Paper Exchange entry, unique ledger fill, journal before send, lost ACK + restart, cancel/pause/flatten verification, and REST/WS disagreement blocking new exposure. Candidate #2 remains an operational baseline.
4. Control plane: authenticated mutations, denied unauthenticated mutations, correct venue/environment/mode, accurate UI state after status stream loss/reconnect. A successful RPC must not be mistaken for independent proof of flat.

Each scenario must have an independent oracle and assert every economically consequential transition. A process starting, an RPC returning success, or a log line containing `PASS` is insufficient. A negative-control test must deliberately introduce a position mismatch or failed REST verification and show the scenario fails closed. Reuse prior evidence packs as regression inputs, not as permanent substitutes for executable assertions.

## Harness isolation and artifacts

- Allocate ports dynamically. Give each run a new temporary directory, account/catalog namespace, client-order-ID prefix, journal, ledger, snapshot, and config. Do not overwrite a prior run. Never kill a process merely because it owns a fixed port; supervise and clean up only children the harness spawned.
- Use readiness probes with bounded deadlines instead of fixed sleeps. Preserve the Paper process across a runner restart. Check that the child actually exited and the restarted process uses the expected config and binaries.
- At every assertion, compare the gRPC status and catalog/journal/ledger with a fresh independent venue REST snapshot. Compare signed quantities with exact decimal arithmetic and the venue's step-size tolerance, not floating-point approximations. Spot ledger deltas are signed for sells; starting inventory must be recorded separately.
- On success or failure, write an immutable per-run artifact directory: `manifest.json` (git SHAs, binary hashes, config hashes, capability matrix, start/end UTC times), `results.json` (scenario, assertion, expected/actual, `PASS`/`FAIL`/`NOT_SUPPORTED`), structured runner/Paper/proxy logs, gRPC snapshots, REST snapshots, journal, ledger, catalog, and a chronological event timeline. Redact credentials and operator tokens. Do not reset or delete evidence on rerun.
- Tests must fail on a missing oracle, unknown/mismatched venue, journal write failure, unverified cancel, unresolved submission, unintended order, leftover working order, or unverified final account. Cleanup is a separate assertion; no `|| true` may turn a failed safety action into success.

## Production diagnostics contract

Use the existing `tracing` infrastructure. Add structured spans/events at the boundaries where a decision or durable state changes, with stable reason codes rather than only free-text messages. Carry `run_id`, `scenario_id` when testing, `venue`, `environment`, `execution_mode`, `instrument`, `pair`, `level_id` or `strategy_id`, `client_order_id`, `exchange_order_id`, `execution_id`, `reconciliation_id`, and `operator_id` where applicable. Never log API secrets, tokens, or full credential-bearing config.

Emit a single decision event for each meaningful post/amend/hold/cancel/flatten choice: action, stable reason code, current and proposed exposure, relevant cap, market-data age, and whether REST verification succeeded. Emit journal transitions, ACK correlation, fill deduplication, ownership changes, and reconciliation source/amount as state transitions. Use `WARN`/`ERROR` for fail-closed conditions and expose their current reason in health/status. Sample or aggregate repetitive hot-path `Hold`/skip decisions; do not produce thousands of identical INFO lines. Preserve correlation across gRPC, runner, and exchange adapters.

The test harness must assert reason codes for intentional blocks and attach a compact failure timeline keyed by IDs, with the first failed invariant visible without reading a full log. UI may show the last operator-relevant block reason; it must not become the only diagnostic source. Structured JSON output can be enabled for the test run while keeping production log filtering configurable.

## Implementation order and acceptance

1. Make the existing suite honest: classify component tests correctly, remove unsafe implicit Bybit mutation defaults, and repair/retire the misleading `test_runner` claim. Add commands that say exactly what ran.
2. Build the isolated Paper harness and shared venue contract. Migrate the Spot two-level/position/cancel/restart workflows first. Demonstrate that a deliberately corrupted position or failed REST lookup makes the gate fail.
3. Add the remaining Futures, prop composition, Shadow, and control-plane scenarios. Move assertions out of the long shell scripts; retain scripts only as thin launchers or historical evidence once equivalent tests pass.
4. Implement structured diagnostic events and assert their reason codes in the workflow tests. Add the small UI browser path.
5. Add the Bybit Demo adapter. Run read-only cases by default. Only an explicitly authorized, bounded mutating invocation may place Demo orders. Record supported/unsupported scenarios and exact end-state.

Acceptance: from a clean checkout of pinned `trading_system`, `k-paper`, and UI revisions, one command runs the local technical qualification without manual clicks or hand-collected snapshots. It is repeatable with a fresh namespace, fails on the negative control, leaves no unowned children or working orders, and produces a human-readable verdict plus machine-readable artifacts. No claim of Bybit Demo or live qualification follows from a green Paper suite.
