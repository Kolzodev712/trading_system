# Strategy Engine Architecture

**Version**: 0.1 (Placeholder)  
**Last Updated**: 2026-09-29  
**Status**: Proposed Python DSL → IR path. **Not** the implemented Candidate #2 architecture (`strategy_core` + `btc_price_ma_app`). See [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md).

## Overview

The Strategy Engine provides runtime execution of strategies defined via Python DSL.
Strategies are compiled to IR (Intermediate Representation) and interpreted at runtime.

**Key Features**:
- Python strategy DSL → IR translation
- IR interpreter for runtime execution
- Runtime parameter updates (structure is compile-time only)
- Support for both IR-based and native Rust strategies

## Key Principles

### Compile-Time vs Runtime

1. **Strategy Structure/Logic**: Compiled at build time. Structure is fixed.
2. **Strategy Parameters**: Updateable at runtime via live_config service.

### Strategy Execution Models

1. **IR Interpreter**: Python strategies → IR → Rust interpreter (flexible, acceptable performance)
2. **Native Rust**: Direct Rust implementation (maximum performance)

### Indicator Management

- **System-wide indicators**: Defined in config, available to all strategies
- **Strategy-specific indicators**: Defined by individual strategies, can use system indicators
- Strategies can reference both types

**Note**: Indicator generation is handled separately (see `python/indicator_config_generator/` in the Python workspace).

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Python Workspace (python/)                                  │
│  Managed with uv (like Cargo)                               │
│                                                             │
│  python/                                                    │
│  ├── strategy_dsl/         # Python → IR compiler           │
│  └── indicator_config_generator/  # Indicator generation    │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ Generate IR at build time
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Configuration Files (JSON)                                 │
│                                                             │
│  config/                                                   │
│  └── strategies/                                           │
│      └── *.json             (strategy IR definitions)      │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ Load at startup
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Strategy Engine (Rust)                                     │
│                                                             │
│  ┌────────────────────────────────────────────────────┐   │
│  │ API (api/)                                         │   │
│  │  - IR definitions                                  │   │
│  │  - Configuration types                             │   │
│  │  - Context types                                   │   │
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
│  └──────────────┘  └────────────┘  └──────────────┘        │
└─────────────────────────────────────────────────────────────┘
```

## Components

### 1. API (`api/`)

**Purpose**: Core types and building blocks

**Responsibilities**:
- Strategy IR definitions and validation
- Configuration types
- Execution context types
- Strategy management interfaces

**Sub-modules**:
- `ir.rs`: Strategy Intermediate Representation
- `config/`: Configuration management types
- `context/`: Execution context types
- `manager/`: Strategy management interfaces

### 2. Runner (`runner/`)

**Purpose**: Runtime execution layer

**Responsibilities**:
- IR interpreter (interprets StrategyIR at runtime)
- Event loop (handles indicator updates, market data)
- Runtime state management
- Order generation and execution

**Sub-modules**:
- `interpreter/`: IR interpreter implementation
- `event_loop/`: Event loop for indicator/market data updates
- `runtime/`: Runtime state management
- `executor/`: Execution engine coordination

### 3. Python Core (`python_core/`)

**Purpose**: Rust-side support for Python translation

**Responsibilities**:
- IR validation utilities (Rust-side)
- Type definitions for translation results
- Translation validation helpers

**Sub-modules**:
- `python_dsl/`: Rust types representing Python DSL constructs
- `translation/`: Translation utilities and validation (Rust-side)
- `integration/`: Integration helpers (if needed)

**Important**: 
- **Rust code**: This module (`src/strategy_engine/src/python_core/`) is Rust code providing validation helpers
- **Python code**: The actual Python → IR compiler lives in `python/strategy_dsl/` (separate Python workspace)

The Python workspace uses `uv` for dependency management. See `python/README.md` for details.

## Data Flow

### Strategy Loading Flow

```
1. System Startup
   ↓
2. Load strategy IRs from config/strategies/*.json
   ↓
3. For each strategy IR:
   a. Validate IR structure (api/)
   b. Check indicator dependencies
   c. Register with Runner (runner/)
   ↓
4. Runner subscribes to required indicators
   ↓
5. Strategy ready for execution
```

### Strategy Execution Flow

```
1. Indicator Update Received
   ↓
2. Runner event loop receives update
   ↓
3. Check which strategies depend on this indicator
   ↓
4. For each affected strategy:
   a. Evaluate strategy logic (interpreter in runner/)
   b. Check conditions
   c. Generate order (if conditions met)
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

## File Structure

```
src/strategy_engine/
├── Cargo.toml
└── src/
    ├── lib.rs                    # Public API
    ├── api/                      # Core API types
    │   ├── mod.rs
    │   ├── ir.rs                # Strategy IR definitions
    │   ├── config/              # Configuration types
    │   ├── context/             # Execution context
    │   └── manager/             # Management interfaces
    ├── runner/                   # Execution layer
    │   ├── mod.rs
    │   ├── interpreter/         # IR interpreter
    │   ├── event_loop/          # Event loop
    │   ├── runtime/             # Runtime state
    │   └── executor/            # Execution engine
    └── python_core/              # Python translation support
        ├── mod.rs
        ├── python_dsl/          # DSL type representations
        ├── translation/         # Translation utilities
        └── integration/         # Integration helpers
```

## Integration Points

### With Indicator Engine

- Runner subscribes to indicator updates via channels
- Strategies can reference system-wide or strategy-specific indicators
- Indicator generation handled separately (`python/indicator_config_generator/` in Python workspace)

### With Trading Engine

- Runner generates orders based on strategy logic
- Orders sent via channels (same as native Rust strategies)

### With Live Config Service

- Runtime parameter updates via live_config API
- Audit logging for parameter changes
- Parameter validation before update

## Execution Model Comparison

| Aspect | IR Interpreter | Native Rust |
|--------|---------------|-------------|
| Performance | Acceptable (~10-50μs) | Maximum (~1-5μs) |
| Flexibility | High (updateable via IR) | Lower (requires recompile) |
| Development | Python DSL → IR | Direct Rust coding |
| Use Case | Most strategies | High-frequency strategies |

## Testing Strategy

- Unit tests for IR validation (api/)
- Unit tests for interpreter logic (runner/)
- Integration tests for full strategy execution
- Performance benchmarks

## Future Enhancements

- Strategy backtesting framework
- Strategy performance metrics
- Strategy hot-reloading (structure updates at runtime)
- Strategy composition (strategy of strategies)
- JIT compilation of hot IR paths

---

**TODO**: Fill in detailed implementation details as components are developed.
