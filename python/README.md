# Python Workspace

Python tooling for the trading system, managed using `uv` (fast Python package manager).

## Structure

This workspace contains Python packages for:

- **indicator_config_generator**: Generate indicator configurations from Python DSL
- **strategy_dsl**: Python DSL for defining trading strategies

## Prerequisites

Install `uv` (fast Python package manager):
```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

Or via pip:
```bash
pip install uv
```

## Setup

```bash
cd python

# Install workspace (root + indicator_config_generator member)
uv sync
```

If `uv run indicator-validate` is not found, sync again after pulling latest `pyproject.toml`, or run:

```bash
uv run --package indicator-config-generator indicator-validate --help
```

## Usage

Run from the **`python/`** directory; use `../config/...` for repo-root config paths.

### Indicator Config Generator

```bash
# Generate indicators (+ Mongo if config/bot.json has mongo settings)
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json

# Validate config
uv run indicator-validate --config ../config/indicators.json
```

### Strategy DSL

```bash
# Compile strategy to IR
uv run strategy-compile --input strategy.py --output ../config/strategies/strategy.json

# Validate strategy
uv run strategy-validate --input strategy.py
```

## Dependency Management

Dependencies are managed in `pyproject.toml` files:
- Root `pyproject.toml`: Workspace configuration and shared dependencies
- Package-specific `pyproject.toml`: Package dependencies

To add a dependency:

```bash
# Add to a specific package
cd indicator_config_generator
uv add package-name

# Or add to root (shared dependency)
cd ..
uv add package-name
```

## Version Control

- `uv.lock` - Lock file (commit to git, like Cargo.lock)
- `pyproject.toml` - Dependency specifications (like Cargo.toml)

## Benefits Over requirements.txt

1. **Workspace support**: Multiple packages in one workspace
2. **Lock file**: Reproducible builds (`uv.lock`)
3. **Fast**: `uv` is written in Rust, very fast
4. **Modern**: Uses PEP 621 standard (pyproject.toml)
5. **Centralized**: Dependencies managed in one place

See [indicator_config_generator/README.md](indicator_config_generator/README.md) for defaults, Mongo sync, and JSON schema.

