# Proposal: Solana DEX Integration - Lending Protocol Liquidations

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft  
**Prerequisites**: Proposal 01 (DEX Preparation) must be completed first

## Executive Summary

This proposal details the **Solana-specific implementation** of the DEX integration infrastructure defined in Proposal 01. It focuses on **lending protocol liquidations** as the initial monetization strategy, implementing the `VenueProvider` trait for Solana and building the necessary Solana RPC/WebSocket clients, protocol decoders, and liquidation strategy.

### Key Objectives

1. **Implement `SolanaVenueProvider`** - Solana-specific implementation of `VenueProvider` trait
2. **Implement Solana RPC/WebSocket clients** - Connectivity to Solana blockchain
3. **Implement protocol decoders** - Marginfi, Solend, and other lending protocols
4. **Implement `LiquidationStrategy`** - Automated liquidation detection and execution
5. **Extend MarketDataEngine** - Ingest Solana on-chain state (pool accounts, oracle accounts, protocol accounts)
6. **Maintain architectural integrity** - All execution via ExchangeEngine, full reconciliation, risk isolation

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

**Note**: `MarketDataUpdate` extensions for DEX are defined in Proposal 01. This section focuses on Solana-specific implementation.

#### 2.3.1 Solana MarketDataDriver

**Purpose**: Ingest Solana on-chain state and publish normalized market data

**Implementation**:
- Subscribe to relevant accounts via WebSocket
- Decode account updates using protocol decoders
- Publish `MarketDataUpdate::DexAccountHealth` events (defined in Proposal 01)
- Handle account creation/deletion dynamically

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

**Note**: The `VenueProvider` trait, `ExecutionInterface` trait, and `ExchangeStateManager` integration are defined in Proposal 01 (DEX Preparation). This section focuses on the Solana-specific implementation.

#### 2.4.1 SolanaVenueProvider

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

**Note**: `TradeCommand` extensions and `ExchangeState` extensions for DEX are defined in Proposal 01. This section focuses on Solana-specific state structures.

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

**Prerequisites**: **Proposal 01 (DEX Preparation) must be completed first.** This foundational proposal defines:
- `VenueProvider` trait - Unified DEX interface
- `ExecutionInterface` trait - Unified execution for CEX/DEX
- `TradeCommand` extensions - DEX-specific command variants
- `ExchangeState` extensions - DEX wallet and transaction tracking
- `ExchangeStateManager` integration - Support for both CEX and DEX venues
- `MarketDataUpdate` extensions - DEX-specific market data events

This proposal (02) implements the Solana-specific `VenueProvider` and all Solana connectivity.

### Phase 1: Foundation (Weeks 1-3)

**Goal**: Basic Solana connectivity and data ingestion

**Tasks**:
1. ✅ Set up Solana RPC client (HTTP + WS)
2. ✅ Implement account subscription manager
3. ✅ Create protocol decoder framework (Marginfi first)
4. ✅ Implement `SolanaMarketDataDriver`
5. ✅ Test data ingestion on devnet

**Deliverables**:
- Solana RPC/WS client working
- Account health data flowing into MarketDataEngine
- TUI shows Solana account health updates

**Dependencies**:
- Proposal 01 complete
- Solana Rust SDK
- RPC endpoint (public devnet for testing)

### Phase 2: Execution Infrastructure (Weeks 4-6)

**Goal**: Transaction building and submission

**Tasks**:
1. ✅ Implement `SolanaVenueProvider` (implements `VenueProvider` trait)
2. ✅ Implement wallet management (keypair loading, signing)
3. ✅ Implement liquidation instruction builder (Marginfi)
4. ✅ Transaction assembly (blockhash, compute budget, priority fees)
5. ✅ Transaction submission and signature tracking
6. ✅ Integrate with `ExchangeStateManager` (using DEX support from Proposal 01)

**Deliverables**:
- Can build and submit liquidation transactions
- Transaction lifecycle tracking
- Wallet balance tracking from on-chain

**Dependencies**:
- Proposal 01 complete
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

## 3. Dependencies & Prerequisites

### 3.1 Prerequisites

**Must Complete First**:
- Proposal 01 (DEX Preparation) - All foundational infrastructure

### 3.2 Rust Crates

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

### 3.3 External Services

**RPC Provider** (choose one):
- **Public RPC**: Free, rate-limited, not for production
- **Paid Provider**: Recommended for production
  - Helius, QuickNode, Triton, Alchemy
  - Cost: $50-300/month depending on tier
  - Features: Higher rate limits, stable WS, priority lanes

**Wallet Management**:
- Keypair storage (encrypted, environment variables or secure vault)
- Hot wallet for trading (separate from cold storage)

### 3.4 Protocol Documentation

**Marginfi** (recommended first):
- Program IDL
- Account layouts
- Liquidation instruction parameters
- Bonus calculation

**Solend** (alternative):
- Program documentation
- Account structures
- Liquidation flow

### 3.5 Development Environment

- Solana CLI tools (for local testing)
- Devnet SOL (for testing)
- Test accounts with undercollateralized positions (for validation)

---

## 4. Configuration

### 4.1 Solana Configuration

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

### 4.2 Liquidation Strategy Configuration

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

## 5. Risk Considerations

### 5.1 Technical Risks

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

### 5.2 Financial Risks

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

### 5.3 Operational Risks

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

## 6. Expected Performance & Success Metrics

### 6.1 Realistic Performance Expectations

Based on typical DeFi liquidation bot performance:

| Metric | Conservative | Realistic | Optimistic |
|--------|-------------|-----------|------------|
| Opportunities/day | 2-5 | 10-20 | 30+ |
| Success rate | 30-40% | 50-60% | 70-80% |
| Avg profit per success | $20-50 | $50-100 | $100-200 |
| Capital required | $5K-10K | $10K-25K | $25K-50K |
| Monthly ROI | 2-5% | 5-15% | 15-30% |

**Note**: Actual performance depends on protocol selection, market volatility, competition level, execution speed, and capital allocation. Not a get-rich-quick scheme, but viable with proper execution.

### 6.2 Technical Metrics

- **Detection Latency**: Time from health_factor < 1.0 to TradeCommand emitted
- **Execution Latency**: Time from TradeCommand to transaction submitted
- **Confirmation Rate**: % of transactions confirmed within timeout
- **Reconciliation Accuracy**: % of state matches between internal and on-chain

### 6.3 Business Metrics

- **Opportunities Detected**: Number of liquidation opportunities per day
- **Execution Success Rate**: % of attempts that succeed
- **Average Profit per Liquidation**: Net profit after fees
- **Capital Efficiency**: Profit / capital deployed
- **Missed Opportunities**: Opportunities detected but not executed (with reasons)

### 6.4 Operational Metrics

- **Uptime**: System availability
- **RPC Error Rate**: Failed RPC calls / total calls
- **Subscription Stability**: WS reconnection frequency
- **Reconciliation Discrepancies**: Number and severity of state mismatches

---

## 7. Future Extensions

### 7.1 Additional Strategies

Once liquidations are proven:

1. **Cross-AMM Arbitrage**: Raydium ↔ Orca ↔ Meteora price differences
2. **Backrun Large Swaps**: React to large swaps and trade back to equilibrium
3. **Oracle vs Pool Discrepancies**: Exploit timing mismatches

### 7.2 Additional Protocols

- **Solend**: Second lending protocol
- **Drift**: Perpetuals and spot margin
- **Kamino**: Select markets

### 7.3 Infrastructure Upgrades

- **Geyser/Yellowstone**: High-speed data firehose for thousands of accounts
- **Self-Hosted Node**: When RPC costs become material
- **Multi-Chain**: Extend to Sui or other chains

---

## 8. Open Questions

1. **Protocol Selection**: Marginfi vs Solend for first implementation?
2. **RPC Provider**: Which provider and tier to start with?
3. **Capital Allocation**: Initial capital for testing and scaling plan?
4. **Monitoring**: Integration with existing TUI or separate dashboard?
5. **Security Review**: Internal review or external audit before mainnet?

---

## 9. References

### 9.1 Solana Documentation

- [Solana JSON-RPC API](https://docs.solana.com/api/http)
- [Solana WebSocket PubSub](https://docs.solana.com/api/websocket)
- [Solana Transaction Processing](https://docs.solana.com/developing/programming-model/transactions)

### 9.2 Protocol Documentation

- Marginfi: [GitHub](https://github.com/marginfi), [Docs](https://docs.marginfi.com)
- Solend: [GitHub](https://github.com/solendprotocol), [Docs](https://docs.solend.fi)

### 9.3 Community Resources

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

