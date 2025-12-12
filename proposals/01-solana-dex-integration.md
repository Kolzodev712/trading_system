# Proposal: Solana DEX Integration - Lending Protocol Liquidations

**Version**: 0.1  
**Date**: December 2024  
**Status**: Proposal  
**Priority**: High

## Executive Summary

This proposal outlines the integration of Solana DEX capabilities into the existing trading system, with a primary focus on **lending protocol liquidations** as the initial monetization strategy. The integration leverages the existing architecture's clean separation of concerns, requiring targeted extensions rather than a system rewrite.

### Key Objectives

1. **Extend ExchangeEngine** to support on-chain venues via a `VenueProvider` abstraction
2. **Extend MarketDataEngine** to ingest Solana on-chain state (pool accounts, oracle accounts, protocol accounts)
3. **Implement LiquidationStrategy** in TradingEngine for deterministic, automated liquidation execution
4. **Maintain architectural integrity** - no direct exchange calls from TradingEngine, full reconciliation, risk isolation

### Why Liquidations First?

- **Deterministic**: No price prediction, no indicators, no TA required
- **Mechanical**: Protocol maintenance with guaranteed bonuses (3-10%)
- **Architectural fit**: Perfect match for existing state management and reconciliation patterns
- **Low capital**: Can start with smaller positions and scale
- **Well-defined profit mechanism**: Liquidation bonuses are protocol-defined, profit calculation is straightforward
- **No market direction risk**: Protocol maintenance with guaranteed rewards, not directional trading

---

## 1. Architecture Mapping

### 1.1 Current Architecture (CEX - Binance)

```
MarketDataEngine → Orderbook/Trade Streams → TradingEngine
ExchangeEngine → REST/WS → Account/Balance State → TradingEngine
TradingEngine → TradeCommand → ExchangeEngine → Order Execution
```

### 1.2 Extended Architecture (DEX - Solana)

```
MarketDataEngine → Solana RPC/WS → Pool/Oracle/Protocol Accounts → TradingEngine
ExchangeEngine → SolanaVenueProvider → Wallet/On-chain State → TradingEngine
TradingEngine → TradeCommand::Liquidate → ExchangeEngine → Transaction Execution
```

### 1.3 Key Architectural Principles (Maintained)

✅ **TradingEngine never calls venue directly** - All execution via ExchangeEngine  
✅ **State reconciliation** - WS updates vs authoritative on-chain reads  
✅ **Risk isolation** - RiskEngine applies same limits to DEX strategies  
✅ **Channel-based communication** - Ultra-low latency maintained  
✅ **Strategy abstraction** - LiquidationStrategy fits existing Strategy trait

---

## 2. Technical Implementation

### 2.1 Connectivity Layer

#### 2.1.1 Solana RPC Client

**Purpose**: HTTP JSON-RPC for reads and transaction submission

**Implementation**:
- Use `solana-client` or `solana-sdk` Rust crates
- Support multiple RPC endpoints (primary + fallback)
- Implement retry logic with exponential backoff
- Handle rate limiting gracefully

**Key RPC Methods Required**:
- `getAccountInfo` - Read account data
- `getLatestBlockhash` - For transaction building
- `sendTransaction` - Submit signed transactions
- `getSignatureStatus` - Track transaction confirmation
- `simulateTransaction` - Pre-flight checks

**Location**: `src/exchange_engine/src/exchanges/solana/rpc_client.rs`

#### 2.1.2 WebSocket PubSub Client

**Purpose**: Real-time account/logs/signature subscriptions

**Implementation**:
- Maintain persistent WebSocket connection
- Implement reconnection logic with exponential backoff
- Subscription manager for multiple concurrent subscriptions
- Handle backpressure and subscription limits

**Key Subscriptions Required**:
- `accountSubscribe` - Watch account data changes
- `logsSubscribe` - Watch program logs/instruction execution
- `signatureSubscribe` - Track transaction confirmation

**Location**: `src/exchange_engine/src/exchanges/solana/ws_client.rs`  
**Location**: `src/market_data_engine/src/exchanges/solana/ws_client.rs`

**Configuration**:
```rust
pub struct SolanaRpcConfig {
    pub http_url: String,
    pub ws_url: String,
    pub commitment: CommitmentLevel, // processed, confirmed, finalized
    pub timeout_ms: u64,
    pub max_retries: u32,
}
```

### 2.2 Data Acquisition Strategy

#### 2.2.1 Account Discovery

**Purpose**: Identify which on-chain accounts to monitor

**For Liquidations**:
- Lending protocol market accounts (e.g., Marginfi, Solend)
- Oracle price accounts (Pyth, Switchboard)
- Obligation/position accounts (thousands potentially)

**Implementation**:
- Bootstrap at startup via RPC queries
- Cache account addresses in memory/DB
- Support dynamic account discovery (new positions)
- Filter by risk criteria (e.g., only monitor high-value positions)

**Location**: `src/market_data_engine/src/exchanges/solana/account_discovery.rs`

#### 2.2.2 State Decoding

**Purpose**: Convert binary account data into usable structs

**Implementation**:
- Protocol-specific decoders per lending protocol
- Version-aware decoding (program upgrades change layouts)
- Normalization layer to unified internal types

**Protocol Modules**:
```
src/market_data_engine/src/exchanges/solana/protocols/
├── marginfi/
│   ├── decoder.rs          # Account layout decoding
│   ├── types.rs            # Marginfi-specific types
│   └── liquidation.rs       # Liquidation instruction builder
├── solend/
│   ├── decoder.rs
│   ├── types.rs
│   └── liquidation.rs
└── common/
    ├── normalized.rs       # Unified types
    └── oracle.rs           # Oracle price normalization
```

**Normalized Types**:
```rust
pub struct LendingAccountHealth {
    pub protocol: LendingProtocol,
    pub account: Pubkey,
    pub health_factor: Decimal,
    pub liquidation_price: Decimal,
    pub collateral_value: Decimal,
    pub debt_value: Decimal,
    pub max_liquidation_amount: Decimal,
    pub bonus_percentage: Decimal,
}
```

**Location**: `src/market_data_engine/src/exchanges/solana/protocols/`

### 2.3 MarketDataEngine Extension

#### 2.3.1 Solana MarketDataDriver

**Purpose**: Ingest on-chain state and publish normalized market data

**Implementation**:
- Subscribe to relevant accounts via WS
- Decode account updates using protocol decoders
- Publish `MarketDataUpdate::SolanaAccountHealth` events
- Handle account creation/deletion dynamically

**MarketDataUpdate Extension**:
```rust
pub enum MarketDataUpdate {
    // ... existing variants ...
    SolanaAccountHealth {
        protocol: LendingProtocol,
        account: Pubkey,
        health_factor: Decimal,
        liquidation_price: Decimal,
        collateral_value: Decimal,
        debt_value: Decimal,
        max_liquidation_amount: Decimal,
        bonus_percentage: Decimal,
        timestamp: u64,
    },
    SolanaOraclePrice {
        oracle: OracleProvider,
        asset: String,
        price: Decimal,
        confidence: Decimal,
        timestamp: u64,
    },
}
```

**Location**: `src/market_data_engine/src/exchanges/solana/driver.rs`

#### 2.3.2 State Manager Integration

**Purpose**: Store and manage Solana-specific market data state

**Implementation**:
- Extend `MarketDataState` with Solana-specific fields
- Track account health over time
- Filter and prioritize liquidation opportunities
- Cache oracle prices for fast access

**Location**: `src/market_data_engine/src/state/state.rs` (extend existing)

### 2.4 ExchangeEngine Extension

#### 2.4.1 VenueProvider Trait

**Purpose**: Unified interface for all venue operations (state + execution + streaming). Provides a clean abstraction boundary between CEX (API-based) and DEX (blockchain-based) operations.

**Rationale**: DEX operations are fundamentally different from CEX - transactions vs orders, on-chain state vs REST API, signatures vs order IDs. A unified `VenueProvider` abstraction handles state management, execution, and streaming together.

**Implementation**:
```rust
#[async_trait]
pub trait VenueProvider: Send + Sync {
    /// Bootstrap initial state from authoritative source
    async fn bootstrap_state(&self) -> Result<VenueStateSnapshot, EngineError>;
    
    /// Stream real-time updates
    async fn stream_updates(
        &self,
        tx: broadcast::Sender<VenueUpdate>,
    ) -> Result<(), EngineError>;
    
    /// Execute a trade command
    async fn execute(
        &self,
        cmd: TradeCommand,
    ) -> Result<ExecutionResult, EngineError>;
    
    /// Reconcile internal state with authoritative source
    async fn reconcile(&self) -> Result<ReconciliationResult, EngineError>;
}
```

**Location**: `libs/trading_core/src/traits/venue_provider.rs`

#### 2.4.2 Unified Execution Interface

**Purpose**: TradingEngine needs a unified interface to execute trades regardless of venue type (CEX or DEX)

**Implementation**: Create a unified execution trait that abstracts both:

```rust
#[async_trait]
pub trait ExecutionInterface: Send + Sync {
    async fn execute_order(&self, req: OrderRequest) -> Result<Order, EngineError>;
    async fn execute_trade_command(&self, cmd: TradeCommand) -> Result<ExecutionResult, EngineError>;
    // ... other execution methods
}
```

Both `TradingClient` and `VenueProvider` implement this trait:
- `TradingClient` implements `ExecutionInterface::execute_order()` (CEX orders)
- `VenueProvider` implements `ExecutionInterface::execute_trade_command()` (DEX transactions)

**Location**: `libs/trading_core/src/traits/execution_interface.rs`

#### 2.4.3 ExchangeStateManager Integration

**Purpose**: Make `ExchangeStateManager` support both CEX (TradingClient) and DEX (VenueProvider) venues

**Implementation**:
```rust
pub enum VenueInterface {
    Cex {
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        driver: Box<dyn ExchangeDriver>,
        executor: Arc<dyn ExecutionInterface>,  // Wrapper around TradingClient
    },
    Dex {
        venue_provider: Arc<dyn VenueProvider>,
        executor: Arc<dyn ExecutionInterface>,  // Wrapper around VenueProvider
    },
}

pub struct ExchangeStateManager {
    state: Arc<RwLock<ExchangeState>>,
    publisher: ExchangeStatePublisher,
    venue_interface: VenueInterface,
    reconciler: StateReconciler,
    // ... rest stays the same
}

impl ExchangeStateManager {
    /// Get execution interface (unified for CEX/DEX)
    pub fn get_executor(&self) -> Arc<dyn ExecutionInterface> {
        match &self.venue_interface {
            VenueInterface::Cex { executor, .. } => executor.clone(),
            VenueInterface::Dex { executor, .. } => executor.clone(),
        }
    }
}
```

**TradingEngine Integration**: TradingEngine receives `Arc<dyn ExecutionInterface>` instead of `Arc<RwLock<dyn TradingClient>>`. This way:
- Strategies emit `TradeCommand` (unified enum with Order/Liquidate/Swap variants)
- TradingEngine calls `executor.execute_trade_command(cmd)` 
- ExecutionInterface routes to appropriate venue (CEX → TradingClient, DEX → VenueProvider)
- ExchangeStateManager still handles state/reconciliation via venue-specific interfaces

This maintains separation of concerns while keeping TradingEngine venue-agnostic.

**Location**: `src/exchange_engine/src/state/manager.rs` (extend existing)

#### 2.4.3 SolanaVenueProvider

**Purpose**: Solana-specific venue implementation

**Implementation**:
- Wallet management (keypair loading, signing)
- Transaction building (instructions, compute budget, priority fees)
- Transaction submission and confirmation tracking
- Balance and position tracking from on-chain state
- WebSocket subscriptions for real-time updates

**Key Components**:
```rust
pub struct SolanaVenueProvider {
    rpc_client: Arc<SolanaRpcClient>,
    ws_client: Arc<SolanaWsClient>,
    wallet: Arc<Keypair>,
    config: SolanaVenueConfig,
    pending_executions: Arc<Mutex<HashMap<Signature, PendingExecution>>>,
}

pub struct PendingExecution {
    signature: Signature,
    expected_delta: PositionDelta,
    submitted_slot: u64,
    command: TradeCommand,
    created_at: Instant,
}
```

**Location**: `src/exchange_engine/src/exchanges/solana/venue_provider.rs`

#### 2.4.3 TradeCommand Extension

**Purpose**: Support DEX-specific trade commands

**Implementation**:
```rust
pub enum TradeCommand {
    // ... existing CEX commands ...
    Liquidate {
        protocol: LendingProtocol,
        account: Pubkey,
        repay_asset: String,
        collateral_asset: String,
        amount: Decimal,
        min_collateral_received: Decimal,
    },
    DexSwap {
        protocol: DexProtocol,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount_in: Decimal,
        min_amount_out: Decimal,
        slippage_bps: u16,
    },
}
```

**Location**: `libs/trading_core/src/types/common.rs` (extend existing)

#### 2.4.4 ExchangeState Extension

**Purpose**: Track on-chain wallet state and pending transactions

**Implementation**:
```rust
pub struct ExchangeState {
    // ... existing fields ...
    pub solana_wallet: Option<SolanaWalletState>,
    pub pending_transactions: HashMap<Signature, PendingTransaction>,
}

pub struct SolanaWalletState {
    pub pubkey: Pubkey,
    pub balances: HashMap<String, Decimal>, // token mint -> balance
    pub last_updated_slot: u64,
}

pub struct PendingTransaction {
    signature: Signature,
    expected_delta: PositionDelta,
    submitted_slot: u64,
    status: TransactionStatus,
}
```

**Location**: `src/exchange_engine/src/state/state.rs` (extend existing)

### 2.5 TradingEngine Extension

#### 2.5.1 LiquidationStrategy

**Purpose**: Detect and execute liquidation opportunities

**Implementation**:
```rust
pub struct LiquidationStrategy {
    config: LiquidationConfig,
    risk_manager: Arc<RiskManager>,
    min_profit_threshold: Decimal,
    min_health_factor: Decimal, // e.g., 0.995 for buffer
}

impl Strategy for LiquidationStrategy {
    fn evaluate(
        &self,
        market_data: &MarketDataUpdate,
        exchange_state: &ExchangeState,
        trading_state: &TradingState,
    ) -> Option<TradeCommand> {
        match market_data {
            MarketDataUpdate::SolanaAccountHealth { .. } => {
                // Check health factor < threshold
                // Check wallet has required asset
                // Check risk limits
                // Compute expected profit
                // Emit TradeCommand::Liquidate
            }
            _ => None,
        }
    }
}
```

**Strategy Logic**:
1. Subscribe to `MarketDataUpdate::SolanaAccountHealth`
2. Filter: `health_factor < min_health_factor` (e.g., 0.995)
3. Check: Wallet has sufficient balance for repayment
4. Check: Risk limits allow this liquidation
5. Compute: Expected profit = (collateral_value + bonus) - (repay_amount + fees)
6. Filter: `expected_profit > min_profit_threshold`
7. Emit: `TradeCommand::Liquidate`

**Location**: `src/trading_engine/src/strategies/liquidation.rs`

#### 2.5.2 Post-Execution Handling

**Purpose**: Handle transaction lifecycle and reconciliation

**Implementation**:
- Track pending liquidations in `TradingState`
- Wait for transaction confirmation
- Reconcile expected vs actual position deltas
- Handle failures gracefully (revert pending state)
- Log success/failure for analysis

**Transaction Lifecycle**:
```
Built → Signed → Submitted → InMempool → IncludedInBlock → Confirmed / Reverted
```

**Location**: `src/trading_engine/src/strategies/liquidation.rs` (extend)

### 2.6 RiskEngine Integration

**Purpose**: Apply existing risk limits to DEX strategies

**Existing Risk Checks (Already Compatible)**:
- Max capital per trade
- Max exposure per asset
- Daily loss limit
- Circuit breaker on repeated failures

**DEX-Specific Considerations**:
- Slippage limits for swaps
- Gas/fee budget limits
- Transaction confirmation timeout limits
- Max pending transactions

**Location**: `src/trading_engine/src/risk/` (extend existing)

### 2.7 Reconciliation

**Purpose**: Ensure internal state matches on-chain reality

**Implementation**:
- Periodic authoritative reads from RPC
- Compare wallet balances vs internal state
- Compare pending transactions vs on-chain status
- Reconcile position deltas
- Handle discrepancies (log, alert, auto-correct if safe)

**Reconciliation Flow**:
```
ExchangeStateManager::reconcile()
  → SolanaVenueProvider::reconcile()
    → RPC: getAccountInfo(wallet_pubkey)
    → RPC: getSignatureStatuses(pending_signatures)
    → Compare vs internal state
    → Update ExchangeState
    → Publish ExchangeStateUpdate
```

**Location**: `src/exchange_engine/src/exchanges/solana/venue_provider.rs` (reconcile method)

---

## 3. Implementation Phases

### Phase 1: Foundation (Weeks 1-3)

**Goal**: Basic Solana connectivity and data ingestion

**Tasks**:
1. ✅ Set up Solana RPC client (HTTP + WS)
2. ✅ Implement account subscription manager
3. ✅ Create protocol decoder framework (Marginfi first)
4. ✅ Extend `MarketDataUpdate` with Solana variants
5. ✅ Implement `SolanaMarketDataDriver`
6. ✅ Test data ingestion on devnet

**Deliverables**:
- Solana RPC/WS client working
- Account health data flowing into MarketDataEngine
- TUI shows Solana account health updates

**Dependencies**:
- Solana Rust SDK
- RPC endpoint (public devnet for testing)

### Phase 2: Execution Infrastructure (Weeks 4-6)

**Goal**: Transaction building and submission

**Tasks**:
1. ✅ Implement `VenueProvider` trait
2. ✅ Implement `SolanaVenueProvider` with wallet management
3. ✅ Extend `ExchangeStateManager` to support `VenueInterface` enum (CEX/DEX)
4. ✅ Extend `TradeCommand` with `Liquidate` variant
5. ✅ Implement liquidation instruction builder (Marginfi)
6. ✅ Transaction assembly (blockhash, compute budget, priority fees)
7. ✅ Transaction submission and signature tracking
8. ✅ Extend `ExchangeState` with Solana wallet state

**Deliverables**:
- Can build and submit liquidation transactions
- Transaction lifecycle tracking
- Wallet balance tracking from on-chain

**Dependencies**:
- Marginfi program IDL
- Test wallet with devnet SOL

### Phase 3: Liquidation Strategy (Weeks 7-9)

**Goal**: Automated liquidation detection and execution

**Tasks**:
1. ✅ Implement `LiquidationStrategy`
2. ✅ Integrate with RiskEngine
3. ✅ Implement sizing logic (max liquidation %, wallet balance, risk limits)
4. ✅ Post-execution handling (confirmation, reconciliation)
5. ✅ Error handling (front-running, insufficient balance, tx failures)
6. ✅ Logging and metrics

**Deliverables**:
- Fully automated liquidation bot
- Risk limits enforced
- Success/failure tracking

**Dependencies**:
- Phase 1 & 2 complete
- Test accounts on devnet with undercollateralized positions

### Phase 4: Reconciliation & Reliability (Weeks 10-11)

**Goal**: Production-ready state management

**Tasks**:
1. ✅ Implement reconciliation logic
2. ✅ Handle edge cases (tx timeouts, network issues, program upgrades)
3. ✅ Circuit breaker for repeated failures
4. ✅ Monitoring and alerting
5. ✅ Performance optimization (subscription limits, batching)

**Deliverables**:
- Robust reconciliation
- Graceful error handling
- Production monitoring

**Dependencies**:
- All previous phases

### Phase 5: Mainnet Deployment (Weeks 12-16)

**Goal**: Deploy to mainnet with small capital

**Tasks**:
1. ✅ Switch to mainnet RPC (paid provider recommended)
2. ✅ Security audit (wallet handling, key management)
3. ✅ Paper trading validation
4. ✅ Small capital deployment
5. ✅ Monitor and tune parameters
6. ✅ Scale gradually

**Deliverables**:
- Live mainnet liquidation bot
- PnL tracking
- Operational runbook

**Dependencies**:
- Paid RPC provider account
- Mainnet wallet with capital
- Security review

**Note**: Timeline includes 2-4 week buffer for unexpected issues (protocol documentation gaps, transaction building edge cases, RPC provider integration quirks). **Total timeline: 16-18 weeks** is realistic.

---

## 4. Dependencies & Prerequisites

### 4.1 Rust Crates

```toml
[dependencies]
# Solana core
solana-sdk = "1.18"
solana-client = "1.18"
solana-transaction-status = "1.18"

# Protocol-specific (as needed)
# marginfi-sdk = "..." (if available)
# solend-sdk = "..." (if available)

# WebSocket
tokio-tungstenite = "0.21"
futures-util = "0.3"

# Serialization
borsh = "1.5"  # Solana account serialization
anchor-client = "0.29"  # If using Anchor programs
```

### 4.2 External Services

**RPC Provider** (choose one):
- **Public RPC**: Free, rate-limited, not for production
- **Paid Provider**: Recommended for production
  - Helius, QuickNode, Triton, Alchemy
  - Cost: $50-300/month depending on tier
  - Features: Higher rate limits, stable WS, priority lanes

**Wallet Management**:
- Keypair storage (encrypted, environment variables or secure vault)
- Hot wallet for trading (separate from cold storage)

### 4.3 Protocol Documentation

**Marginfi** (recommended first):
- Program IDL
- Account layouts
- Liquidation instruction parameters
- Bonus calculation

**Solend** (alternative):
- Program documentation
- Account structures
- Liquidation flow

### 4.4 Development Environment

- Solana CLI tools (for local testing)
- Devnet SOL (for testing)
- Test accounts with undercollateralized positions (for validation)

---

## 5. Configuration

### 5.1 Solana Configuration

```json
{
  "solana": {
    "rpc": {
      "http_url": "https://api.mainnet-beta.solana.com",
      "ws_url": "wss://api.mainnet-beta.solana.com",
      "commitment": "confirmed",
      "timeout_ms": 30000,
      "max_retries": 3
    },
    "wallet": {
      "keypair_path": "/path/to/keypair.json",
      "encrypted": true
    },
    "protocols": {
      "marginfi": {
        "program_id": "...",
        "markets": ["..."],
        "enabled": true
      }
    }
  }
}
```

### 5.2 Liquidation Strategy Configuration

```json
{
  "strategies": {
    "liquidation": {
      "enabled": true,
      "protocols": ["marginfi"],
      "min_health_factor": 0.995,
      "min_profit_threshold": 0.001,  // 0.1% minimum profit
      "max_capital_per_liquidation": 1000.0,
      "max_pending_transactions": 5,
      "confirmation_timeout_secs": 30
    }
  }
}
```

---

## 6. Risk Considerations

### 6.1 Technical Risks

**Transaction Failures**:
- Front-running by other bots
- Insufficient priority fees
- Network congestion
- Program upgrades

**Mitigation**:
- Fast detection and submission
- Dynamic priority fee adjustment
- Retry logic with backoff
- Monitor program upgrades

**State Synchronization**:
- Node lagging behind tip
- Stale account data
- Pending transaction confusion

**Mitigation**:
- Use confirmed/finalized commitment levels
- Periodic reconciliation
- Transaction timeout handling
- Clear pending state management

### 6.2 Financial Risks

**Capital Risk**:
- Insufficient balance for liquidation
- Slippage on collateral conversion
- Gas fees exceeding profit

**Mitigation**:
- Conservative sizing
- Slippage limits
- Minimum profit thresholds
- Reserve capital for fees

**Competition Risk**:
- Other bots faster/more sophisticated
- MEV searchers front-running

**Mitigation**:
- Focus on less competitive protocols initially
- Optimize detection and submission speed
- Consider priority fee strategies

### 6.3 Operational Risks

**RPC Provider Issues**:
- Downtime
- Rate limiting
- Latency spikes

**Mitigation**:
- Multiple RPC endpoints (primary + fallback)
- Circuit breakers
- Monitoring and alerting

**Wallet Security**:
- Keypair compromise
- Unauthorized transactions

**Mitigation**:
- Encrypted keypair storage
- Hot wallet with limited funds
- Transaction signing verification
- Audit logging

---

## 7. Expected Performance & Success Metrics

### 7.1 Realistic Performance Expectations

Based on typical DeFi liquidation bot performance:

| Metric | Conservative | Realistic | Optimistic |
|--------|-------------|-----------|------------|
| Opportunities/day | 2-5 | 10-20 | 30+ |
| Success rate | 30-40% | 50-60% | 70-80% |
| Avg profit per success | $20-50 | $50-100 | $100-200 |
| Capital required | $5K-10K | $10K-25K | $25K-50K |
| Monthly ROI | 2-5% | 5-15% | 15-30% |

**Note**: Actual performance depends on protocol selection, market volatility, competition level, execution speed, and capital allocation. Not a get-rich-quick scheme, but viable with proper execution.

### 7.2 Technical Metrics

- **Detection Latency**: Time from health_factor < 1.0 to TradeCommand emitted
- **Execution Latency**: Time from TradeCommand to transaction submitted
- **Confirmation Rate**: % of transactions confirmed within timeout
- **Reconciliation Accuracy**: % of state matches between internal and on-chain

### 7.3 Business Metrics

- **Opportunities Detected**: Number of liquidation opportunities per day
- **Execution Success Rate**: % of attempts that succeed
- **Average Profit per Liquidation**: Net profit after fees
- **Capital Efficiency**: Profit / capital deployed
- **Missed Opportunities**: Opportunities detected but not executed (with reasons)

### 7.4 Operational Metrics

- **Uptime**: System availability
- **RPC Error Rate**: Failed RPC calls / total calls
- **Subscription Stability**: WS reconnection frequency
- **Reconciliation Discrepancies**: Number and severity of state mismatches

---

## 8. Future Extensions

### 8.1 Additional Strategies

Once liquidations are proven:

1. **Cross-AMM Arbitrage**: Raydium ↔ Orca ↔ Meteora price differences
2. **Backrun Large Swaps**: React to large swaps and trade back to equilibrium
3. **Oracle vs Pool Discrepancies**: Exploit timing mismatches

### 8.2 Additional Protocols

- **Solend**: Second lending protocol
- **Drift**: Perpetuals and spot margin
- **Kamino**: Select markets

### 8.3 Infrastructure Upgrades

- **Geyser/Yellowstone**: High-speed data firehose for thousands of accounts
- **Self-Hosted Node**: When RPC costs become material
- **Multi-Chain**: Extend to Sui or other chains

---

## 9. Open Questions

1. **Protocol Selection**: Marginfi vs Solend for first implementation?
2. **RPC Provider**: Which provider and tier to start with?
3. **Capital Allocation**: Initial capital for testing and scaling plan?
4. **Monitoring**: Integration with existing TUI or separate dashboard?
5. **Security Review**: Internal review or external audit before mainnet?

---

## 10. References

### 10.1 Solana Documentation

- [Solana JSON-RPC API](https://docs.solana.com/api/http)
- [Solana WebSocket PubSub](https://docs.solana.com/api/websocket)
- [Solana Transaction Processing](https://docs.solana.com/developing/programming-model/transactions)

### 10.2 Protocol Documentation

- Marginfi: [GitHub](https://github.com/marginfi), [Docs](https://docs.marginfi.com)
- Solend: [GitHub](https://github.com/solendprotocol), [Docs](https://docs.solend.fi)

### 10.3 Community Resources

- Solana Discord
- Protocol-specific Discord servers
- MEV/Bot strategy discussions

---

## Appendix A: Example Liquidation Flow

### Step-by-Step Execution

1. **MarketDataEngine** receives account update via WS subscription
2. **Protocol Decoder** decodes account bytes → `LendingAccountHealth`
3. **MarketDataDriver** publishes `MarketDataUpdate::SolanaAccountHealth`
4. **LiquidationStrategy** receives update, evaluates:
   - `health_factor = 0.98 < 0.995` ✅
   - `wallet_balance = 1500 USDC > 1000 USDC required` ✅
   - `risk_check = PASS` ✅
   - `expected_profit = $50 > $10 minimum` ✅
5. **LiquidationStrategy** emits `TradeCommand::Liquidate { ... }`
6. **ExchangeEngine** receives command, routes to `SolanaVenueProvider`
7. **SolanaVenueProvider** builds liquidation transaction:
   - Fetches recent blockhash
   - Adds liquidation instruction
   - Sets compute budget and priority fee
   - Signs with wallet keypair
8. **SolanaVenueProvider** submits transaction via RPC
9. **SolanaVenueProvider** tracks signature via `signatureSubscribe`
10. **Transaction confirmed** → Update `ExchangeState` with position delta
11. **TradingStateManager** reconciles expected vs actual
12. **TUI** displays successful liquidation with PnL

---

## Appendix B: Code Structure Overview

```
src/
├── exchange_engine/
│   └── src/
│       ├── exchanges/
│       │   ├── binance/          # Existing
│       │   └── solana/           # NEW
│       │       ├── rpc_client.rs
│       │       ├── ws_client.rs
│       │       ├── venue_provider.rs
│       │       ├── wallet.rs
│       │       └── protocols/
│       │           └── marginfi/
│       │               ├── decoder.rs
│       │               ├── types.rs
│       │               └── liquidation.rs
│       └── state/
│           └── manager.rs        # Extend with VenueInterface enum
├── market_data_engine/
│   └── src/
│       ├── exchanges/
│       │   ├── binance/          # Existing
│       │   └── solana/           # NEW
│       │       ├── driver.rs
│       │       ├── ws_client.rs
│       │       ├── account_discovery.rs
│       │       └── protocols/
│       │           ├── marginfi/
│       │           │   └── decoder.rs
│       │           └── common/
│       │               └── normalized.rs
└── trading_engine/
    └── src/
        └── strategies/
            ├── strategies.rs     # Existing
            └── liquidation.rs    # NEW

libs/
└── trading_core/
    └── src/
        ├── traits/
        │   └── venue_provider.rs # NEW trait
        └── types/
            └── common.rs         # Extend TradeCommand
```

---

**End of Proposal**

