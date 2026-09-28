#!/usr/bin/env bash
# Focused Increment 6 Spot runner → k-paper lost-ACK + restart check.
# Paper stays alive. Runner is killed while the place ACK is unknown, then relaunched.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KPAPER="${K_PAPER_DIR:-/home/kolzo-dev/Desktop/k-paper}"
PORT="${PAPER_EXCHANGE_PORT:-19997}"
HTTP="http://127.0.0.1:${PORT}"
CID="${TRADING_TEST_CLIENT_ORDER_ID:-inc6-lost-ack-1}"
WORKDIR="${INC6_WORKDIR:-/tmp/inc6-lost-ack}"
JOURNAL="${WORKDIR}/submission-journal.jsonl"
LOG1="${WORKDIR}/runner-1.log"
LOG2="${WORKDIR}/runner-2.log"
PAPER_LOG="${WORKDIR}/k-paper.log"

mkdir -p "${WORKDIR}"
rm -f "${JOURNAL}" "${LOG1}" "${LOG2}" "${PAPER_LOG}"
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
  "server": { "bind_address": "0.0.0.0", "port": 18080 },
  "redis": { "url": "", "enabled": false },
  "features": { "enable_reconciliation": true },
  "reconciliation": {
    "exchange_level_interval_ms": 180000,
    "trading_level_interval_ms": 180000,
    "enabled": true
  },
  "mongo": { "connection_string": "mongodb://localhost:27017", "database": "trading_system", "bot_id": "inc6", "enabled": true },
  "submission_journal_path": "${JOURNAL}"
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
      "buy_threshold_percent": "0.01",
      "sell_threshold_percent": "50",
      "stop_loss_percent": "50.0",
      "take_profit_percent": "50.0",
      "max_active_orders": 2,
      "price_history_size": 100,
      "min_price_history": 1,
      "min_order_size": "0.00001",
      "step_size": "0.00001",
      "tick_size": "0.01",
      "min_notional": "5.0",
      "initially_enabled": true
    }
  ]
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
echo "arm next-place lost ACK"
curl -sf -X POST "${HTTP}/test/suppress_place_ack" -H 'content-type: application/json' -d '{}' >/dev/null

echo "starting runner 1"
(
  cd "${ROOT}"
  TRADING_TEST_CLIENT_ORDER_ID="${CID}" \
  TRADING_TEST_FORCE_PLACE=1 \
  TRADING_TEST_LIMIT_PRICE=5000 \
  TRADING_TEST_ORDER_QTY=0.002 \
  cargo run -q -p runner --bin runner -- \
    --mode strategies \
    --config "${WORKDIR}/bot.json" \
    --strategies-config "${WORKDIR}/strategies.json"
) >"${LOG1}" 2>&1 &
RUNNER_PID=$!

ready=0
for _ in $(seq 1 90); do
  if grep -q "gRPC server started" "${LOG1}"; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready}" -ne 1 ]]; then
  echo "FAILED: runner 1 did not start gRPC"
  tail -n 80 "${LOG1}" || true
  exit 1
fi
(cd "${ROOT}" && TRADING_STRATEGY=BuyLowSellHigh TRADING_SYMBOL=BTC/USDT \
  cargo run -q -p cli --bin grpc_ctl -- enable BuyLowSellHigh BTC/USDT)

placed=0
for _ in $(seq 1 60); do
  orders="$(curl -sf "${HTTP}/open_orders" || echo '[]')"
  count="$(python3 -c "import json,sys; print(len(json.load(sys.stdin)))" <<<"${orders}")"
  if [[ "${count}" -ge 1 ]] && [[ -f "${JOURNAL}" ]] && grep -q "${CID}" "${JOURNAL}"; then
    placed=1
    break
  fi
  sleep 0.5
done

echo "paper open_orders after place:"
curl -sf "${HTTP}/open_orders" || true
echo
echo "journal after place:"
cat "${JOURNAL}" || true

if [[ "${placed}" -ne 1 ]]; then
  echo "FAILED: order was not accepted on Paper while ACK unknown"
  echo "----- runner-1 log (tail) -----"
  tail -n 80 "${LOG1}" || true
  exit 1
fi

echo "killing runner 1 (Paper stays up)"
kill -KILL "${RUNNER_PID}"
wait "${RUNNER_PID}" 2>/dev/null || true
RUNNER_PID=""
sleep 1

count_before="$(curl -sf "${HTTP}/open_orders" | python3 -c "import json,sys; print(len(json.load(sys.stdin)))")"
echo "Paper order count while runner is down: ${count_before}"
if [[ "${count_before}" -ne 1 ]]; then
  echo "FAILED: expected exactly 1 Paper order while runner is down"
  exit 1
fi

echo "starting runner 2 (no force-place, trading stays off)"
(
  cd "${ROOT}"
  cargo run -q -p runner --bin runner -- \
    --mode strategies \
    --config "${WORKDIR}/bot.json" \
    --strategies-config "${WORKDIR}/strategies.json"
) >"${LOG2}" 2>&1 &
RUNNER_PID=$!

recovered=0
for _ in $(seq 1 60); do
  if grep -q "submission recovery finished" "${LOG2}"; then
    recovered=1
    break
  fi
  sleep 0.5
done

echo "----- recovery log -----"
grep -E "submission recovery|imported_working|Unresolved|halt" "${LOG2}" || true
echo "grpc_ctl status after restart:"
(cd "${ROOT}" && TRADING_STRATEGY=BuyLowSellHigh TRADING_SYMBOL=BTC/USDT \
  cargo run -q -p cli --bin grpc_ctl -- status) || true
echo "journal after restart:"
cat "${JOURNAL}" || true
echo "paper open_orders after restart:"
curl -sf "${HTTP}/open_orders" || true
echo
echo "paper by client id:"
curl -sf "${HTTP}/orders/by_client_id/${CID}" || true
echo

count_after="$(curl -sf "${HTTP}/open_orders" | python3 -c "import json,sys; print(len(json.load(sys.stdin)))")"
if [[ "${recovered}" -ne 1 ]]; then
  echo "FAILED: runner 2 did not finish submission recovery"
  tail -n 80 "${LOG2}" || true
  exit 1
fi
if [[ "${count_after}" -ne 1 ]]; then
  echo "FAILED: Paper order count after restart is ${count_after}, expected 1 (no replacement)"
  exit 1
fi
if ! grep -q '"state":"acknowledged"' "${JOURNAL}"; then
  echo "FAILED: journal did not record acknowledged after recovery"
  exit 1
fi
if ! grep -q "imported_working" "${LOG2}"; then
  echo "FAILED: recovery did not report imported_working"
  exit 1
fi

echo "PASS: lost-ACK restart imported the same Paper order; no replacement"
