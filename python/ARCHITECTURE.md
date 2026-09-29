# Python Workspace Architecture

The Python packages here generate **indicator** JSON (and a proposed strategy IR). They are **not** the Candidate #2 live path (`libs/strategy_core` + `src/btc_price_ma_app`). Current architecture: [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md).

## Overview

This workspace contains Python tooling that integrates with the Rust trading system. The Python code runs at **build time** to generate configurations that Rust loads at runtime.

## Package Structure

### 1. indicator_config_generator

**Purpose**: Generate indicator configurations from Python DSL

**Flow**:
```
Python DSL → Compiler → JSON → Rust indicator_engine loads at startup
```

**Location**: `python/indicator_config_generator/`

**Output**: `config/indicators.json` (`indicators` + `level_pricing` for level `"price": "Binding"`)

### 2. strategy_dsl

**Purpose**: Compile Python strategy DSL to Strategy IR

**Flow**:
```
Python DSL → Compiler → Strategy IR (JSON) → Rust strategy_engine executes at runtime
```

**Location**: `python/strategy_dsl/`

**Output**: `config/strategies/*.json`

## Relationship with Rust Code

### indicator_config_generator ↔ indicator_engine

- **Python side**: Generates JSON configs from Python DSL
- **Rust side**: `indicator_engine` loads JSON and registers indicators
- **No runtime Python dependency**: Rust loads JSON directly

### strategy_dsl ↔ strategy_engine

- **Python side**: Compiles Python DSL to Strategy IR (JSON)
- **Rust side**: `strategy_engine` has:
  - `api/`: IR definitions (Rust types)
  - `runner/`: IR interpreter (executes IR at runtime)
  - `python_core/`: Rust helpers for translation (validation, types)

**Important**: The Rust `strategy_engine/src/python_core/` module is **Rust code**, not Python. It provides Rust-side support for the translation process. The actual Python → IR compiler lives in `python/strategy_dsl/`.

## Build-Time vs Runtime

| Component | When it Runs | Language | Output |
|-----------|--------------|----------|--------|
| indicator_config_generator | Build time | Python | `config/indicators.json` |
| strategy_dsl | Build time | Python | `config/strategies/*.json` |
| indicator_engine | Runtime | Rust | Indicator values |
| strategy_engine | Runtime | Rust | Executes strategies |

## Dependency Management

All Python dependencies are managed via `uv` and `pyproject.toml`:

- **Workspace root** (`python/pyproject.toml`): Workspace configuration
- **Package level** (`python/*/pyproject.toml`): Package-specific dependencies

This mirrors the Rust `Cargo.toml` workspace structure.

## Workflow

### Development

```bash
# 1. Write Python DSL
vim python/strategy_dsl/src/strategies/my_strategy.py

# 2. Compile to IR
cd python
uv run strategy-compile --input strategy_dsl/src/strategies/my_strategy.py \
    --output ../config/strategies/my_strategy.json

# 3. Rust loads IR at startup
cargo run
```

### CI/CD

```bash
# Generate all configs
cd python
uv sync
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json
uv run indicator-validate --config ../config/indicators.json
uv run strategy-compile --input strategies/*.py --output-dir ../config/strategies/

# Build Rust system (loads generated configs)
cd ..
cargo build
```

## File Locations

| What | Where | Language |
|------|-------|----------|
| Indicator generation | `python/indicator_config_generator/` | Python |
| Strategy DSL compiler | `python/strategy_dsl/` | Python |
| Indicator engine | `src/indicator_engine/` | Rust |
| Strategy engine (IR interpreter) | `src/strategy_engine/` | Rust |
| Strategy engine (Python helpers) | `src/strategy_engine/src/python_core/` | Rust |

## Key Principle

**Python = Build time tooling**
**Rust = Runtime execution**

Python generates configs/IR, Rust executes them. No Python runtime in production!

