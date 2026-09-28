# Justfile for trading_system
# Run `just` to see all available commands

# Default recipe: show all available commands
default:
    @just --list

# Code formatting
fmt:
    # Format all Rust code
    cargo fmt --all

fmt-check:
    # Check if code is formatted (for CI)
    cargo fmt --all -- --check

# Linting
clippy:
    # Run clippy with all targets and features
    cargo clippy --all-targets --all-features

clippy-fix:
    # Run clippy and automatically apply fixes
    cargo clippy --fix --allow-dirty --all-targets --all-features

clippy-strict:
    # Run clippy with warnings as errors (for CI)
    cargo clippy --all-targets --all-features -- -D warnings

# Security audit
audit:
    # Run cargo audit to check for security vulnerabilities
    cargo audit

# Testing
test:
    # Workspace lib tests (root `cargo test --lib` is the empty trading_system package)
    cargo test --workspace --lib --all-features
    # 5R packages include integration/bin tests (frozen parity lives under tests/)
    cargo test -p strategy_core -p research_engine -p research_cli -p trading_engine -p runner -p btc_price_ma_app --all-targets

# Fail if live crates depend on research/model crates or the Candidate #2 app.
test-5r-graph:
    #!/usr/bin/env bash
    set -euo pipefail
    forbidden='(strategy_core|research_engine|research_cli|btc_price_ma_app) v'
    for pkg in trading_engine runner; do
      extra=()
      if [ "$pkg" = runner ]; then extra=(--features redis); fi
      tree=$(cargo tree -p "$pkg" -e normal,build,dev --all-features "${extra[@]}")
      if echo "$tree" | grep -E "$forbidden" >/dev/null; then
        echo "forbidden crate in $pkg tree"
        echo "$tree" | grep -E "$forbidden" || true
        exit 1
      fi
    done
    app=$(cargo tree -p btc_price_ma_app -e normal,build,dev --all-features)
    echo "$app" | grep -E 'strategy_core v' >/dev/null
    echo "$app" | grep -E 'runner v' >/dev/null

test-integration:
    # Run integration tests (excluding ignored)
    cargo test --test integration -- --test-threads=1

test-all:
    # Run all tests (unit + integration)
    cargo test --all-features -- --test-threads=1

test-bin:
    # Run test binary in levels mode for 60 seconds
    cargo run --bin test_runner -- --mode levels --duration 60

# Compilation checks
check:
    # Check every workspace member, including btc_price_ma_app
    cargo check --workspace --all-targets --all-features

build:
    # Build all packages in debug mode
    cargo build --all-targets --all-features

build-release:
    # Build all packages in release mode
    cargo build --release --all-targets --all-features

build-runner:
    # Build runner package in release mode
    cargo build --release --package runner

build-cli:
    # Build CLI binary in release mode
    cargo build --release --package cli --bin trading-cli

# Indicator config (pairs must match config/indicators.json)
indicator-pairs := "BTCUSDT,ETHUSDT"

generate-indicators:
    # Regenerate config/indicators.json from default templates (no Mongo)
    cd python && uv sync --quiet && uv run indicator-generate --pairs {{indicator-pairs}} --output ../config/indicators.json --no-mongo

indicator-validate:
    cd python && uv sync --quiet && uv run indicator-validate --config ../config/indicators.json

indicator-config-check: generate-indicators
    # Fail if committed indicators.json drifts from generator output
    git diff --exit-code config/indicators.json

indicator-test:
    cd python && uv sync --quiet && uv run --with pytest pytest indicator_config_generator/tests -q

# Venue import hygiene (no traits shim / legacy exchange channel paths in src/)
check-venue-imports:
    bash .cursor/tools/check_venue_imports.sh

# 5R focused gate (GitHub Actions is not used). Run this before merge.
gate-5r:
    cargo fmt --all -- --check
    cargo test -p strategy_core -p research_engine -p research_cli -p trading_engine -p runner -p btc_price_ma_app --all-targets
    cargo build -p btc_price_ma_app --bin btc_price_ma_app
    just test-5r-graph
    cargo clippy -p trading_engine -p runner -p btc_price_ma_app --all-targets --no-deps -- -D warnings

# Full CI check (local; GitHub Actions is disabled)
ci:
    just gate-5r
    just check-venue-imports
    just check
    just test-integration

# Quick check (format, clippy, test)
quick:
    # Quick local checks before committing
    just fmt-check
    just clippy
    just test

# Run trading system (mode picks config/cex/ or config/dex/ automatically)
run mode="levels":
    cargo run --release -p runner -- --mode {{mode}}

run-dex:
    just run dex

run-levels:
    just run levels

run-strategies:
    just run strategies

run-dex-strategies:
    just run dex-strategies

# DEX setup — deposit wallet funds into BalanceManager (uses config/dex/bot.json)
# Usage: just dex-deposit --sui 2 --dbusdc 50
dex-deposit +ARGS:
    ./scripts/deepbook/deposit.sh {{ARGS}}

# DEX setup — create BalanceManager (once)
dex-create-balance-manager:
    ./scripts/deepbook/create-balance-manager.sh

run-cli:
    cargo run --release -p cli --bin trading-cli

cli:
    just run-cli

# Clean build artifacts
clean:
    # Remove all build artifacts
    cargo clean

clean-all:
    # Remove all build artifacts and target directories
    cargo clean
    @find . -name "target" -type d -exec rm -rf {} + 2>/dev/null || true

