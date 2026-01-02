# Proposal: DEX Integration Preparation

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft  
**Prerequisites**: None (foundational infrastructure)

## Executive Summary

This proposal outlines the foundational infrastructure needed in `trading_core` to support DEX (Decentralized Exchange) integrations. This includes new traits, types, and abstractions that allow the system to work with both CEX (Centralized Exchange) and DEX venues in a unified manner. This is a **preparation phase** that must be completed before any specific DEX (like Solana) can be integrated.

### Key Objectives

1. **Create `VenueProvider` trait** - Unified interface for DEX operations (state + execution + streaming)
2. **Create `ExecutionInterface` trait** - Unified execution abstraction for both CEX and DEX
3. **Extend `TradeCommand`** - Add DEX-specific command variants (Liquidate, DexSwap, etc.)
4. **Extend `ExchangeState`** - Support DEX wallet state and pending transactions
5. **Extend `ExchangeStateManager`** - Support both CEX and DEX venues via `VenueInterface` enum
6. **Extend `MarketDataUpdate`** - Support DEX-specific market data events
7. **Maintain architectural integrity** - All changes are extensions, no breaking changes

---

## Phase 1: Core Traits

### 1.1 VenueProvider Trait

**Location**: `libs/trading_core/src/traits/venue_provider.rs` (new file)

**Purpose**: Unified interface for all DEX venue operations (state + execution + streaming). Provides a clean abstraction boundary between CEX (API-based) and DEX (blockchain-based) operations.

**Rationale**: DEX operations are fundamentally different from CEX:
- **Transactions** vs orders
- **On-chain state** vs REST API
- **Signatures** vs order IDs
- **Blockchain confirmation** vs instant fills

A unified `VenueProvider` abstraction handles state management, execution, and streaming together.

**Implementation**:
```rust
use async_trait::async_trait;
use crate::error::EngineError;
use tokio::sync::broadcast;

/// Unified interface for DEX venue operations
#[async_trait]
pub trait VenueProvider: Send + Sync + 'static {
    /// Bootstrap initial state from authoritative source (on-chain)
    async fn bootstrap_state(&self) -> Result<VenueStateSnapshot, EngineError>;
    
    /// Stream real-time updates (account changes, transaction confirmations, etc.)
    async fn stream_updates(
        &self,
        tx: broadcast::Sender<VenueUpdate>,
    ) -> Result<(), EngineError>;
    
    /// Execute a trade command (build, sign, submit transaction)
    async fn execute(
        &self,
        cmd: TradeCommand,
    ) -> Result<ExecutionResult, EngineError>;
    
    /// Reconcile internal state with authoritative source (on-chain)
    async fn reconcile(&self) -> Result<ReconciliationResult, EngineError>;
    
    /// Get venue identifier
    fn venue_id(&self) -> VenueId;
}

/// Snapshot of venue state (wallet balances, positions, etc.)
#[derive(Debug, Clone)]
pub struct VenueStateSnapshot {
    pub wallet_balances: HashMap<String, Decimal>, // asset -> balance
    pub positions: Vec<Position>,
    pub pending_transactions: Vec<PendingTransaction>,
    pub last_updated_slot: Option<u64>, // For blockchain-based venues
}

/// Real-time venue updates
#[derive(Debug, Clone)]
pub enum VenueUpdate {
    /// Wallet balance changed
    BalanceUpdate {
        asset: String,
        new_balance: Decimal,
        delta: Decimal,
    },
    /// Position changed
    PositionUpdate(Position),
    /// Transaction status changed
    TransactionUpdate {
        signature: String,
        status: TransactionStatus,
    },
    /// Account data changed (for protocol-specific accounts)
    AccountDataUpdate {
        account: String,
        data: Vec<u8>,
    },
}

/// Result of execution attempt
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub signature: String, // Transaction signature or order ID
    pub status: ExecutionStatus,
    pub expected_delta: Option<PositionDelta>,
    pub submitted_at: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
    Submitted,
    Confirmed,
    Failed(String),
}

/// Result of reconciliation
#[derive(Debug, Clone)]
pub struct ReconciliationResult {
    pub discrepancies: Vec<StateDiscrepancy>,
    pub reconciled_at: u64,
}

#[derive(Debug, Clone)]
pub struct StateDiscrepancy {
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub severity: DiscrepancySeverity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiscrepancySeverity {
    Warning,
    Error,
    Critical,
}

/// Venue identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VenueId {
    Solana,
    Ethereum,
    // ... other DEX venues
}
```

### 1.2 ExecutionInterface Trait

**Location**: `libs/trading_core/src/traits/execution_interface.rs` (new file)

**Purpose**: TradingEngine needs a unified interface to execute trades regardless of venue type (CEX or DEX). This trait abstracts both `TradingClient` (CEX) and `VenueProvider` (DEX) execution.

**Implementation**:
```rust
use async_trait::async_trait;
use crate::error::EngineError;
use crate::types::exchange::{Order, OrderRequest};
use crate::types::common::TradeCommand;

/// Unified execution interface for both CEX and DEX
#[async_trait]
pub trait ExecutionInterface: Send + Sync + 'static {
    /// Execute an order (CEX-style)
    /// For CEX: Direct order placement
    /// For DEX: Convert to transaction and execute
    async fn execute_order(&self, req: OrderRequest) -> Result<Order, EngineError>;
    
    /// Execute a trade command (unified for both CEX and DEX)
    /// For CEX: Convert TradeCommand to OrderRequest
    /// For DEX: Execute directly via VenueProvider
    async fn execute_trade_command(&self, cmd: TradeCommand) -> Result<ExecutionResult, EngineError>;
    
    /// Cancel an order/transaction
    async fn cancel(&self, id: String) -> Result<(), EngineError>;
    
    /// Get venue type
    fn venue_type(&self) -> VenueType;
}

#[derive(Debug, Clone, PartialEq)]
pub enum VenueType {
    Cex,
    Dex,
}
```

**Implementation Strategy**:
- `TradingClient` implements `ExecutionInterface` (wraps CEX operations)
- `VenueProvider` implements `ExecutionInterface` (wraps DEX operations)
- `ExchangeStateManager` provides `Arc<dyn ExecutionInterface>` to TradingEngine

---

## Phase 2: Type Extensions

### 2.1 TradeCommand Extension

**Location**: `libs/trading_core/src/types/common.rs` (extend existing)

**Purpose**: Support DEX-specific trade commands alongside existing CEX commands.

**Implementation**:
```rust
/// Unified trade command enum (CEX + DEX)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum TradeCommand {
    // Existing CEX commands
    PlaceOrder(OrderRequest),
    CancelOrder(OrderId),
    CancelAllOrders(Pair),
    
    // DEX-specific commands
    /// Liquidate an undercollateralized position
    Liquidate {
        protocol: LendingProtocol,
        account: String, // Account address/pubkey
        repay_asset: String,
        collateral_asset: String,
        amount: Decimal,
        min_collateral_received: Decimal,
    },
    /// Execute a DEX swap
    DexSwap {
        protocol: DexProtocol,
        input_mint: String, // Token mint address
        output_mint: String,
        amount_in: Decimal,
        min_amount_out: Decimal,
        slippage_bps: u16,
    },
    /// Place a limit order on a DEX
    DexLimitOrder {
        protocol: DexProtocol,
        input_mint: String,
        output_mint: String,
        amount_in: Decimal,
        limit_price: Decimal,
        expiry: Option<u64>, // Unix timestamp
    },
}

/// Lending protocol identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum LendingProtocol {
    Marginfi,
    Solend,
    // ... other protocols
}

/// DEX protocol identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum DexProtocol {
    Raydium,
    Orca,
    Meteora,
    Jupiter, // Aggregator
    // ... other DEXes
}
```

### 2.2 ExchangeState Extension

**Location**: `src/exchange_engine/src/state/state.rs` (extend existing)

**Purpose**: Track on-chain wallet state and pending transactions for DEX venues.

**Implementation**:
```rust
pub struct ExchangeState {
    // ... existing CEX fields ...
    
    /// DEX wallet state (for blockchain-based venues)
    pub dex_wallet: Option<DexWalletState>,
    
    /// Pending transactions (DEX) or orders (CEX)
    pub pending_executions: HashMap<String, PendingExecution>,
}

#[derive(Debug, Clone)]
pub struct DexWalletState {
    /// Wallet address/pubkey
    pub address: String,
    
    /// Token balances (mint -> balance)
    pub balances: HashMap<String, Decimal>,
    
    /// Last updated slot/block number
    pub last_updated_slot: Option<u64>,
    
    /// Last update timestamp
    pub last_updated: u64,
}

#[derive(Debug, Clone)]
pub struct PendingExecution {
    /// Transaction signature or order ID
    pub id: String,
    
    /// Expected position delta
    pub expected_delta: Option<PositionDelta>,
    
    /// Submitted slot/block number
    pub submitted_slot: Option<u64>,
    
    /// Command that triggered this execution
    pub command: TradeCommand,
    
    /// Execution status
    pub status: ExecutionStatus,
    
    /// Created timestamp
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
    Pending,
    Submitted,
    Confirmed,
    Failed(String),
    Expired,
}

#[derive(Debug, Clone)]
pub struct PositionDelta {
    pub asset: String,
    pub delta: Decimal, // Positive = increase, negative = decrease
}
```

### 2.3 MarketDataUpdate Extension

**Location**: `libs/trading_core/src/types/market_data.rs` (extend existing)

**Purpose**: Support DEX-specific market data events (account health, oracle prices, etc.).

**Implementation**:
```rust
pub enum MarketDataUpdate {
    // ... existing CEX variants ...
    
    // DEX-specific variants
    /// Lending account health update
    DexAccountHealth {
        protocol: LendingProtocol,
        account: String,
        health_factor: Decimal,
        liquidation_price: Decimal,
        collateral_value: Decimal,
        debt_value: Decimal,
        max_liquidation_amount: Decimal,
        bonus_percentage: Decimal,
        timestamp: u64,
    },
    
    /// Oracle price update
    DexOraclePrice {
        oracle: OracleProvider,
        asset: String,
        price: Decimal,
        confidence: Decimal,
        timestamp: u64,
    },
    
    /// DEX pool state update
    DexPoolState {
        protocol: DexProtocol,
        pool: String,
        token_a: String,
        token_b: String,
        reserve_a: Decimal,
        reserve_b: Decimal,
        price: Decimal,
        timestamp: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OracleProvider {
    Pyth,
    Switchboard,
    Chainlink,
}
```

---

## Phase 3: ExchangeStateManager Integration

### 3.1 VenueInterface Enum

**Location**: `src/exchange_engine/src/state/manager.rs` (extend existing)

**Purpose**: Make `ExchangeStateManager` support both CEX (TradingClient) and DEX (VenueProvider) venues.

**Implementation**:
```rust
use trading_core::traits::{TradingClient, VenueProvider, ExecutionInterface};
use trading_core::traits::exchange_driver::ExchangeDriver;

/// Unified venue interface (CEX or DEX)
pub enum VenueInterface {
    /// Centralized Exchange venue
    Cex {
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        driver: Box<dyn ExchangeDriver>,
        executor: Arc<dyn ExecutionInterface>, // Wrapper around TradingClient
    },
    /// Decentralized Exchange venue
    Dex {
        venue_provider: Arc<dyn VenueProvider>,
        executor: Arc<dyn ExecutionInterface>, // Wrapper around VenueProvider
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
    /// Create new CEX state manager (existing functionality)
    pub fn new_cex(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        driver: Box<dyn ExchangeDriver>,
        // ... other params
    ) -> Self {
        let executor: Arc<dyn ExecutionInterface> = Arc::new(CexExecutor::new(trading_client.clone()));
        Self {
            venue_interface: VenueInterface::Cex {
                trading_client,
                driver,
                executor,
            },
            // ... rest
        }
    }
    
    /// Create new DEX state manager (new functionality)
    pub fn new_dex(
        venue_provider: Arc<dyn VenueProvider>,
        // ... other params
    ) -> Self {
        let executor: Arc<dyn ExecutionInterface> = Arc::new(DexExecutor::new(venue_provider.clone()));
        Self {
            venue_interface: VenueInterface::Dex {
                venue_provider,
                executor,
            },
            // ... rest
        }
    }
    
    /// Get execution interface (unified for CEX/DEX)
    pub fn get_executor(&self) -> Arc<dyn ExecutionInterface> {
        match &self.venue_interface {
            VenueInterface::Cex { executor, .. } => executor.clone(),
            VenueInterface::Dex { executor, .. } => executor.clone(),
        }
    }
    
    /// Get venue type
    pub fn venue_type(&self) -> VenueType {
        match &self.venue_interface {
            VenueInterface::Cex { .. } => VenueType::Cex,
            VenueInterface::Dex { .. } => VenueType::Dex,
        }
    }
}
```

### 3.2 Executor Wrappers

**Location**: `src/exchange_engine/src/state/executors.rs` (new file)

**Purpose**: Wrapper implementations that make `TradingClient` and `VenueProvider` implement `ExecutionInterface`.

**Implementation**:
```rust
use trading_core::traits::{TradingClient, VenueProvider, ExecutionInterface};
use trading_core::types::common::TradeCommand;

/// CEX executor wrapper
pub struct CexExecutor {
    client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
}

impl CexExecutor {
    pub fn new(client: Arc<RwLock<dyn TradingClient<OrderRequest>>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ExecutionInterface for CexExecutor {
    async fn execute_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        self.client.read().await.open_order(req).await
    }
    
    async fn execute_trade_command(&self, cmd: TradeCommand) -> Result<ExecutionResult, EngineError> {
        match cmd {
            TradeCommand::PlaceOrder(req) => {
                let order = self.client.read().await.open_order(req).await?;
                Ok(ExecutionResult {
                    signature: order.order_id.as_str().to_string(),
                    status: ExecutionStatus::Submitted,
                    expected_delta: None,
                    submitted_at: order.internal_created_at,
                })
            }
            // Handle other CEX commands...
            _ => Err(EngineError::NotSupported("Command not supported for CEX".into())),
        }
    }
    
    fn venue_type(&self) -> VenueType {
        VenueType::Cex
    }
}

/// DEX executor wrapper
pub struct DexExecutor {
    provider: Arc<dyn VenueProvider>,
}

impl DexExecutor {
    pub fn new(provider: Arc<dyn VenueProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl ExecutionInterface for DexExecutor {
    async fn execute_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        // Convert OrderRequest to TradeCommand, then execute
        let cmd = TradeCommand::PlaceOrder(req);
        let result = self.provider.execute(cmd).await?;
        
        // Convert ExecutionResult to Order
        // This is a simplified conversion - actual implementation may vary
        Ok(Order {
            order_id: OrderId::new(result.signature.into()),
            // ... map from ExecutionResult
        })
    }
    
    async fn execute_trade_command(&self, cmd: TradeCommand) -> Result<ExecutionResult, EngineError> {
        self.provider.execute(cmd).await
    }
    
    fn venue_type(&self) -> VenueType {
        VenueType::Dex
    }
}
```

---

## Phase 4: TradingEngine Integration

### 4.1 TradingEngine Updates

**Location**: `src/trading_engine/src/strategies/executor.rs` (extend existing)

**Purpose**: TradingEngine should receive `Arc<dyn ExecutionInterface>` instead of `Arc<RwLock<dyn TradingClient>>` to support both CEX and DEX.

**Changes**:
```rust
// Before:
pub struct StrategyExecutor {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    // ...
}

// After:
pub struct StrategyExecutor {
    executor: Arc<dyn ExecutionInterface>,
    // ...
}

impl StrategyExecutor {
    pub fn new(executor: Arc<dyn ExecutionInterface>) -> Self {
        Self { executor, /* ... */ }
    }
    
    async fn execute_command(&self, cmd: TradeCommand) -> Result<(), EngineError> {
        self.executor.execute_trade_command(cmd).await?;
        Ok(())
    }
}
```

---

## Implementation Phases

### Phase 1: Core Traits (Week 1)
- [ ] Create `VenueProvider` trait
- [ ] Create `ExecutionInterface` trait
- [ ] Add unit tests for trait definitions
- [ ] Document trait contracts

### Phase 2: Type Extensions (Week 1-2)
- [ ] Extend `TradeCommand` with DEX variants
- [ ] Extend `ExchangeState` with DEX fields
- [ ] Extend `MarketDataUpdate` with DEX variants
- [ ] Add serialization tests
- [ ] Update type documentation

### Phase 3: ExchangeStateManager Integration (Week 2)
- [ ] Add `VenueInterface` enum
- [ ] Extend `ExchangeStateManager` with DEX support
- [ ] Implement `CexExecutor` wrapper
- [ ] Implement `DexExecutor` wrapper
- [ ] Add integration tests
- [ ] Ensure backward compatibility

### Phase 4: TradingEngine Integration (Week 2-3)
- [ ] Update `StrategyExecutor` to use `ExecutionInterface`
- [ ] Update strategy trait to emit `TradeCommand`
- [ ] Add tests for unified execution
- [ ] Update documentation

### Phase 5: Testing & Validation (Week 3)
- [ ] End-to-end tests with mock CEX
- [ ] End-to-end tests with mock DEX
- [ ] Backward compatibility tests
- [ ] Performance tests
- [ ] Documentation review

---

## Success Criteria

- [ ] All traits defined and documented
- [ ] All type extensions implemented
- [ ] `ExchangeStateManager` supports both CEX and DEX
- [ ] `TradingEngine` can execute via unified `ExecutionInterface`
- [ ] No breaking changes to existing CEX functionality
- [ ] All tests passing
- [ ] Documentation complete
- [ ] Ready for DEX-specific implementations (Solana, etc.)

---

## Design Principles

1. **Backward Compatible**: All changes are extensions, existing CEX code continues to work
2. **Unified Interface**: Same execution path for CEX and DEX at TradingEngine level
3. **Type Safety**: Strong typing prevents mixing CEX and DEX operations incorrectly
4. **Separation of Concerns**: Venue-specific logic stays in venue-specific implementations
5. **Testability**: All abstractions are testable with mocks

---

**Next Steps**: Once this proposal is complete, proceed to Proposal 04.02 (Solana Integration) which will implement the Solana-specific `VenueProvider`.

