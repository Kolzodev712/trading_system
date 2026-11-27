# DeepBook trading readiness — remaining work

> Continuation of [006-deepbook-sui-phasing](006-deepbook-sui-phasing.md). Tracks what is left before **live testnet trading** is turnkey and before **production-ready** DEX levels.

## Problem / context

Layers 0–4 and the gRPC write-path migration are implemented: `--mode dex`, `DexLevelsEngine`, `DeepBookExecutor`, indexer MD, BalanceManager setup binary, integration tests in `tests/integration/`. The code path is wired but **not turnkey** — missing config examples, funding tooling, live smoke validation, and a few operational sharp edges (mandatory Mongo, no shutdown cancel).

## Decision

Track remaining work in the checklist below. **Next session:** work top-down within P0 → P1 unless blocked on testnet funds.

### Already done (do not re-do)

- [x] gRPC write path via `sui-rpc` (no vendored protos / JSON-RPC writes)
- [x] `create_balance_manager` binary + `config/bot.dex.example.json`
- [x] `--mode dex` runner (`dex_runner`, `DexLevelsEngine`, repost-on-fill)
- [x] Config validation: `balanceManagerId` + `walletAddress` for `--mode dex`
- [x] Integration tests relocated to `tests/integration/` (offline fixtures + live indexer `#[ignore]`)
- [x] Redundancy cleanup: shared submit, single gRPC client, `runtime.pool_meta()`

---

### P0 — Required for first live testnet trade

- [ ] **`config/dex_levels.example.json`** — CLI default is `config/dex_levels.example.json` but file does not exist; add bid/ask example for `SUI_DBUSDC` with price/qty aligned to pool `tick_size` / `lot_size` / `min_size`
- [ ] **Deposit helper** — `TradeCommand::Deposit` exists in executor but no user-facing tool; add binary or script to deposit SUI + DBUSDC into `BalanceManager` from wallet coins
- [ ] **Live testnet smoke** — manual or `#[ignore]` integration: funded BM → `cargo run -p runner -- --mode dex --config … --dex-levels-config …` → place limit → confirm in indexer → optional cancel
- [ ] **Ops runbook** — extend `scripts/deepbook/README.md`: faucet SUI, create BM, deposit amounts, env vars, exact run command, expected log lines

### P1 — Operational hardening (before leaving bot unattended)

- [ ] **Mongo for dex-only** — runner hard-fails if `mongo.enabled: false`; either make Mongo optional for `--mode dex` or document as explicit requirement in example config + README
- [ ] **Startup key check** — validate `SUI_PRIVATE_KEY` (or `privateKeyEnv`) is set and matches `walletAddress` before first place (fail fast at bootstrap)
- [ ] **Graceful shutdown** — on Ctrl-C / shutdown signal, cancel open DEX quotes (cancel-all or per-quote) instead of leaving orders on book
- [ ] **Integration tests (offline)** — PTB build from `PlaceLimitOrder` (no submit); load/validate `config/bot.dex.example.json` + `dex_levels.example.json`
- [ ] **Integration test (live `#[ignore]`)** — gRPC submit path smoke (optional; needs funded wallet + BM)

### P2 — Layer 5 / polish

- [ ] **`venue.mode: both`** — verify DeepBook MD sidecar + CEX MD + dual health/readiness under `--mode both` (example: `config/bot.both.example.json`)
- [ ] **Clippy cleanup** — resolve any remaining runner warnings (e.g. unreachable match arms in `config.rs` if still present)
- [ ] **Design log #006 append** — mark Layer 4 complete in phasing table once P0 smoke passes

### P3 — Explicitly deferred (not blocking testnet trial)

- [ ] TUI / web UI support for DEX levels and `MonitoringData.dex` controls
- [ ] `--mode strategies` wired to `DexStrategy` / `DexStrategyEngine` (not just fixed `--mode dex` quotes)
- [ ] `TradeCommand::Liquidate` / `Swap` on DeepBook (currently `UnsupportedCommand`)
- [ ] Mainnet defaults + funded mainnet smoke
- [ ] Optional gRPC DexQuote CRUD (parallel to CEX `CreateLevel`)
- [ ] Auto-create `BalanceManager` on first run if `balanceManagerId` missing
- [ ] DEEP token balance / `payFeesInDeep: true` path if testnet rejects input-token fees
- [ ] Coin decimals via `GetCoinInfo` everywhere (heuristic fallback remains for some paths)

## Alternatives

- **Single mega-checklist in #006 only** — rejected; #006 is already long; this log is the session handoff for “what’s left to trade.”
- **Skip deposit helper; document manual DeepBook UI** — rejected for v1; too much friction for repeat testnet runs.

## Consequences

- P0 items are **user-facing blockers**; without them every testnet attempt requires rediscovering steps from chat.
- P1 items affect **unattended** operation; safe to trade manually once P0 is done.
- P3 stays out of scope until testnet loop is proven stable.

## Verification

| Milestone | Done when |
|-----------|-----------|
| P0 complete | Operator can follow README only: fund → deposit → run dex → see order on testnet indexer |
| P1 complete | Shutdown leaves no stray quotes; bad key fails at startup; `cargo test --test integration internal::dex` + config tests pass |
| Trading “ready” | P0 + P1 + live smoke recorded in **Implementation results** below |

## Implementation results

*(Append as items ship — date + one line per checkbox.)*
