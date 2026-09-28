#!/usr/bin/env bash
# Increment 8 operator commands against k-paper. Not the 9A matrix.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KPAPER="${K_PAPER_DIR:-/home/kolzo-dev/Desktop/k-paper}"
PORT="${PAPER_EXCHANGE_PORT:-19996}"
HTTP="http://127.0.0.1:${PORT}"
WORKDIR="${INC8_WORKDIR:-/tmp/inc8-operator}"
LOG="${WORKDIR}/runner.log"
PAPER_LOG="${WORKDIR}/k-paper.log"

mkdir -p "${WORKDIR}"
rm -f "${LOG}" "${PAPER_LOG}"
rm -f "${ROOT}/config/cex/runtime-snapshot.json"

cat > "${WORKDIR}/bot.json" <<EOF
{
  "system": { "log_level": "info", "shutdown_timeout_secs": 5 },
  "exchange_engine": {
    "exchange": "Paper",
    "config": {
      "paper_exchange_url": "ws://127.0.0.1:${PORT}/ws/trade",
      "pairs": [{ "base": "BTC", "quote": "USDT" }],
      "instrument_type": "Spot"
    }
  },
  "server": { "bind_address": "0.0.0.0", "port": 18081 },
  "redis": { "url": "", "enabled": false },
  "features": { "enable_reconciliation": true },
  "reconciliation": {
    "exchange_level_interval_ms": 180000,
    "trading_level_interval_ms": 180000,
    "enabled": true
  },
  "mongo": { "connection_string": "mongodb://localhost:27017", "database": "trading_system", "bot_id": "inc8", "enabled": true },
  "submission_journal_path": "${WORKDIR}/submission-journal.jsonl"
}
EOF

cat > "${WORKDIR}/strategies.json" <<'EOF'
{
  "strategies": [
    {
      "strategy_type": "BuyLowSellHigh",
      "symbol": { "base": "BTC", "quote": "USDT" },
      "position_size": "0.001",
      "max_position": "0.01",
      "buy_threshold_percent": "0.5",
      "sell_threshold_percent": "0.5",
      "stop_loss_percent": "5.0",
      "take_profit_percent": "10.0",
      "max_active_orders": 2,
      "price_history_size": 100,
      "min_price_history": 10,
      "min_order_size": "0.00001",
      "step_size": "0.00001",
      "tick_size": "0.01",
      "min_notional": "5.0",
      "initially_enabled": true
    }
  ],
  "risk_limits": {
    "total_max_position": "0.01",
    "max_total_orders": 4,
    "max_notional": "100",
    "max_order_qty": "0.01"
  }
}
EOF

cleanup() {
  if [[ -n "${RUNNER_PID:-}" ]] && kill -0 "${RUNNER_PID}" 2>/dev/null; then
    kill -KILL "${RUNNER_PID}" 2>/dev/null || true
  fi
  if [[ -n "${PAPER_PID:-}" ]] && kill -0 "${PAPER_PID}" 2>/dev/null; then
    kill -TERM "${PAPER_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "building runner + grpc_ctl + k-paper..."
(cd "${ROOT}" && cargo build -q -p runner --bin runner -p cli --bin grpc_ctl)
(cd "${KPAPER}" && cargo build -q --bin k-paper)

if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  fuser -k "${PORT}/tcp" 2>/dev/null || true
  sleep 1
fi

echo "starting k-paper on :${PORT}"
(cd "${KPAPER}" && PAPER_EXCHANGE_PORT="${PORT}" PAPER_EXCHANGE_PROFILE=dev \
  cargo run -q --bin k-paper) >"${PAPER_LOG}" 2>&1 &
PAPER_PID=$!

for _ in $(seq 1 40); do
  if curl -sf "${HTTP}/open_orders" >/dev/null; then
    break
  fi
  sleep 0.25
done
curl -sf "${HTTP}/open_orders" >/dev/null
echo "paper open_orders before commands: $(curl -sf "${HTTP}/open_orders")"

echo "starting runner"
(
  cd "${ROOT}"
  cargo run -q -p runner --bin runner -- \
    --mode strategies \
    --config "${WORKDIR}/bot.json" \
    --strategies-config "${WORKDIR}/strategies.json"
) >"${LOG}" 2>&1 &
RUNNER_PID=$!

ready=0
for _ in $(seq 1 90); do
  if grep -q "gRPC server started" "${LOG}"; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready}" -ne 1 ]]; then
  echo "FAILED: runner did not start gRPC"
  tail -n 80 "${LOG}" || true
  exit 1
fi

CTL=(cargo run -q -p cli --bin grpc_ctl --)
cd "${ROOT}"

echo "===== status (baseline) ====="
"${CTL[@]}" status
echo "===== pause ====="
"${CTL[@]}" pause inc8-pause-1
pause_status="$("${CTL[@]}" status)"
echo "===== status after pause ====="
echo "${pause_status}"
if ! echo "${pause_status}" | grep -q "entries_paused=true"; then
  echo "FAILED: PauseEntries did not set entries_paused"
  exit 1
fi
echo "===== flatten ====="
"${CTL[@]}" flatten inc8-flatten-1
flatten_status="$("${CTL[@]}" status)"
echo "===== status after flatten ====="
echo "${flatten_status}"
if ! echo "${flatten_status}" | grep -q "flatten_latched=true"; then
  echo "FAILED: FlattenAndPause did not set flatten_latched"
  exit 1
fi
echo "paper open_orders after flatten: $(curl -sf "${HTTP}/open_orders")"
echo "===== resume ====="
"${CTL[@]}" resume inc8-resume-1
resume_status="$("${CTL[@]}" status)"
echo "===== status after resume ====="
echo "${resume_status}"

if ! echo "${resume_status}" | grep -q "flatten_latched=false"; then
  echo "FAILED: expected flatten latch cleared after Resume"
  exit 1
fi
if ! echo "${resume_status}" | grep -q "entries_paused=false"; then
  echo "FAILED: expected entries_paused=false after Resume"
  exit 1
fi

echo "PASS: PauseEntries / FlattenAndPause / Resume against Paper; open_orders inspected"
