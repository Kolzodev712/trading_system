#!/usr/bin/env bash
# Supervisor for strategies mode. Relaunches the already-built runner when
# gRPC RequestRestart writes config/cex/restart.requested. Does not rebuild.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FLAG="config/cex/restart.requested"
BIN="${RUNNER_BIN:-target/release/runner}"

if [[ ! -x "$BIN" ]]; then
  echo "missing $BIN — build once: cargo build --release -p runner" >&2
  cargo build --release -p runner
fi

while true; do
  set +e
  "$BIN" --mode strategies
  code=$?
  set -e
  if [[ -f "$FLAG" ]]; then
    rm -f "$FLAG"
    echo "restart requested — relaunching $BIN (no rebuild)"
    continue
  fi
  exit "$code"
done
