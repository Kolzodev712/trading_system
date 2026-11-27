# DeepBook V3 on Sui — integration phasing

> Builds on [002-dex-preparation](002-dex-preparation.md) (venue split, `VenueProvider`, `dex_engine`). Supersedes Solana-first scope in Proposal 01. Reference: [Proposal 01-sui-deepbook](../../proposals/01-sui-deepbook-integration.md).

## Problem / context

We need the first **real** DEX venue: **DeepBookV3 on Sui** — an on-chain CLOB ([design](https://docs.sui.io/onchain-finance/deepbookv3/design)). The scaffold is shipped (`dex_engine`, mock provider, `MonitoringData.dex`, `venue.mode`). CEX (Bybit/Paper levels) must not regress.

DeepBook’s model maps cleanly to our stack if we respect its layers:

- **`Pool`** — one market; `Book` → `State` → `Vault` on every order ([place-limit flow](https://docs.sui.io/onchain-finance/deepbookv3/design))
- **`BalanceManager`** — funds one trader across **all pools** (1:1 with on-chain `Account`)
- **Indexer** — HTTP read API for books, orders, trades (no signing)
- **SDK + Sui RPC** — PTBs for place/cancel/deposit; tx digest = our `PendingTransaction`

Question: how to **split work** so each slice is testable, UI-visible, and does not block CEX?

## Decision

Adopt **horizontal layers with thin vertical milestones**, aligned to DeepBook’s read vs write boundary and our existing crates.

### Layer 0 — Shared types & config (no network)

**Crate:** `dex_engine::chains::sui::types` (+ small `deepbook_types` module if needed)

- `SuiDeepBookConfig`: network, RPC URL, indexer URL, package/registry IDs, pool names, `balance_manager_id`, key env var
- `DeepBookPoolMeta` from indexer `GET /get_pools`: `pool_id`, `pool_name`, `tick_size`, `lot_size`, `min_size`, asset decimals ([contract-info](https://docs.sui.io/onchain-finance/deepbookv3/contract-information) for mainnet IDs; testnet discovered via indexer)
- **Tick/lot conversion** (single module, heavy unit tests): human price/qty ↔ on-chain units
- `VenueId::Sui`, `DexChain::Sui`; `select_provider()` branches on config (replaces mock)

**Exit:** runner starts with `venue.mode: dex`, config validates, mock replaced by no-op or stub provider that loads pool metadata.

### Layer 1 — Read plane (indexer only, no keys)

**Crates:** `market_data_engine/exchanges/sui/deepbook_indexer`, gateway monitoring

- HTTP client: `/get_pools`, `/orderbook/:pool_name`, `/trades/:pool_name`, `/orders/:pool/:balance_manager_id` ([indexer API](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer))
- Poll ~1 Hz; map to existing `MonitoringData.orderbooks`, `trades`, optional `orders`
- **Public testnet indexer** first: `https://deepbook-indexer.testnet.mystenlabs.com`
- **First pool:** `SUI_USDC` (liquid, matches CEX mental model)

**Exit:** Web UI shows live DeepBook book + trades in `venue.mode: dex`; no wallet required.

### Layer 2 — Wallet state (Sui RPC read + BalanceManager)

**Crate:** `dex_engine::chains::sui` — read half of `VenueProvider`

- Create or load `BalanceManager` (SDK); persist ID in config/mongo
- `bootstrap_state` / `reconcile`: Sui `getBalance` + indexer open orders → `DexWalletBalances` + internal order map
- Populate `MonitoringData.dex` (balances, pending txs empty)
- Optional: `TradeCommand::Deposit` manual funding path for testnet

**Exit:** DEX panel shows SUI/USDC balances; reconciliation loop compares indexer orders vs `DexState`.

### Layer 3 — Write plane (SDK execution)

**Crates:** `trading_core::venue::dex` + `dex_engine::chains::sui::deepbook_sdk`

- Extend `TradeCommand`: `PlaceLimitOrder`, `CancelOrder`, `CancelAllOrders` (defer `Swap`, margin)
- `VenueProvider::execute`: build PTB → simulate → sign → submit → track digest in `pending`
- `stream_updates` / reconcile: poll tx status + indexer until `Placed` / `Canceled` / `Filled`
- Fee mode: `pay_fees_in_deep: false` initially (input-token fees); document DEEP balance later

**Exit:** integration test on testnet — place limit, visible in indexer, cancel; gRPC shows pending → confirmed.

### Layer 4 — DEX levels strategy (trading_engine)

**Crate:** `trading_engine::dex::levels` (new; **do not** extend CEX `levels/`)

- Config: `config/dex_levels.json` or `venue: sui` tag in levels file
- Reuse *concepts* (fixed price, repost-on-fill, risk caps) with **tick/lot/min_size** validation from `DeepBookPoolMeta`
- Runner mode: `--mode dex` with DEX level executor calling `TradeCommand` only

**Exit:** one pool, two levels (bid/ask), repost on fill via indexer trade events.

### DEX trading model vs CEX `levels/` (follow-up)

**What “do not reuse CEX levels” means:** do not route DeepBook through `trading_engine::levels::{LevelManager, LevelExecutor}` or the existing gRPC level CRUD (`CreateLevel` → Mongo → `TradingClient<OrderRequest>`). That stack is **CEX-only** by design ([002](002-dex-preparation.md)): it assumes exchange API keys, `OrderRequest`, pair expansion from `bot.json`, and exchange tick/step from `AssetService`.

**Do we need a new system?** Yes — a **parallel DEX quote system**, not a rewrite of the whole trading engine.

| Layer | CEX today | DEX (to build) | Chain-specific? |
|-------|-----------|----------------|-----------------|
| Config | `levels.json` + `LevelConfig` | `dex_levels.json` + `DexQuoteConfig` (new) | No (serde shape) |
| State | `LevelManager` + Mongo | `DexQuoteManager` (new, in-memory + optional mongo) | No |
| Policy loop | `levels/event_loop.rs` | `dex/levels/loop.rs` or extend `DexStrategyEngine` | No |
| Execution | `LevelExecutor` → `TradingClient` | `DexQuoteExecutor` → `TradeCommand` → `VenueProvider` | No at trait boundary |
| Venue I/O | `exchange_engine` | `dex_engine::chains::<chain>/` | **Yes** (Sui/DeepBook first) |
| Funding | Exchange balances | `BalanceManager` object ID | **Yes** (DeepBook on Sui) |
| Precision | Exchange `tick_size` / `step_size` | Pool `tick_size` / `lot_size` / `min_size` | **Yes** (per pool metadata) |
| UI/gRPC | `GetLevels` / `CreateLevel` (CEX) | Reuse `MonitoringData.dex` + optional `DexQuote` RPC later | TBD |

**Core types that already exist (chain-agnostic, in `trading_core::venue::dex`):**

- `VenueProvider` — bootstrap, stream, execute, reconcile
- `TradeCommand` — today only `Liquidate` / `Swap` (Solana-era); **must add** `PlaceLimitOrder`, `CancelOrder`, etc.
- `DexWalletBalances`, `PendingTransaction`, `ExecutionResult`, `TxStatus`
- `DexVenueUpdate` — balance / pending / execution events
- `DexStrategy` + `DexStrategyEngine` (`trading_engine::dex`) — evaluates MD → `Option<TradeCommand>`

**Not defined yet (needed for Layer 4):**

- `DexQuoteConfig` / `DexQuote` — side, price, quantity, pool reference, lifecycle (mirror CEX *ideas*, not types)
- `DexQuoteManager` — active quotes, link to on-chain `order_id` after confirm
- `TradeCommand` CLOB variants (chain-agnostic intents)
- gRPC surface for DEX quote CRUD (optional; defer past Layer 3)

**Chain-specific vs portable:**

- **Portable:** `TradeCommand::PlaceLimitOrder { venue_pool_id, side, price, quantity, ... }`, `DexQuoteConfig`, `DexStrategy`, reconcile lifecycle (pending tx → confirmed → update quote state).
- **Sui/DeepBook only:** `dex_engine/chains/sui/` — indexer client, `BalanceManager` ID, tick/lot conversion, DeepBook SDK PTBs, package/pool object IDs from [contract information](https://docs.sui.io/onchain-finance/deepbookv3/contract-information).
- **Future chain:** new `dex_engine/chains/<chain>/` implementing the same `VenueProvider`; `trading_engine::dex::levels` stays unchanged if `TradeCommand` stays abstract.

**What we *can* reuse without coupling:**

- Risk *patterns* (daily loss cap, max position) — shared helpers or parallel structs, not `LevelRiskConfig` inheritance
- Indicator-driven pricing *ideas* — later, via `DexStrategy` reading `MarketDataUpdate`
- UI panels (orderbook, orders, balances) — already venue-agnostic proto fields
- Runner event-loop / reconcile patterns from CEX

**What we must not reuse:**

- `LevelManager`, `LevelExecutor`, `LevelPersistence` (Mongo bot levels)
- gRPC `CreateLevel` / `ToggleLevel` for DeepBook without a separate code path (those call `LevelManager` today)
- `OrderRequest` / `TradingClient` for on-chain orders

### Layer 5 — Dual venue (`both`)

- Run CEX + DEX in one process; health checks for each plane
- **No cross-venue hedging** in v1; UI shows CEX + `MonitoringData.dex` side by side
- Defer unified orderbook tabs to UI polish

**Explicitly out of scope for v1:** DeepBook margin/flash loans, DEEP staking/governance, self-hosted indexer (unless public SLA fails), mainnet until testnet stable.

### Workstream ownership (parallel-safe)

| Stream | Owner focus | Depends on |
|--------|-------------|------------|
| A — Types/config | `dex_engine`, runner config | — |
| B — Indexer MD | `market_data_engine`, gRPC | A (pool meta) |
| C — DexState read | `dex_engine` provider | A, B |
| D — Execute | `dex_engine` SDK | A, C |
| E — DEX levels | `trading_engine::dex` | D |
| F — Both mode | runner health | B, C |

Streams **B** and **A** can start in parallel once pool name list is fixed.

## Alternatives

1. **Vertical slice only** — ship SUI_USDC place/cancel in one large PR without read-only phase. *Rejected:* harder to debug PTB vs indexer vs UI; no demo value until keys funded.
2. **Chain-first (RPC event subscriptions, no indexer)** — read book from on-chain `Pool` objects. *Rejected for v1:* higher complexity; Mysten indexer is the [documented integration path](https://docs.sui.io/onchain-finance/deepbookv3/deepbook); add as fallback later if indexer lag is unacceptable.
3. **Fold DEX MD into `dex_engine` only** — skip `market_data_engine`. *Rejected:* violates 002 split; CEX MD patterns and DEX polling should stay parallel in `market_data_engine`.

## Consequences

- **Indexer lag** is acceptable for v1 monitoring (~1s); execution reconcile must use tx digest + indexer, not book alone.
- **Package upgrades** ([contract versions](https://docs.sui.io/onchain-finance/deepbookv3/contract-information)): pin `deepbook_package_id` in config; only package ID changes on upgrade — pool IDs stable unless redeploy.
- **CEX levels unchanged**; DEX levels are a separate module and config file — avoids polluting `LevelManager` Mongo schema.
- **New dependencies:** `sui-sdk`, DeepBook SDK (git dep) — semver and MSRV review required before Layer 3.
- **Secrets:** `SUI_PRIVATE_KEY` only required from Layer 2 onward (deposit) / Layer 3 (trade).

## Verification

| Layer | Tests / checks |
|-------|----------------|
| 0 | Config deserialize; tick/lot round-trip unit tests per pool fixture |
| 1 | HTTP mock or testnet: `/orderbook/SUI_USDC` parses; gRPC stream shows non-empty book |
| 2 | Testnet wallet: balances match Sui explorer; reconcile idempotent |
| 3 | Testnet: place → `Placed` in indexer → cancel → `Canceled`; pending tx lifecycle in `DexState` |
| 4 | DEX level repost after simulated fill (indexer trade poll) |
| 5 | `venue.mode: both` + Bybit CEX: CEX tests still pass; `/health` 200 when both ready |
| Regression | `cargo test -p dex_engine -p trading_core`; k-paper/Bybit levels unchanged with `venue.mode: cex` |

[cursor-hub workflow] step=design-review

## Implementation results

**Shipped (Layers 0 + 1):**

- `VenueId::Sui`, `DexChain::Sui`; `TradeCommand` CLOB variants (`PlaceLimitOrder`, `CancelOrder`, `CancelAllOrders`, `Deposit`, `Withdraw`)
- `dex_engine::chains::sui`: `SuiDeepBookConfig`, `DeepBookPoolMeta`, tick/lot `precision` module (unit tests), `DeepBookIndexerClient`, `SuiDeepBookVenueProvider` stub (loads pool meta from indexer at bootstrap; execution deferred to Layer 3)
- `select_provider()` branches on `DexChain::Sui` vs mock Solana
- `market_data_engine::exchanges::sui`: `DeepBookMarketDataClient` polls indexer ~1 Hz → `StructuredOrderbook` / `StructuredPublicTrade`
- Runner: `initialize_dex_market_data_engine` for `venue.mode: dex` + `chain: sui`; config validation for Sui JSON
- Updated `config/bot.dex.example.json`, `config/bot.both.example.json`, `docs/VENUE_MODES.md` (testnet pool `SUI_DBUSDC`)

**Deviations:**

- Testnet pool name is `SUI_DBUSDC` (not mainnet `SUI_USDC`) per live indexer
- `venue.mode: both` still uses CEX MD driver only; DeepBook MD polling is wired for `dex`-only until Layer 5
- No new external deps (`sui-sdk` / DeepBook SDK deferred to Layer 3)

**Verification:**

- `cargo test -p dex_engine -p market_data_engine -p runner --lib` — pass
- Config smoke: `bot_dex_example_validates`, `bot_both_example_validates` — pass
- Manual: `curl` testnet indexer `/orderbook/SUI_DBUSDC` returns live book

[cursor-hub workflow] step=implement

**Shipped (Layer 2 — wallet state read path):**

- `dex_engine::chains::sui::grpc`: `SuiGrpcClient` via full-node gRPC `StateService.ListBalances` ([Sui gRPC protocol](https://docs.sui.io/references/fullnode-protocol)); replaces deprecated JSON-RPC `suix_getAllBalances`
- Minimal vendored proto (`proto/sui_state_service.proto`) + `build.rs`; config `rpcUrl` / `grpcUrl` → `DeepBookConfig::grpc_url()` (defaults: `https://fullnode.<network>.sui.io`)
- `DeepBookVenueProvider`: `bootstrap_state` / `reconcile` / `stream_updates` poll wallet balances when `walletAddress` is set
- `DexStateManager::set_wallet_balances` + subscriber in `dex_engine::run` so gRPC monitoring sees live balance updates without reconciliation enabled
- `runner::dex_monitoring`: stub CEX state + gRPC server on `:50051` in `--mode dex` (Web UI sees orderbooks + `MonitoringData.dex`)
- `config/bot.dex.example.json`: `balancePollIntervalMs`

**Deviations (Layer 2):**

- Decimals still heuristic (9 for SUI, 6 otherwise); `GetCoinInfo` metadata fetch deferred
- Indexer open orders not merged into DexState yet (balances only; orders in Layer 3)
- `tonic` + `prost` added to `dex_engine` (no full `sui-sdk` until Layer 3 execution)

**Verification (Layer 2):**

- `cargo clippy -p dex_engine -p runner --lib -- -D warnings` — pass
- `cargo test -p dex_engine -p runner --lib` — pass (12 + 8 tests)

**Layer 2 completion (account snapshot):**

- `DexOpenOrder`, `DexAccountSnapshot`; `VenueProvider::bootstrap_state` / `reconcile` return full snapshot (wallet + open orders)
- `DexVenueUpdate::OpenOrders`; `DexState.open_orders`; reconcile + stream apply both wallet and orders
- Indexer `GET /orders/:pool/:balance_manager_id?status=Placed` → `DeepBookIndexerClient::get_open_orders`
- gRPC `StateService.GetCoinInfo` for coin decimals (cached); fallback heuristic retained
- `MonitoringData.dex.open_orders` proto field + gateway snapshot wiring
- Config: `balanceManagerId` accessor; orders require it, balances require `walletAddress`

**Still deferred to Layer 3:**

- `BalanceManager` create/load (SDK); `TradeCommand::Deposit` execution
- Live testnet integration test (balances vs explorer)

**Verification (Layer 2 finish):**

- `cargo clippy -p dex_engine -p runner -p gateway -p trading_core -p trading_networking --lib -- -D warnings` — pass
- `cargo test -p dex_engine -p runner -p trading_core -p trading_networking --lib` — pass

**Shipped (Layer 3 — write plane / execution):**

- `dex_engine::chains::sui::execution`: `SuiSigner` (env / hex / `suiprivkey`), `SuiJsonRpcClient` (gas, coins, submit, tx status), `DeepBookPtbContext` PTB builders (`place_limit_order`, `cancel_order`, `cancel_all_orders`, `deposit`, `withdraw`), `DeepBookExecutor`
- Deps: `sui-sdk-types`, `sui-transaction-builder`, `sui-crypto` (git, no `sui-rpc` — tower conflict avoided)
- `DeepBookVenueProvider::execute` → sign + submit; digest tracked in provider `pending` map; `stream_updates` polls JSON-RPC tx status → `DexVenueUpdate::ExecutionResult`
- `DexEngineRuntime::execute` records `PendingTransaction` in `DexState`; confirmed/failed clears pending
- Config: network-default `deepbookPackageId` + `registryId` (testnet/mainnet from Mysten TS SDK constants); `privateKeyEnv` → `SUI_PRIVATE_KEY`

**Deviations (Layer 3):**

- ~~**Write path uses JSON-RPC**~~ — **superseded:** writes now use gRPC (`TransactionExecutionService`, `LedgerService`, `StateService.ListOwnedObjects`, `GetEpoch`)
- Price/qty on-chain uses TS SDK `FLOAT_SCALAR` formula (not tick/lot from Layer 0 `precision` module) for PTB args
- `BalanceManager` create/register not automated — user supplies `balanceManagerId` after manual/SDK setup
- `payFeesInDeep: false` honored per config; testnet may require DEEP balance if on-chain rejects input-token fees

**Verification (Layer 3):**

- `cargo clippy -p dex_engine -p runner -p trading_core --lib -- -D warnings` — pass
- `cargo test -p dex_engine -p runner -p trading_core --lib` — pass (17 + 8 + 107 tests)
- PTB unit tests: `price_matches_ts_sdk_formula`, `quantity_in_base_units`

**Shipped (BalanceManager setup utility — wire-up when funded):**

- `dex_engine::execution::balance_manager`: PTB `new` → `register_balance_manager` → `public_share_object`
- `DeepBookSetup::create_and_register_balance_manager()` + gRPC `changed_objects` parsing
- CLI: `cargo run -p dex_engine --bin create_balance_manager -- --config config/bot.dex.example.json`
- Wrapper: `scripts/deepbook/create-balance-manager.sh` (see `scripts/deepbook/README.md`)
- Example config uses placeholder `walletAddress` / `balanceManagerId` until user runs utility

**Still deferred (until testnet funds / Layer 4):**

- Live testnet integration test (create manager → deposit → place → cancel)
- Layer 4 DEX levels (`trading_engine::dex::levels`)
- Deposit helper script (use DeepBook SDK or bot `TradeCommand::Deposit` when ready)

**Verification (setup utility):**

- `cargo clippy -p dex_engine --all-targets -- -D warnings` — pass
- `cargo test -p dex_engine --lib` — pass (18 tests)

**gRPC write-path migration (2026-06):**

- Uses Mysten's [`sui-rpc`](https://github.com/MystenLabs/sui-rust-sdk) crate for generated protos and gRPC clients (no vendored `.proto` tree, no `build.rs`, no manual `google.rpc.Status` shim)
- `SuiGrpcClient` wraps `sui_rpc::Client` with `FieldMaskUtil`, accessor APIs, and `get_reference_gas_price()`
- Removed JSON-RPC write path; `DeepBookSetup` + `DeepBookExecutor` hold a cloneable `SuiGrpcClient` (no `Mutex` wrapper)
- Workspace `tower` bumped to `0.5.3` for `sui-rpc` compatibility

**Verification (gRPC migration):**

- `cargo test -p dex_engine --lib` — pass (22 tests)
- `cargo clippy -p dex_engine --lib -- -D warnings` — pass

[cursor-hub workflow] step=implement

---

**Remaining work:** see [007-deepbook-trading-readiness](007-deepbook-trading-readiness.md) for the P0–P3 checklist (config examples, deposit helper, live smoke, Mongo/shutdown hardening, deferred UI/strategies).
