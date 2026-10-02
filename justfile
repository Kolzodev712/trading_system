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

# Fast component + parity: no processes, credentials, or required network.
test-fast:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "LAYER fast-component: RUN"
    cargo test --workspace --lib --all-features
    cargo test -p strategy_core -p research_engine -p research_cli -p trading_engine -p runner -p btc_price_ma_app --all-targets
    cargo test --test integration integration::internal -- --test-threads=1
    cargo test --test integration helpers::external -- --test-threads=1
    python3 -m unittest discover -s research/tests -q
    python3 scripts/research_artifacts.py check-index --repo .
    python3 scripts/research_git_guard.py --repo .
    echo "LAYER fast-component: PASS"
    echo "LAYER paper-process: NOT_RUN"
    echo "LAYER ui-envoy: NOT_RUN"
    echo "LAYER bybit-demo-readonly: NOT_RUN"
    echo "LAYER bybit-demo-mutating: NOT_RUN"

test-integration:
    # Component integration + non-ignored external probes. Ignored live/API cases are NOT_RUN.
    #!/usr/bin/env bash
    set -euo pipefail
    echo "LAYER component-integration: RUN"
    cargo test --test integration -- --test-threads=1
    echo "LAYER component-integration: PASS"
    echo "LAYER external-ignored: NOT_RUN (use cargo test --test integration -- --ignored)"
    echo "LAYER paper-process: NOT_RUN"

test-all:
    # Run all tests (unit + integration). Does not run ignored external or Paper process workflows.
    cargo test --all-features -- --test-threads=1

test-bin:
    # Retired: test_runner never installed a no-order guard.
    cargo run --quiet -p runner --bin test_runner

test-paper:
    # Isolated Paper process workflows. Fails if k-paper or Mongo is missing.
    cargo run -p workflow_harness --bin workflow_gate -- --layer paper

test-paper-negative:
    # Deliberate mismatch. The scenario must FAIL; this recipe exits 0 when it does.
    cargo run -p workflow_harness --bin workflow_gate -- --layer paper --only negative_control_mismatch --expect-process-failure

test-paper-ui:
    cargo run -p workflow_harness --bin workflow_gate -- --layer ui

test-demo-readonly:
    cargo run -p workflow_harness --bin workflow_gate -- --layer demo-readonly

test-demo-mutating:
    # Opt-in only. Requires WORKFLOW_DEMO_MUTATE=1, BYBIT/DEMO, linear/BTCUSDT,
    # 300s deadline, WORKFLOW_DEMO_BOT, TRADING_OPERATOR_ID/TOKEN, and 0.001/150 caps.
    # Do not run until the operator has reviewed the printed procedure.
    cargo run -p workflow_harness --bin workflow_gate -- --layer demo-mutating

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

# Component/parity CI. Does not run Paper process workflows or Bybit Demo.
ci:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "LAYER gate-5r: RUN"
    just gate-5r
    echo "LAYER gate-5r: PASS"
    echo "LAYER venue-imports: RUN"
    just check-venue-imports
    echo "LAYER venue-imports: PASS"
    echo "LAYER check: RUN"
    just check
    echo "LAYER check: PASS"
    just test-integration
    echo "LAYER paper-process: NOT_RUN (use just ci-local)"
    echo "LAYER ui-envoy: NOT_RUN"
    echo "LAYER bybit-demo-readonly: NOT_RUN"
    echo "LAYER bybit-demo-mutating: NOT_RUN"
    echo "VERDICT: component CI only — not Paper/Demo/live qualification"

# Mandatory local technical qualification: fast tests + Paper process workflows.
ci-local:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "LAYER fast-component: RUN"
    just test-fast
    echo "LAYER paper-process: RUN"
    WORKFLOW_REQUIRE_CLEAN=1 just test-paper
    echo "LAYER paper-process: PASS"
    echo "LAYER paper-negative: RUN"
    WORKFLOW_REQUIRE_CLEAN=1 just test-paper-negative
    echo "LAYER paper-negative: PASS"
    echo "LAYER ui-envoy: RUN"
    WORKFLOW_REQUIRE_CLEAN=1 just test-paper-ui
    echo "LAYER ui-envoy: PASS"
    echo "LAYER bybit-demo-readonly: NOT_RUN (use just test-demo-readonly)"
    echo "LAYER bybit-demo-mutating: NOT_RUN (explicit just test-demo-mutating only)"
    echo "VERDICT: local Paper technical qualification — not Bybit Demo or live qualification"

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

# Research record + local artifact store (no venue, no Demo).
research-record-test:
    python3 -m unittest discover -s research/tests -v

research-git-guard:
    python3 scripts/research_git_guard.py --repo .

research-index:
    python3 scripts/research_artifacts.py generate-index --repo .

research-index-check:
    python3 scripts/research_artifacts.py check-index --repo .

research-manifests:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=(research/candidates/*/manifest.json)
    if [ "${#files[@]}" -eq 0 ]; then
      echo "no research manifests"
      exit 0
    fi
    for path in "${files[@]}"; do
      python3 scripts/research_artifacts.py validate-manifest "$path"
    done

# Clean build artifacts
clean:
    # Remove all build artifacts
    cargo clean

clean-all:
    # Remove all build artifacts and target directories
    cargo clean
    @find . -name "target" -type d -exec rm -rf {} + 2>/dev/null || true

