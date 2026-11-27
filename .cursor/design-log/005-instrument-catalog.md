# Instrument catalog / standardized data layer

## Problem / context

The codebase already has **canonical Rust domain types** in `trading_core` (`Pair`, `Instrument`, `Order`, `Balance`, CEX/DEX venue modules) and **venue adapters** that map wire formats into those types. Symbol identity today is split: `SymbolRegistry` (in-memory, bootstrapped from bot `pairs`), `asset_engine` (TTL cache of exchange `SymbolInfo` / `TradingRules` from REST), and per-venue string conventions (`BTCUSDT` vs `BTC-USDT`, Bybit categories, DEX mint/pool IDs).

The question is whether to add a persistent **instrument DB** that holds “all key types” and becomes the source of truth for symbols across venues.

## Decision

**Do not** build a single DB that stores orders, trades, balances, and other runtime trading objects as the standardized layer. Those belong in **in-memory domain types + venue state**, with optional **separate** persistence for audit/replay if needed later.

**Do** pursue a **narrow instrument catalog** (reference data only) if cross-venue standardization becomes painful—but **phase 1** should strengthen what exists: canonical types in `trading_core`, venue adapters, `SymbolRegistry`, and `asset_engine`. A DB-backed catalog is **deferred** until there is a concrete ops or multi-bot requirement (add instruments without redeploy, stable internal IDs across venue renames, shared catalog across processes).

When introduced, the catalog should:

- Define a stable internal key (e.g. `InstrumentKey` or `(venue_id, market_kind, canonical_pair)`).
- Store **venue aliases** (`exchange_symbol`, `instrument_id`, category/product type), tick/lot/min-notional metadata (or pointers to asset_engine refresh).
- Load once at startup (or on change) into an in-memory map used on the hot path—**not** per-message DB reads.
- Keep **Rust structs** as the execution/API contract; the DB is configuration/reference, not a replacement for `Order`/`Trade` types.

## Alternatives

| Option | Pros | Cons |
|--------|------|------|
| **A. Status quo + harden registry** (extend `SymbolRegistry`, unify k_paper/Binance/Bybit mapping in one module; keep `asset_engine` for rules) | Zero new infra; matches current bot-config-driven deployment; hot path stays in RAM | No central ops UI; each process builds registry from config; DEX identity still separate |
| **B. Instrument catalog DB** (Mongo/Postgres) + snapshot into memory at bootstrap | Ops can edit instruments; stable IDs for analytics; one place for venue aliases | Schema drift vs Rust types; migration cost; still need adapters; risk of dual source of truth |
| **C. “DB is the types layer”** (persist orders/trades/balances schemas in DB, codegen or dynamic rows) | Theoretical single source of truth | Wrong boundary—conflates reference data with transactional state; hurts latency and type safety; large refactor |

**Rejected now:** C. **Deferred:** B until A shows gaps (unknown pairs, conflicting aliases, multi-tenant catalog).

## Consequences

- Orders/trades/balances remain `trading_core::venue::cex::types::exchange` (and DEX equivalents)—not rows in an instrument DB.
- `SymbolRegistry::get_or_parse_pair` heuristic fallback is a **tech debt** signal; catalog or explicit per-venue maps should replace “guess quote suffix” for production symbols.
- DEX (see log 002) needs **chain-specific instrument identity** (mint, pool); a Pair-only DB is insufficient for `venue.mode: both` without a polymorphic `InstrumentRef`.
- Adding Mongo collections for instruments overlaps with existing config persistence—avoid a second config store unless ops workflow requires it.

## Verification

- **Invariants:** Every inbound venue message maps to at most one canonical `Instrument` / `Pair` for configured symbols; unconfigured symbols fail closed or log loudly (no silent `UNKNOWN/UNKNOWN` in production paths).
- **Tests:** Registry/catalog round-trip per venue (Binance, Bybit, k_paper); asset_engine rules match catalog tick/lot when both exist.
- **Metrics:** Count of `get_or_parse_pair` fallbacks; symbol mapping failures at adapter boundary.
- **Acceptance for future catalog:** Bootstrap loads catalog → in-memory registry; change catalog entry → restart or explicit reload → adapters see new alias without code change.

[cursor-hub workflow] step=design-review
