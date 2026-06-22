#!/usr/bin/env bash
# Create a DeepBook V3 BalanceManager on Sui (testnet/mainnet per bot config).
#
# Prerequisites:
#   - SUI_PRIVATE_KEY set (suiprivkey or hex)
#   - Wallet funded with testnet SUI for gas
#
# Usage:
#   export SUI_PRIVATE_KEY='suiprivkey1...'
#   ./scripts/deepbook/create-balance-manager.sh
#   ./scripts/deepbook/create-balance-manager.sh config/bot.dex.local.json

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG="${1:-$ROOT/config/bot.dex.example.json}"

if [[ -z "${SUI_PRIVATE_KEY:-}" ]]; then
  echo "error: SUI_PRIVATE_KEY is not set" >&2
  echo "Generate one with: sui keytool export --key-identity <address>" >&2
  exit 1
fi

cd "$ROOT"
exec cargo run --release -p dex_engine --bin create_balance_manager -- --config "$CONFIG"
