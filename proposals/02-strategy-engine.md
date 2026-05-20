# Proposal: Strategy Engine

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft  
**Prerequisites**: Indicator config generator (implemented — [design log 001](../.cursor/design-log/001-indicator-config-generator.md)). For DEX/Solana strategies: design logs [002](../.cursor/design-log/002-dex-preparation.md) + [003](../.cursor/design-log/003-production-platform-readiness.md) (shipped), then [Proposal 01 — Solana](01-solana-integration.md). CEX-only strategies do not require Solana.

## Executive Summary

This proposal outlines the design and implementation of a **Strategy Engine** that enables developers and traders to define trading strategies using a Python DSL, which are compiled to an Intermediate Representation (IR) format and executed at runtime by a Rust interpreter. This provides flexibility for strategy development while maintaining the performance and safety of Rust execution.

The strategy engine supports two execution models:
1. **IR Interpreter**: Python strategies → IR → Rust interpreter (flexible, acceptable performance)
2. **Native Rust**: Direct Rust implementation (maximum performance for HFT)

Strategies can be updated at runtime (parameters only) while maintaining structure integrity through compile-time validation.

## Objectives

1. **Enable Python strategy DSL** - Allow strategies to be written in Python with readable syntax
2. **IR-based execution** - Compile Python strategies to IR format for runtime interpretation
3. **Runtime parameter updates** - Support updating strategy parameters without redeployment
4. **Support dual execution models** - Both IR interpreter and native Rust strategies
5. **Indicator integration** - Strategies can use system-wide or define strategy-specific indicators
6. **Type safety and validation** - Validate strategy IR at load time and execution time

## Key Design Principles

1. **Compile-Time Structure, Runtime Parameters**: Strategy logic is fixed at build time; parameters can be updated at runtime
2. **IR as Bridge**: IR format bridges Python (development) and Rust (execution)
3. **JSON for IR Storage**: IR stored as JSON for debuggability and human inspection (can optimize to binary later if needed)
4. **Separation of Concerns**: API types, execution runtime, and Python translation are separate modules
5. **Event-Driven Execution**: Strategies react to indicator updates and market data changes
6. **Dual Indicator Support**: Strategies can reference system indicators or define their own

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│ Python Workspace (python/)                                  │
│  Managed with uv (like Cargo for Rust)                      │
│                                                             │
│  strategy_dsl/                                              │
│  ├── pyproject.toml      # Package dependencies             │
│  ├── src/                                                   │
│  │   ├── dsl.py          # Strategy DSL                     │
│  │   ├── compiler.py     # Python → IR compiler             │
│  │   └── validators.py   # Strategy validation              │
│  └── README.md                                               │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ Compile to IR at build time
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Configuration Files (JSON)                                 │
│                                                             │
│  config/strategies/                                        │
│  └── *.json            (strategy IR definitions)            │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ Load at startup
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Strategy Engine (Rust)                                     │
│                                                             │
│  ┌────────────────────────────────────────────────────┐   │
│  │ API (api/)                                         │   │
│  │  - IR definitions (ir.rs)                          │   │
│  │  - Configuration types                             │   │
│  │  - Context types                                   │   │
│  │  - Manager interfaces                              │   │
│  └────────────────────────────────────────────────────┘   │
│                        │                                     │
│       ┌────────────────┼─────────────────┐                  │
│       │                │                 │                  │
│       ▼                ▼                 ▼                  │
│  ┌──────────────┐  ┌────────────┐  ┌──────────────┐        │
│  │ Runner       │  │ Python     │  │ (future)     │        │
│  │              │  │ Core       │  │              │        │
│  │ - Interpreter│  │            │  │              │        │
│  │ - Event Loop │  │ - DSL types│  │              │        │
│  │ - Runtime    │  │ - Translat.│  │              │        │
│  │ - Executor   │  │            │  │              │        │
│  └──────────────┘  └────────────┘  └──────────────┘        │
└─────────────────────────────────────────────────────────────┘
                        │
                        │ Subscribe to updates
                        ▼
                ┌───────────────────┐
                │ Indicator Engine  │
                │ Trading Engine    │
                └───────────────────┘
```

## Strategy IR Format

### IR Structure

```rust
pub struct StrategyIR {
    pub id: SmolStr,                    // Unique identifier
    pub name: SmolStr,                  // Display name
    pub pair: SmolStr,                  // Trading pair
    pub indicators: StrategyIndicators, // Required indicators
    pub logic: StrategyLogic,           // Execution logic
    pub parameters: StrategyParameters, // Runtime-updateable params
    pub metadata: StrategyMetadata,     // Version, timestamps, etc.
}
```

### IR JSON Example

```json
{
  "id": "momentum_breakout",
  "name": "Momentum Breakout Strategy",
  "pair": "BTCUSDT",
  "indicators": {
    "system_indicator_ids": ["sma_20_btcusdt"],
    "strategy_indicators": [
      {
        "id": "ema_12_momentum_breakout",
        "type": "ema",
        "period": 12,
        "source": "close"
      }
    ]
  },
  "logic": {
    "triggers": [
      {
        "type": "indicator_update",
        "indicator_id": "ema_12_momentum_breakout"
      }
    ],
    "conditions": [
      {
        "type": "and",
        "left": {
          "type": "greater_than",
          "left": {"type": "indicator", "id": "ema_12_momentum_breakout"},
          "right": {"type": "indicator", "id": "sma_20_btcusdt"}
        },
        "right": {
          "type": "greater_than",
          "left": {"type": "parameter", "name": "momentum_threshold"},
          "right": {"type": "constant", "value": 0.02}
        }
      }
    ],
    "actions": [
      {
        "type": "generate_order",
        "side": "buy",
        "amount": {"type": "parameter", "name": "order_amount"}
      }
    ]
  },
  "parameters": {
    "order_amount": 0.1,
    "momentum_threshold": 0.02
  },
  "metadata": {
    "version": "1.0.0",
    "created_at": 1234567890,
    "updated_at": 1234567890,
    "active": true
  }
}
```

## Component Architecture

### 1. API Module (`api/`)

**Purpose**: Core types and building blocks

**Components**:

- `ir.rs`: Strategy IR definitions (StrategyIR, StrategyLogic, StrategyParameters, etc.)
- `config/`: Configuration management types (loading, validation, updates)
- `context/`: Execution context types (market state, indicator values, strategy state)
- `manager/`: Strategy management interfaces (lifecycle, registration, updates)

**Responsibilities**:

- Define all core data structures
- Provide validation logic
- Define interfaces and traits
- Type safety guarantees

### 2. Runner Module (`runner/`)

**Purpose**: Runtime execution layer

**Components**:

- `interpreter/`: IR interpreter implementation
- `event_loop/`: Event loop for indicator/market data updates
- `runtime/`: Runtime state management (parameter cache, indicator values)
- `executor/`: Execution engine coordination

**Responsibilities**:

- Load and register strategy IRs
- Subscribe to indicator updates
- Evaluate strategy logic
- Generate orders based on conditions
- Handle runtime parameter updates

### 3. Python Core Module (`python_core/`)

**Purpose**: Python translation support

**Components**:

- `python_dsl/`: Rust types representing Python DSL constructs
- `translation/`: Translation utilities and validation
- `integration/`: Integration helpers (if needed)

**Responsibilities**:

- Define Rust types for Python DSL
- Provide IR validation utilities
- Support translation result processing

**Important**: 
- **Rust code**: `src/strategy_engine/src/python_core/` provides Rust-side IR validation helpers and type definitions
- **Python code**: The actual Python → IR compiler lives in `python/strategy_dsl/` (separate Python workspace)

The Python workspace (`python/`) uses `uv` for dependency management, similar to how Cargo works for Rust. See `python/README.md` for details.

## Strategy Execution Flow

### Loading Flow

```
1. System Startup
   ↓
2. Strategy Engine loads IRs from config/strategies/*.json
   ↓
3. For each strategy IR:
   a. Validate IR structure (api/)
   b. Check indicator dependencies
   c. Merge strategy indicators with system indicators
   d. Register with Runner (runner/)
   ↓
4. Runner subscribes to required indicators
   ↓
5. Strategy ready for execution
```

### Execution Flow

```
1. Indicator Update Received
   ↓
2. Runner event loop receives update
   ↓
3. Check which strategies depend on this indicator
   ↓
4. For each affected strategy:
   a. Gather current state (indicator values, parameters)
   b. Evaluate strategy logic (interpreter in runner/)
   c. Check conditions
   d. Generate order (if conditions met)
   ↓
5. Order sent to Trading Engine
```

### Parameter Update Flow

```
1. Runtime Parameter Update Request (via live_config API)
   ↓
2. Validate parameters (api/)
   ↓
3. Update parameters in Runner runtime state
   ↓
4. Persist to MongoDB
   ↓
5. Strategy continues with new parameters
```

## Python DSL Design

### Strategy Definition

```python
from strategy_dsl import *

@strategy("momentum_breakout")
class MomentumBreakoutStrategy:
    # Define indicators
    indicators = [
        SystemIndicator("sma_20_btcusdt"),
        SimpleIndicator.ema("BTCUSDT", period=12),
    ]
    
    # Define parameters (runtime-updateable)
    parameters = {
        "order_amount": 0.1,
        "momentum_threshold": 0.02,
    }
    
    # Define triggers
    @on_update("ema_12_momentum_breakout")
    def on_indicator_update(self, ema12, sma20):
        # Strategy logic
        if ema12 > sma20 and self.calculate_momentum(ema12, sma20) > self.params.momentum_threshold:
            return Order.buy(amount=self.params.order_amount)
        
        return None
    
    def calculate_momentum(self, ema12, sma20):
        return (ema12 - sma20) / sma20
```

### Compilation to IR

Python DSL → IR compiler generates:

1. Extracts indicator definitions
2. Extracts parameter definitions
3. Converts triggers to IR trigger format
4. Converts logic to IR expression tree
5. Validates all dependencies
6. Exports to JSON IR format

## Runtime vs Compile-Time

### Compile-Time (Fixed)

- **Strategy Structure**: Logic, conditions, triggers
- **Indicator Definitions**: Which indicators used, how they're calculated
- **Order Generation Logic**: When and how orders are generated
- **Strategy Metadata**: ID, name, pair

### Runtime (Updateable)

- **Parameters**: Order amounts, thresholds, stop-loss levels
- **Active Status**: Enable/disable strategy
- **Parameter Values**: All parameter values can be updated

## Phase 1: API Module (Core Types)

### 1.1 IR Definitions (`api/ir.rs`)

**Components**:

- `StrategyIR`: Main IR structure
- `StrategyIndicators`: Indicator requirements
- `StrategyLogic`: Logic representation (conditions, actions)
- `StrategyParameters`: Parameter definitions
- `StrategyMetadata`: Version and metadata

**Responsibilities**:

- Define all IR data structures
- Serialization/deserialization (serde)
- IR validation logic
- Type safety

### 1.2 Configuration Types (`api/config/`)

**Components**:

- Strategy config loading from JSON/MongoDB
- Parameter update types
- Validation types
- Persistence types

### 1.3 Context Types (`api/context/`)

**Components**:

- Market state (current prices, orderbook)
- Indicator values (cached latest values)
- Strategy state (internal state management)
- Execution context (what's available during evaluation)

### 1.4 Manager Interfaces (`api/manager/`)

**Components**:

- Strategy lifecycle management
- Registration interfaces
- Update interfaces
- Query interfaces

## Phase 2: Runner Module (Execution)

### 2.1 IR Interpreter (`runner/interpreter/`)

**Components**:

- IR loading and validation
- Expression evaluation engine
- Condition checking
- Action generation
- State management

**Responsibilities**:

- Load strategy IRs
- Evaluate expressions (arithmetic, comparisons, conditionals)
- Check trigger conditions
- Generate order actions
- Manage strategy state

### 2.2 Event Loop (`runner/event_loop/`)

**Components**:

- Indicator update handler
- Market data update handler
- Timer-based triggers
- Event routing

**Responsibilities**:

- Subscribe to indicator updates
- Subscribe to market data (if needed)
- Route events to appropriate strategies
- Handle timer-based triggers

### 2.3 Runtime State (`runner/runtime/`)

**Components**:

- Strategy registry (active strategies)
- Parameter cache (current values)
- Indicator value cache (latest values)
- State storage

**Responsibilities**:

- Track active strategies
- Cache parameter values for fast access
- Cache indicator values
- Manage strategy lifecycle state

### 2.4 Executor (`runner/executor/`)

**Components**:

- Coordination between interpreter, event loop, runtime
- Order generation
- Error handling
- Lifecycle management

**Responsibilities**:

- Coordinate all runner components
- Generate orders from actions
- Handle execution errors
- Manage strategy lifecycle

## Phase 3: Python DSL Compiler (Python Workspace)

**Location**: `python/strategy_dsl/` (Python workspace, separate from Rust code)

### 3.1 Python DSL Implementation

**Components**:

- DSL definitions and syntax
- Strategy definition classes
- Decorators and helpers

### 3.2 Python → IR Compiler

**Components**:

- AST parsing and analysis
- IR generation from Python DSL
- Translation validation
- Error reporting

### 3.3 Rust-Side Support (`python_core/` in Rust)

**Location**: `src/strategy_engine/src/python_core/` (Rust code)

**Components**:

- `python_dsl/`: Rust types representing Python DSL constructs (for validation)
- `translation/`: IR validation utilities (Rust-side)
- `integration/`: Integration helpers (if needed)

**Note**: The actual Python compiler is in `python/strategy_dsl/`. This Rust module provides validation and type support for the translation process.

## Phase 4: Integration

### 4.1 Indicator Engine Integration

- Subscribe to indicator updates via channels
- Cache indicator values
- Handle indicator readiness

### 4.2 Trading Engine Integration

- Generate orders via channels (same as native strategies)
- Respect order validation rules
- Handle order execution results

### 4.3 Live Config Integration

- Runtime parameter updates via live_config API
- Parameter validation
- Audit logging
- Persistence to MongoDB

### 4.4 Strategy Loading

- Load strategies from `config/strategies/` at startup
- Load strategies from MongoDB (optional)
- Validate all strategies on load
- Register with Runner

## Execution Model Comparison

| Aspect | IR Interpreter | Native Rust |
|--------|---------------|-------------|
| Performance | Acceptable (~10-50μs per evaluation) | Maximum (~1-5μs per evaluation) |
| Flexibility | High (updateable via IR) | Lower (requires recompile) |
| Development | Python DSL → IR | Direct Rust coding |
| Use Case | Most strategies | High-frequency strategies |
| Runtime Updates | Parameters only | Requires redeploy |

## Error Handling

### Load-Time Errors

- Invalid IR structure → Error log, skip strategy
- Missing dependencies → Error log, strategy not ready
- Validation failures → Detailed error messages

### Runtime Errors

- Evaluation errors → Log error, don't generate order
- Missing indicator values → Skip evaluation, wait for values
- Parameter validation → Reject invalid updates

## Testing Strategy

### Unit Tests

- IR validation (`api/ir.rs`)
- Expression evaluation (`runner/interpreter/`)
- Event routing (`runner/event_loop/`)
- State management (`runner/runtime/`)

### Integration Tests

- Full strategy execution flow
- Indicator update handling
- Order generation
- Parameter updates

### End-to-End Tests

- Load strategy IR
- Subscribe to indicators
- Receive updates
- Generate orders
- Update parameters

## Performance Considerations

### IR Interpretation Overhead

- Expression evaluation: ~10-50μs per evaluation
- Acceptable for most trading strategies
- Can optimize hot paths with caching
- Consider JIT compilation for frequently evaluated expressions (future)

### Memory Usage

- IR structures are relatively small (few KB per strategy)
- Indicator value cache is bounded
- Parameter cache is minimal

### Scalability

- Each strategy is independent
- Can scale horizontally with multiple Runner instances
- Event loop is efficient (O(1) indicator lookup)

## Future Enhancements

1. **Strategy Backtesting**: Test strategies on historical data
2. **Strategy Optimization**: Auto-tune parameters
3. **Strategy Composition**: Strategy of strategies
4. **Hot Reloading**: Update strategy structure at runtime (complex)
5. **JIT Compilation**: Compile hot IR paths to native code
6. **Strategy Marketplace**: Share and distribute strategies
7. **Visual Strategy Builder**: GUI for building strategies
8. **Binary IR Format**: Optional binary serialization for faster loading

## Migration Path

### Existing Rust Strategies

1. Keep existing Rust strategies as-is
2. New strategies can use IR interpreter
3. Gradually migrate if desired
4. Both models coexist

### Gradual Adoption

- Start with simple strategies in IR
- Migrate complex strategies as IR matures
- Keep native Rust for performance-critical strategies

## Dependencies

### Rust Dependencies

- `indicator_engine`: Indicator updates and values
- `trading_core`: Channels, types, order generation
- `live_config`: Runtime parameter updates (optional)
- `serde_json`: IR serialization/deserialization
- `mongodb`: Strategy persistence (optional)

### Python Dependencies (Python Workspace)

**Location**: `python/strategy_dsl/pyproject.toml`

**Dependencies**:
- `pydantic>=2.0.0`: Type validation
- `pydantic-settings>=2.0.0`: Settings management
- `ast-decompiler>=0.7.0`: Python AST manipulation

**Management**:
```bash
cd python
uv add pydantic ast-decompiler  # Add dependencies
uv sync                          # Install dependencies
```

**Workspace Structure**:
```
python/
├── pyproject.toml          # Workspace root
├── uv.lock                # Lock file
├── indicator_config_generator/  # Package 1
└── strategy_dsl/              # Package 2 (this one)
```

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| IR interpretation too slow | High | Benchmark early, optimize hot paths, consider JIT |
| IR format changes break strategies | High | Version IR format, migration path |
| Parameter updates cause inconsistencies | Medium | Validate updates, atomic updates |
| Complex strategies hard to express in IR | Medium | Extend IR format incrementally |
| Python → IR translation errors | Low | Comprehensive validation, tests |

## Success Criteria

1. ✅ Python DSL is intuitive and readable
2. ✅ IR interpreter executes strategies correctly
3. ✅ Performance is acceptable for trading strategies (< 100μs per evaluation)
4. ✅ Runtime parameter updates work seamlessly
5. ✅ Strategies integrate with indicator engine correctly
6. ✅ Error handling is robust and informative
7. ✅ Both IR and native Rust strategies work side-by-side

---

**Next Steps**: 
1. ✅ Python workspace structure created (`python/` with `uv`)
2. Implement API module (IR definitions)
3. Implement Runner module (interpreter, event loop)
4. Create Python DSL and compiler in `python/strategy_dsl/`
5. Implement Rust-side `python_core/` module (validation helpers)
6. Integrate with indicator_engine and trading_engine
7. Add runtime parameter update support

**Note**: The Python workspace uses `uv` for dependency management (similar to Cargo). The Python DSL compiler lives in `python/strategy_dsl/`, while Rust-side validation helpers are in `src/strategy_engine/src/python_core/`. See `python/README.md` and `python/ARCHITECTURE.md` for details.

