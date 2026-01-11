# Strategy DSL

Python DSL compiler for defining trading strategies that compile to Strategy IR.

## Overview

This package contains the Python DSL and compiler that translates Python strategy definitions into Strategy IR (JSON format) that the Rust strategy_engine can execute.

## Architecture

```
Python Strategy DSL (this package)
    ↓ [Python compiler]
Strategy IR (JSON)
    ↓ [loaded by Rust]
strategy_engine (Rust) - executes IR at runtime
```

## Relationship with Rust strategy_engine

The Rust `strategy_engine` has a `python_core` module, but it's **Rust code** that provides:
- IR validation helpers (Rust-side)
- Type definitions for translation results
- Validation utilities

**The actual Python → IR compiler lives here** in `python/strategy_dsl/`.

## Package Structure

```
strategy_dsl/
├── pyproject.toml          # Package dependencies
├── src/
│   ├── __init__.py
│   ├── dsl.py             # DSL definitions
│   ├── compiler.py        # Python → IR compiler
│   ├── validators.py      # Validation logic
│   └── cli.py             # CLI interface
└── README.md              # This file
```

## Usage

```bash
# Compile strategy to IR
uv run strategy-compile --input strategy.py --output ../../config/strategies/strategy.json

# Validate strategy
uv run strategy-validate --input strategy.py
```

## Dependencies

See `pyproject.toml` for dependencies. Key dependencies:
- `pydantic`: Type validation
- `ast-decompiler`: Python AST manipulation for compilation

