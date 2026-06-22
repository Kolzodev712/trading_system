# Proposal: Sui DeepBookV3 Integration — On-Chain CLOB Trading

**Version**: 2.0  
**Date**: June 2026  
**Status**: Draft  
**Supersedes**: [01-solana-integration.md](./01-solana-integration.md) (lending-liquidation-first approach deferred)  
**Prerequisites**: Venue split + platform readiness **shipped** — design logs 002 (DEX preparation), 003 (platform readiness). DEX plumbing exists: `VenueProvider`, `dex_engine`, `trading_engine::dex`, `venue.mode` (`cex` | `dex` | `both`), gRPC `MonitoringData.dex`.

**Official references**

- [DeepBookV3 overview](https://docs.sui.io/onchain-finance/deepbookv3/deepbook)
- [DeepBookV3 design](https://docs.sui.io/onchain-finance/deepbookv3/design) — Pool, BalanceManager, Book / State / Vault
- [Contract information](https://docs.sui.io/onchain-finance/deepbookv3/contract-information) — package IDs, pool IDs, coin types
- [DeepBookV3 Indexer](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer) — orderbook, orders, trades HTTP API

---

## Executive Summary

Implement **Sui + DeepBookV3** as the first real DEX venue under `dex_engine/chains/sui/`. DeepBookV3 is a **central limit order book (CLOB)** on Sui — structurally closer to our CEX levels/orderbook stack than the prior Solana lending-liquidation plan.

### Key objectives

1. **`SuiDeepBookVenueProvider`** — `VenueProvider` implementation for Sui + DeepBookV3
2. **Sui connectivity** — JSON-RPC full node + transaction signing (PTBs)
3. **DeepBook SDK** — place/cancel limit orders, `BalanceManager` funding (official SDK abstracts PTB complexity per [DeepBook docs](https://docs.sui.io/onchain-finance/deepbookv3/deepbook))
4. **DeepBook Indexer client** — orderbook, open orders, trades for market data and UI
5. **Extend `TradeCommand`** — CLOB order ops (not only `Liquidate` / `Swap`)
6. **DEX levels strategy** — port levels/MM concepts to DeepBook pools (phase 3+)
7. **Preserve CEX stack** — `exchange_engine` / Bybit levels unchanged; DEX via `dex_engine` only

### Why DeepBook first (not Solana liquidations)

| | Solana (v1 draft) | Sui + DeepBookV3 |
|--|-------------------|------------------|
| Primary model | Lending protocol liquidations | **CLOB** — bids, asks, limit orders |
| Fit with levels / orderbook UI | Weak | **Strong** — same mental model as CEX |
| Rust client | `solana-sdk` | **`sui-sdk`** + DeepBook SDK |
| First demo value | Niche MEV | Balances, book, place/cancel visible in Web UI |
| Existing code waste | — | **None** — only `MockVenueProvider` shipped |

### Phased delivery

| Phase | Scope | Outcome |
|-------|--------|---------|
| **0** | Config + `VenueId::Sui`, chain enum | Runner starts in `venue.mode: dex` with Sui config |
| **1** | Indexer polling → market data + gRPC `dex` / orderbooks | Read-only monitor (testnet `SUI_USDC`) |
| **2** | `BalanceManager` + Sui RPC reads → `DexWalletBalances` | Balances in UI DEX panel |
| **3** | SDK: place/cancel limit orders, tx tracking | Manual/strategy execution |
| **4** | DEX levels engine on DeepBook pools | Parity with CEX levels (repost, risk) |
| **5** | `venue.mode: both` + unified monitoring | CEX + DEX in one process |

---

## 1. DeepBookV3 protocol model (external)

From [Design](https://docs.sui.io/onchain-finance/deepbookv3/design):

### Shared objects

| Object | Role |
|--------|------|
| **`Pool`** | One market; owns order book, pool state, vault |
| **`PoolRegistry`** | Pool creation / versioning guard |
| **`BalanceManager`** | User funds for orders; **one `BalanceManager` can serve all pools** |

### Pool internals (processing order)

```
place_order_int:
  1. Book   — validate, match against makers, inject remainder as limit order
  2. State  — fees, volumes, account settled/owed balances
  3. Vault  — move coins between Vault and BalanceManager (base, quote, DEEP)
```

### Order lifecycle (limit)

1. Client builds PTB calling pool `place_order` (via DeepBook SDK)
2. `OrderInfo` created → matched against `Book` → fills appended
3. Remaining quantity → compact `Order` in book
4. `State.process_create` updates fees, maker/taker volumes, account orders
5. `Vault.settle_balance_manager` transfers base / quote / DEEP vs `BalanceManager`

### Fee tokenomics

- Fees in **DEEP** (discounted) or input token
- Staked DEEP → taker/maker incentives ([overview](https://docs.sui.io/onchain-finance/deepbookv3/deepbook))
- Governance sets per-pool taker/maker bps (volatile vs stable pools)

### Mainnet contracts (v6 — verify before prod)

| Parameter | Value |
|-----------|--------|
| Package ID | `0x337f4f4f6567fcd778d5454f27c16c70e2f274cc6377ea6249ddf491482ef497` |
| Registry ID | `0xaf16199a2dff736e9f07a845f23c5da6df6f756eddb631aed9d24a93efc4549d` |

Example pool (**SUI/USDC** mainnet):

| Field | Value |
|-------|--------|
| Pool ID | `0xe05dafb5133bcffb8d59f4e12465dc0e9faeaa05e3e342a08fe135800e3e4407` |
| Tick size | 0.00001 |
| Lot size | 0.1 SUI |
| Min size | 1 SUI |

Coin types (examples):

- SUI: `0x2::sui::SUI` (9 decimals)
- Native USDC: `0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC` (6 decimals)
- DEEP: `0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270::deep::DEEP` (6 decimals)

Full pool/coin tables: [Contract information](https://docs.sui.io/onchain-finance/deepbookv3/contract-information).

---

## 2. Architecture mapping

### 2.1 Current (CEX only — e.g. Bybit)

```
MarketDataEngine → WS orderbooks/trades → TradingEngine (levels/strategies)
ExchangeEngine   → REST/WS → balances, orders, positions
Gateway gRPC     → MonitoringData stream → TUI / Web UI
```

### 2.2 Target (DEX — Sui DeepBook)

```
DeepBook Indexer (HTTP)  → orderbook, orders, trades
        ↓
MarketDataEngine (sui/deepbook driver) → normalized book + trades
        ↓
TradingEngine::dex (DEX levels / MM strategy) → TradeCommand
        ↓
dex_engine::chains::sui::SuiDeepBookVenueProvider
        ↓
Sui RPC + DeepBook SDK → PTB sign & execute → Pool / BalanceManager
        ↓
DexStateManager → reconcile → gRPC MonitoringData.dex
```

### 2.3 Target (`venue.mode: both`)

```
CEX path (unchanged)          DEX path (new)
Bybit / Paper        ||       DeepBook testnet/mainnet
       ↓             ||              ↓
MonitoringData       ||       MonitoringData.dex + shared orderbooks*
```

\*Optional phase 5: map DeepBook pool books into `MonitoringData.orderbooks` for unified UI tabs, or keep DEX-specific panels only.

### 2.4 Principles (unchanged)

- Trading logic never calls Sui RPC directly — only via `VenueProvider::execute`
- Internal `DexState` reconciled against indexer + on-chain reads
- CEX crates untouched except shared monitoring hooks (already present)
- Same gRPC / Web UI contract (`TradingCliService`); DEX data in `MonitoringData.dex`

---

## 3. Connectivity layer

### 3.1 Sui full node (execution + authoritative reads)

**Crate:** `sui-sdk` (Rust)

**Uses:**

| Operation | Sui API | Purpose |
|-----------|---------|---------|
| Read coins | `suix_getBalance`, `suix_getAllBalances` | Wallet / `BalanceManager` reconciliation |
| Object read | `sui_getObject` | Pool, `BalanceManager`, order state |
| Submit tx | `sui_executeTransactionBlock` | Place/cancel orders |
| Simulate | `sui_dryRunTransactionBlock` | Pre-flight |
| Subscribe (optional) | `sui_subscribeEvent` / websocket | Faster fill detection |

**Config:**

```rust
pub struct SuiRpcConfig {
    pub rpc_url: String,           // e.g. https://fullnode.testnet.sui.io:443
    pub ws_url: Option<String>,
    pub network: SuiNetwork,       // mainnet | testnet | devnet
    pub request_timeout_ms: u64,
    pub max_retries: u32,
}
```

**Location:** `src/dex_engine/src/chains/sui/rpc.rs`

### 3.2 DeepBook Indexer (market data — read path)

Public endpoints ([Indexer docs](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer)):

| Network | Base URL |
|---------|----------|
| Mainnet | `https://deepbook-indexer.mainnet.mystenlabs.com/` |
| Testnet | `https://deepbook-indexer.testnet.mystenlabs.com/` |

**Primary endpoints for this system:**

| Endpoint | Use in our stack |
|----------|------------------|
| `GET /get_pools` | Discover pools, `tick_size`, `lot_size`, `min_size`, asset decimals |
| `GET /orderbook/:pool_name?level=2&depth=N` | **Orderbook panel** — bids/asks `[[price, qty], ...]` |
| `GET /orders/:pool_name/:balance_manager_id?status=Placed` | **Open orders** — map to internal order list |
| `GET /trades/:pool_name?limit=N` | **Recent trades** panel |
| `GET /order_updates/:pool_name?...` | Incremental order events (optional stream substitute) |
| `GET /assets` | Coin metadata / deposit flags |

**Indexer pool response (normalized):**

```json
{
  "pool_id": "0xe05dafb5133bcffb8d59f4e12465dc0e9faeaa05e3e342a08fe135800e3e4407",
  "pool_name": "SUI_USDC",
  "base_asset_symbol": "SUI",
  "base_asset_decimals": 9,
  "quote_asset_symbol": "USDC",
  "quote_asset_decimals": 6,
  "min_size": 100000000,
  "lot_size": 10000000,
  "tick_size": 10000
}
```

**Orderbook response:**

```json
{
  "timestamp": "1733874965431",
  "bids": [["3.715", "2.7"], ["3.713", "2294.8"]],
  "asks": [["3.717", "0.9"], ["3.718", "1000"]]
}
```

**Open order response:**

```json
{
  "order_id": "170141183464610341308794360958165054983",
  "balance_manager_id": "0xd335e8aa...",
  "type": "sell",
  "current_status": "Placed",
  "price": 0.045,
  "original_quantity": 1000.0,
  "filled_quantity": 0.0,
  "remaining_quantity": 1000.0
}
```

**Volume scalars:** indexer volumes are in **smallest units**; divide by `10^decimals` per [asset conversion table](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer).

**Location:** `src/market_data_engine/src/exchanges/sui/deepbook_indexer.rs`

**Polling cadence (phase 1):** 1 Hz orderbook (align with gRPC stream); 5 s open orders / balances — mirror CEX TUI levels refresh.

### 3.3 DeepBook SDK (execution — write path)

DeepBookV3 does not ship a user-facing exchange UI; integrations use the **DeepBook SDK** to build PTBs ([overview](https://docs.sui.io/onchain-finance/deepbookv3/deepbook)).

**Responsibilities:**

- Create / reuse `BalanceManager`
- Deposit base/quote into `BalanceManager`
- `place_limit_order` / `place_market_order` / `cancel_order`
- Handle DEEP fee payment mode

**Location:** `src/dex_engine/src/chains/sui/deepbook_sdk.rs` (thin wrapper over official SDK)

**Wallet:** Ed25519 keypair from env or encrypted file — `SUI_PRIVATE_KEY` or `sui.keystore` path in config.

---

## 4. Internal type mapping

### 4.1 DeepBook → `MonitoringData` (gRPC / UI)

| DeepBook / Indexer | Internal (existing proto) | Notes |
|------------------|---------------------------|--------|
| `pool_name` `SUI_USDC` | `Orderbook.pair` = `"SUI/USDC"` | Display format |
| Indexer bids/asks | `OrderbookLevel.price`, `.quantity` | Strings |
| Best bid/ask | `Orderbook.best_bid`, `.best_ask` | From level=1 or first row |
| `order_id` | `Order.order_id` | |
| `type` buy/sell | `Order.side` | Normalize to `Buy`/`Sell` |
| `price`, `remaining_quantity` | `Order.price`, `.quantity` | |
| `current_status` | `Order.status` | `Placed`, `Canceled`, `Filled` |
| BalanceManager coin balances | `DexBalance.mint`, `.amount` | `MonitoringData.dex` |
| Tx digest | `DexPendingTransaction.signature` | Pending until finalized |

### 4.2 DeepBook → `DexState` (`dex_engine`)

```rust
pub struct DeepBookAccountState {
    pub balance_manager_id: ObjectID,
    pub pool_orders: HashMap<ObjectID /* pool_id */, Vec<DeepBookOpenOrder>>,
}

pub struct DeepBookOpenOrder {
    pub order_id: String,
    pub side: Side,
    pub price: Decimal,      // human units after tick conversion
    pub quantity: Decimal,   // human units after lot conversion
    pub status: OrderStatus,
}
```

### 4.3 Pool precision helpers

```rust
/// Convert human price to on-chain tick units
fn price_to_ticks(price: Decimal, tick_size: u64) -> u64;

/// Convert human quantity to lot units  
fn qty_to_lots(qty: Decimal, lot_size: u64) -> u64;

/// Validate min_size before submit
fn validate_min_size(lots: u64, pool: &DeepBookPoolMeta) -> Result<(), DexError>;
```

Pool metadata loaded from `GET /get_pools` and cached at startup.

### 4.4 Extend `TradeCommand` (`trading_core::venue::dex`)

Current enum only has `Liquidate` and `Swap` (Solana-era). **Add CLOB commands:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TradeCommand {
    // existing
    Liquidate { ... },
    Swap { ... },

    // DeepBook CLOB (new)
    PlaceLimitOrder {
        pool_id: SmolStr,
        balance_manager_id: SmolStr,
        side: Side,
        price: Decimal,           // human price; engine converts to ticks
        quantity: Decimal,        // human qty; engine converts to lots
        pay_fees_in_deep: bool,
    },
    CancelOrder {
        pool_id: SmolStr,
        balance_manager_id: SmolStr,
        order_id: SmolStr,
    },
    CancelAllOrders {
        pool_id: SmolStr,
        balance_manager_id: SmolStr,
    },
    Deposit {
        balance_manager_id: SmolStr,
        coin_type: SmolStr,
        amount: Decimal,
    },
    Withdraw {
        balance_manager_id: SmolStr,
        coin_type: SmolStr,
        amount: Decimal,
    },
}
```

`VenueProvider::execute` dispatches these to DeepBook SDK PTBs.

### 4.5 Extend `VenueId` / `DexChain`

```rust
pub enum VenueId {
    Binance,
    Bybit,
    Paper,
    Solana,  // reserved / deprecated
    Sui,     // new
}

pub enum DexChain {
    Solana,  // deprecated default
    Sui,     // new default for DeepBook work
}
```

---

## 5. Crate layout

```
src/dex_engine/src/chains/
├── mod.rs
├── mock.rs                    # keep until Sui provider passes integration tests
└── sui/
    ├── mod.rs
    ├── config.rs              # SuiDeepBookConfig (RPC, indexer, pools, keys)
    ├── rpc.rs                 # sui-sdk client
    ├── deepbook_indexer.rs    # HTTP indexer client
    ├── deepbook_sdk.rs        # PTB builders (place/cancel/deposit)
    ├── provider.rs            # impl VenueProvider for SuiDeepBookVenueProvider
    ├── types.rs               # DeepBookPoolMeta, order mappings
    └── reconcile.rs           # DexState vs indexer + chain

src/market_data_engine/src/exchanges/sui/
├── mod.rs
├── deepbook_indexer.rs        # shared client (or re-export from dex_engine)
└── driver.rs                  # poll indexer → MarketDataUpdate

src/trading_engine/src/dex/
├── engine.rs
├── strategy.rs
├── levels.rs                  # NEW: DeepBookLevelsStrategy (phase 4)
└── liquidation.rs             # unchanged; not in scope for v2.0
```

---

## 6. `SuiDeepBookVenueProvider`

```rust
pub struct SuiDeepBookVenueProvider {
    rpc: Arc<SuiRpcClient>,
    indexer: Arc<DeepBookIndexerClient>,
    sdk: Arc<DeepBookSdk>,
    config: SuiDeepBookConfig,
    signer: SuiKeyPair,
    pools: HashMap<String, DeepBookPoolMeta>, // keyed by pool_name
}
```

### `VenueProvider` behavior

| Method | Behavior |
|--------|----------|
| `venue_id()` | `VenueId::Sui` |
| `bootstrap_state()` | Load/create `BalanceManager`; read balances; fetch open orders from indexer |
| `stream_updates()` | Poll indexer + optional Sui event WS; emit `DexVenueUpdate::Balance`, `::Order`, `::Execution` |
| `execute()` | Map `TradeCommand` → SDK PTB → sign → submit → track digest in `pending` |
| `reconcile()` | Compare `DexState` vs indexer orders + on-chain balances; clear stale pending |

### Transaction lifecycle

```
execute(PlaceLimitOrder)
  → build PTB (SDK)
  → dry_run (optional)
  → sign + executeTransactionBlock
  → PendingTransaction { signature: digest, status: Pending }
  → poll until Confirmed / Failed
  → DexVenueUpdate::ExecutionResult
  → reconcile open orders
```

---

## 7. Configuration

### 7.1 `bot.json` — DEX only (testnet)

```json
{
  "venue": { "mode": "dex" },
  "system": { "log_level": "info", "shutdown_timeout_secs": 30 },
  "dex_engine": {
    "venue": "sui",
    "chain": "sui",
    "config": {
      "network": "testnet",
      "rpc_url": "https://fullnode.testnet.sui.io:443",
      "indexer_url": "https://deepbook-indexer.testnet.mystenlabs.com",
      "deepbook_package_id": "<testnet package — verify at deploy time>",
      "balance_manager_id": null,
      "pools": ["SUI_USDC"],
      "pay_fees_in_deep": false,
      "private_key_env": "SUI_PRIVATE_KEY"
    }
  },
  "server": { "bind_address": "0.0.0.0", "port": 8080 },
  "mongo": {
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system",
    "bot_id": "deepbook_bot",
    "enabled": true
  }
}
```

### 7.2 `bot.json` — both (CEX + DEX)

```json
{
  "venue": { "mode": "both" },
  "exchange_engine": {
    "exchange": "Bybit",
    "config": { "...": "unchanged CEX config" }
  },
  "dex_engine": {
    "venue": "sui",
    "chain": "sui",
    "config": { "...": "as above" }
  }
}
```

### 7.3 Secrets

| Secret | Source | Notes |
|--------|--------|-------|
| `SUI_PRIVATE_KEY` | env | Testnet wallet with SUI + USDC for `SUI_USDC` |
| Optional DEEP | wallet | Fee discounts; not required phase 1–3 |

Never commit private keys to `bot.json`.

---

## 8. UI / gRPC impact

**No proto changes required for phase 1–2** — use existing fields:

- `MonitoringData.dex` → balances + pending txs (already in Web UI `DexPanel`)
- `MonitoringData.orderbooks` → populate from indexer (optional unified view)
- `MonitoringData.orders` → can include DeepBook open orders in `dex` mode

**Phase 4+:** optional RPC extensions for DEX-specific commands (`PlaceDeepBookOrder`) if we do not route through existing level CRUD semantics.

---

## 9. Implementation phases (detailed)

### Phase 0 — Plumbing (1 week)

- [ ] Add `VenueId::Sui`, `DexChain::Sui`
- [ ] `SuiDeepBookConfig` deserialize from `dex_engine.config`
- [ ] Wire runner to select `chains::sui::provider` instead of `mock` when `venue: sui`
- [ ] Update `config/bot.dex.example.json`, `docs/VENUE_MODES.md`

### Phase 1 — Read-only monitor (1–2 weeks)

- [ ] `DeepBookIndexerClient` — `get_pools`, `orderbook`, `trades`
- [ ] `market_data_engine` driver — poll → internal orderbook state
- [ ] Feed gRPC `MonitoringData` (orderbooks + trades)
- [ ] Verify in Web UI Spot monitor + `DexPanel`
- [ ] Testnet pool: `SUI_USDC`

### Phase 2 — Balances & reconciliation (1 week)

- [ ] Create/load `BalanceManager` via SDK
- [ ] `bootstrap_state` + `reconcile` implemented
- [ ] `MonitoringData.dex.balances` populated
- [ ] Deposit helper command (manual funding)

### Phase 3 — Execution (2 weeks)

- [ ] Extend `TradeCommand` with CLOB variants
- [ ] `place_limit_order`, `cancel_order` via SDK
- [ ] Pending tx tracking + confirmation polling
- [ ] Integration test against testnet (place → verify in indexer → cancel)

### Phase 4 — DEX levels (2–3 weeks)

- [ ] `DeepBookLevelsStrategy` in `trading_engine::dex`
- [ ] Map `levels.json`-style config to pool + tick/lot constraints
- [ ] Repost-on-fill (cancel + replace on indexer fill event)
- [ ] Risk limits (max position in base coin)

### Phase 5 — Dual venue (1 week)

- [ ] `venue.mode: both` hardening
- [ ] Health checks: CEX + DEX readiness in `/health`
- [ ] Operational runbook

---

## 10. Risks & mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Indexer downtime / lag | Stale book, bad quotes | Self-host indexer for prod; fallback poll Sui events |
| PTB simulation failure | Rejected orders | `dry_run` before submit; validate tick/lot/min_size |
| Package upgrade | Broken pool IDs | Pin package ID in config; [version table](https://docs.sui.io/onchain-finance/deepbookv3/contract-information) |
| Fee token (DEEP) balance | Tx fail | Monitor DEEP balance; config `pay_fees_in_deep` |
| Key compromise | Fund loss | Hot wallet, limited balances, env-only secrets |
| Competition on CLOB | Adverse fills | Post-only limits where supported; realistic testnet first |
| `lot_size` / `tick_size` rounding | Order reject | Centralize conversion; unit tests per pool |

---

## 11. Success metrics

| Metric | Phase 1 target | Phase 3 target |
|--------|----------------|----------------|
| Orderbook refresh | ≤ 2 s staleness | ≤ 1 s |
| Balance reconciliation | — | 100% match indexer + chain |
| Place limit success rate | — | > 95% on testnet |
| Cancel latency | — | < 3 s to `Canceled` in indexer |
| UI parity | Book + trades visible | Orders + balances + pending txs |

---

## 12. Out of scope (v2.0)

- Solana lending liquidations (prior proposal — revisit later if needed)
- DeepBook **margin** / flash loans ([indexer margin endpoints](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer) exist but not targeted)
- Cross-venue hedging (CEX Bybit vs DEX DeepBook)
- Self-hosted Sui validator
- On-chain DEEP staking automation

---

## 13. Open questions

1. **First pool:** `SUI_USDC` testnet only, or mainnet small size?
2. **Indexer:** Public Mysten testnet indexer vs self-hosted from [DeepBook indexer repo](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer)?
3. **SDK packaging:** Vendor DeepBook SDK as git dep or copy generated Move bindings?
4. **Levels config:** Separate `config/dex_levels.json` or extend `levels.json` with `venue: sui`?
5. **UI:** Unified orderbook tabs (CEX + DEX) or separate DEX dashboard section?

---

## 14. References

- [DeepBookV3](https://docs.sui.io/onchain-finance/deepbookv3/deepbook)
- [Design — Pool, BalanceManager, Book/State/Vault](https://docs.sui.io/onchain-finance/deepbookv3/design)
- [Contract information](https://docs.sui.io/onchain-finance/deepbookv3/contract-information)
- [DeepBookV3 Indexer API](https://docs.sui.io/onchain-finance/deepbookv3/deepbookv3-indexer)
- [Sui JSON-RPC](https://docs.sui.io/sui-api-ref)
- Internal: `libs/trading_core/src/venue/dex/`, `src/dex_engine/`, `docs/VENUE_MODES.md`

---

## Appendix A: Example place-limit flow

1. **Indexer driver** polls `GET /orderbook/SUI_USDC?depth=20` → updates `MarketDataState`
2. **DEX levels strategy** decides bid at 3.710, qty 10 SUI
3. Emits `TradeCommand::PlaceLimitOrder { pool_id, side: Buy, price: 3.710, quantity: 10 }`
4. **`SuiDeepBookVenueProvider::execute`**
   - Load pool meta: `tick_size`, `lot_size`, `min_size`
   - Convert price/qty → ticks/lots
   - SDK builds PTB: `place_limit_order(pool, balance_manager, ...)`
   - Sign, submit, record digest as `Pending`
5. **Indexer** `GET /orders/SUI_USDC/:balance_manager_id?status=Placed` shows new order
6. **gRPC stream** updates `MonitoringData.orders` / `dex` panel
7. On fill: `order_updates` or trades endpoint → strategy may repost if configured

## Appendix B: Superseded Solana scope

The [Solana liquidation proposal](./01-solana-integration.md) remains valid for a **future** `dex_engine/chains/solana/` if MEV/liquidation is prioritized. It is **not** the first DEX milestone under this revision.
