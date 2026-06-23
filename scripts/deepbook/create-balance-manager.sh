#!/usr/bin/env bash
# Create a DeepBook V3 BalanceManager on Sui (testnet/mainnet per bot config).
#
# Signing key is read from dex_engine.config.privateKey in the bot JSON.
# Export SUI_PRIVATE_KEY only if you use privateKeyEnv instead of inline privateKey.
#
# Usage:
#   ./scripts/deepbook/create-balance-manager.sh
#   ./scripts/deepbook/create-balance-manager.sh config/dex/bot.json
#   ./scripts/deepbook/create-balance-manager.sh --config config/dex/bot.json

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG=""

resolve_default_config() {
  local candidate
  for candidate in \
    "$ROOT/config/dex/bot.json" \
    "$ROOT/config/bot.dex.local.json" \
    "$ROOT/config/bot.dex.example.json"
  do
    if [[ -f "$candidate" ]]; then
      echo "$candidate"
      return
    fi
  done
  echo "$ROOT/config/dex/bot.json"
}

abs_config_path() {
  local path="$1"
  if [[ "$path" != /* ]]; then
    path="$ROOT/$path"
  fi
  echo "$path"
}

config_has_inline_private_key() {
  python3 - "$1" <<'PY'
import json
import sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as f:
        bot = json.load(f)
    key = bot.get("dex_engine", {}).get("config", {}).get("privateKey")
    sys.exit(0 if key else 1)
except OSError:
    sys.exit(1)
PY
}

config_private_key_env() {
  python3 - "$1" <<'PY'
import json
import sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as f:
        bot = json.load(f)
    env = bot.get("dex_engine", {}).get("config", {}).get("privateKeyEnv") or "SUI_PRIVATE_KEY"
    print(env)
except OSError:
    print("SUI_PRIVATE_KEY")
PY
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -c|--config)
      CONFIG="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,18p' "$0"
      exit 0
      ;;
    *)
      if [[ -z "$CONFIG" && -f "$1" ]]; then
        CONFIG="$1"
        shift
      else
        echo "error: unknown argument \`$1\` (try --help)" >&2
        exit 1
      fi
      ;;
  esac
done

if [[ -z "$CONFIG" ]]; then
  CONFIG="$(resolve_default_config)"
else
  CONFIG="$(abs_config_path "$CONFIG")"
fi

if [[ ! -f "$CONFIG" ]]; then
  echo "error: bot config not found: $CONFIG" >&2
  exit 1
fi

if ! config_has_inline_private_key "$CONFIG"; then
  env_name="$(config_private_key_env "$CONFIG")"
  if [[ -z "${!env_name:-}" ]]; then
    echo "error: no dex_engine.config.privateKey in $CONFIG and \$$env_name is not set" >&2
    echo "hint: add privateKey to dex_engine.config (see config/dex/bot.json) or export $env_name" >&2
    exit 1
  fi
fi

cd "$ROOT"
exec cargo run --release -p deepbook_tools --bin create_balance_manager -- --config "$CONFIG"
