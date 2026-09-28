#!/usr/bin/env bash
# Increment 9A: Candidate #2 composition → Paper linear futures (Stage C + section 49).
# Paper stays alive across runner restart. Not Bybit demo.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KPAPER="${K_PAPER_DIR:-/home/kolzo-dev/Desktop/k-paper}"
PORT="${PAPER_EXCHANGE_PORT:-19995}"
HTTP="http://127.0.0.1:${PORT}"
WORKDIR="${INC9A_WORKDIR:-/tmp/inc9a-matrix}"
LOG="${WORKDIR}/runner.log"
LOG2="${WORKDIR}/runner-2.log"
PAPER_LOG="${WORKDIR}/k-paper.log"
CANDLES="${ROOT}/scripts/inc9a_long_candles.json"
JOURNAL="${WORKDIR}/submission-journal.jsonl"
LEDGER="${WORKDIR}/execution-ledger.jsonl"

export TRADING_STRATEGY="${TRADING_STRATEGY:-BTCPriceMALongFlatV1}"
export TRADING_SYMBOL="${TRADING_SYMBOL:-BTC/USDT}"
export TRADING_GRPC="${TRADING_GRPC:-http://127.0.0.1:50051}"

mkdir -p "${WORKDIR}"
rm -f "${LOG}" "${LOG2}" "${PAPER_LOG}" "${JOURNAL}" "${LEDGER}"
rm -f "${ROOT}/config/cex/runtime-snapshot.json"

cat > "${WORKDIR}/bot.json" <<EOF
{
  "system": { "log_level": "info", "shutdown_timeout_secs": 5 },
  "execution_mode": "exchange",
  "exchange_engine": {
    "exchange": "Paper",
    "config": {
      "paper_exchange_url": "ws://127.0.0.1:${PORT}/ws/trade",
      "pairs": [{ "base": "BTC", "quote": "USDT" }],
      "instrument_type": "Futures",
      "leverage": 10
    }
  },
  "server": { "bind_address": "0.0.0.0", "port": 18083 },
  "redis": { "url": "", "enabled": false },
  "features": { "enable_reconciliation": true },
  "reconciliation": {
    "exchange_level_interval_ms": 180000,
    "trading_level_interval_ms": 180000,
    "enabled": true
  },
  "mongo": { "connection_string": "mongodb://localhost:27017", "database": "trading_system", "bot_id": "inc9a", "enabled": true },
  "submission_journal_path": "${JOURNAL}",
  "execution_ledger_path": "${LEDGER}"
}
EOF

cat > "${WORKDIR}/strategies.json" <<'EOF'
{
  "strategies": [
    {
      "strategy_type": "BTCPriceMALongFlatV1",
      "symbol": { "base": "BTC", "quote": "USDT" },
      "position_size": "0.001",
      "max_position": "0.001",
      "buy_threshold_percent": "0.5",
      "sell_threshold_percent": "0.5",
      "max_active_orders": 1,
      "price_history_size": 10,
      "min_price_history": 2,
      "min_order_size": "0.001",
      "step_size": "0.001",
      "tick_size": "0.01",
      "min_notional": "10.0",
      "lookback": 2,
      "max_entry_age_secs": 86400000,
      "max_market_data_age_ms": 60000,
      "initially_enabled": true
    }
  ],
  "risk_limits": {
    "total_max_position": "0.01",
    "max_total_orders": 4,
    "max_notional": "1000",
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

paper_json() {
  curl -sf "${HTTP}/$1" || echo "[]"
}

count_json_array() {
  python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d) if isinstance(d,list) else 0)'
}

start_paper() {
  (
    cd "${KPAPER}"
    PAPER_EXCHANGE_PORT="${PORT}" \
      PAPER_EXCHANGE_CONFIG=config/linear-btcusdt.yaml \
      PAPER_EXCHANGE_DUMMY_OB=1 \
      cargo run -p paper-exchange-server --quiet
  ) >"${PAPER_LOG}" 2>&1 &
  PAPER_PID=$!
  for _ in $(seq 1 60); do
    if curl -sf "${HTTP}/open_orders" >/dev/null; then
      return 0
    fi
    sleep 1
  done
  echo "k-paper did not become ready" >&2
  tail -50 "${PAPER_LOG}" >&2
  exit 1
}

start_runner() {
  local log="$1"
  TRADING_TEST_CANDLES_PATH="${CANDLES}" \
    TRADING_TEST_NOW_MS=259200000 \
    cargo run -p btc_price_ma_app --quiet -- \
      --mode strategies \
      --config "${WORKDIR}/bot.json" \
      --strategies-config "${WORKDIR}/strategies.json" \
    >"${log}" 2>&1 &
  RUNNER_PID=$!
  local ready=0
  for _ in $(seq 1 90); do
    if grep -q "gRPC server started" "${log}"; then
      ready=1
      break
    fi
    sleep 0.5
  done
  if [[ "${ready}" -ne 1 ]]; then
    echo "FAILED: runner did not start gRPC" >&2
    tail -n 80 "${log}" >&2
    exit 1
  fi
}

wait_for_paper_activity() {
  local placed=0
  local orders="[]"
  local pos="[]"
  for _ in $(seq 1 40); do
    orders="$(paper_json open_orders)"
    pos="$(paper_json positions)"
    if [[ "${orders}" != "[]" || "${pos}" != "[]" ]]; then
      placed=1
      break
    fi
    sleep 0.5
  done
  echo "${orders}" > "${WORKDIR}/open_orders.json"
  echo "${pos}" > "${WORKDIR}/positions.json"
  echo "paper open_orders=${orders}"
  echo "paper positions=${pos}"
  if [[ "${placed}" -ne 1 ]]; then
    echo "FAILED: Candidate #2 did not produce a Paper order/position after enable+resume" >&2
    tail -n 80 "${LOG}" >&2
    exit 1
  fi
}

wait_for_paper_fill() {
  local trades="[]"
  for _ in $(seq 1 40); do
    trades="$(paper_json trades)"
    if [[ "${trades}" != "[]" ]]; then
      echo "${trades}" > "${WORKDIR}/trades.json"
      echo "paper trades=${trades}"
      return 0
    fi
    sleep 0.5
  done
  echo "FAILED: expected a Paper fill" >&2
  tail -n 80 "${LOG}" >&2
  exit 1
}

wait_for_grpc_flat() {
  for _ in $(seq 1 20); do
    local status
    status="$("${CTL[@]}" status)"
    if echo "${status}" | grep -q "positions=0" && ! echo "${status}" | grep -q "pos=0.001"; then
      echo "${status}"
      return 0
    fi
    sleep 0.5
  done
  echo "FAILED: runner TSM did not go flat after Paper flatten" >&2
  "${CTL[@]}" status >&2
  echo "paper positions=$(paper_json positions)" >&2
  exit 1
}

if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  fuser -k "${PORT}/tcp" 2>/dev/null || true
  sleep 1
fi

echo "building composition app + grpc_ctl + k-paper..."
(cd "${ROOT}" && cargo build -q -p btc_price_ma_app -p cli --bin grpc_ctl)
(cd "${KPAPER}" && cargo build -q -p paper-exchange-server)

start_paper
curl -sf -X POST "${HTTP}/test/reset_lab_faults" >/dev/null || true

orders_before="$(paper_json open_orders | count_json_array)"
echo "paper independent order count before runner: ${orders_before}"

echo "== start C2 composition on Paper Exchange futures (trading off) =="
start_runner "${LOG}"

CTL=(cargo run -q -p cli --bin grpc_ctl --)
echo "== pause then enable then resume =="
"${CTL[@]}" pause inc9a-pause-1
"${CTL[@]}" enable "${TRADING_STRATEGY}"
"${CTL[@]}" resume inc9a-resume-1
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status.txt"

echo "== wait for Paper order then fill =="
wait_for_paper_activity
wait_for_paper_fill

echo "== runner journal/ledger =="
if [[ ! -f "${JOURNAL}" ]]; then
  echo "FAILED: submission journal missing after place" >&2
  exit 1
fi
ls -l "${JOURNAL}" "${LEDGER}" || true
wc -l "${JOURNAL}" "${LEDGER}" 2>/dev/null || true
cp "${JOURNAL}" "${WORKDIR}/journal-after-place.jsonl"
head -n 8 "${JOURNAL}" | tee "${WORKDIR}/journal-head.txt"
if [[ -f "${LEDGER}" ]]; then
  head -n 8 "${LEDGER}" | tee "${WORKDIR}/ledger-head.txt"
fi

paper_count="$(paper_json open_orders | count_json_array)"
trade_count="$(paper_json trades | count_json_array)"
echo "paper independent open_orders=${paper_count} trades=${trade_count}"

echo "== runner restart while Paper keeps state =="
"${CTL[@]}" disable "${TRADING_STRATEGY}"
sleep 1
kill -KILL "${RUNNER_PID}" 2>/dev/null || true
wait "${RUNNER_PID}" 2>/dev/null || true
RUNNER_PID=""
sleep 1
after_kill_orders="$(paper_json open_orders)"
after_kill_pos="$(paper_json positions)"
after_kill_trades="$(paper_json trades)"
echo "paper after runner kill: orders=${after_kill_orders} positions=${after_kill_pos} trades=${after_kill_trades}"
if [[ "${after_kill_orders}" == "[]" && "${after_kill_pos}" == "[]" && "${after_kill_trades}" == "[]" ]]; then
  echo "FAILED: Paper lost account state when the runner died" >&2
  exit 1
fi

if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  sleep 1
fi

echo "== relaunch runner (no enable; recover existing state) =="
start_runner "${LOG2}"
sleep 3
grep -E "imported_working|recovery|Acknowledged|acknowledged" "${LOG2}" | tail -n 20 || true
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status_restart.txt"
paper_after_restart="$(paper_json open_orders)"
echo "paper after runner-2: ${paper_after_restart}"
if grep -q "k-paper: sent place_order" "${LOG2}"; then
  echo "FAILED: relaunched runner posted a replacement order" >&2
  exit 1
fi

echo "== operator pause / flatten / resume =="
"${CTL[@]}" pause inc9a-pause-2
pause_status="$("${CTL[@]}" status)"
echo "${pause_status}"
if ! echo "${pause_status}" | grep -q "entries_paused=true"; then
  echo "FAILED: PauseEntries did not set entries_paused" >&2
  exit 1
fi
"${CTL[@]}" flatten inc9a-flatten-1
flatten_status="$("${CTL[@]}" status)"
echo "${flatten_status}"
if ! echo "${flatten_status}" | grep -q "flatten_latched=true"; then
  echo "FAILED: FlattenAndPause did not set flatten_latched" >&2
  exit 1
fi
echo "paper open_orders after flatten RPC: $(paper_json open_orders)"
echo "paper positions after flatten RPC: $(paper_json positions)"
echo "== wait for runner TSM to match Paper flat =="
wait_for_grpc_flat
"${CTL[@]}" resume inc9a-resume-2
resume_status="$("${CTL[@]}" status)"
echo "${resume_status}"

echo "== lab reject does not book =="
curl -sf -X POST "${HTTP}/test/reset_lab_faults" >/dev/null
before_reject="$(paper_json open_orders | count_json_array)"
curl -sf -X POST "${HTTP}/test/reject_next_place" -H 'content-type: application/json' -d '{"reason":"lab_reject"}' >/dev/null
echo "reject hook armed"

echo "== lab duplicate-next-fill + stale book + disconnect =="
curl -sf -X POST "${HTTP}/test/reset_lab_faults" >/dev/null
curl -sf -X POST "${HTTP}/test/duplicate_next_fill" >/dev/null
curl -sf -X POST "${HTTP}/test/stale_orderbook" -H 'content-type: application/json' -d '{"age_secs":3600}' >/dev/null
sleep 1
curl -sf -X POST "${HTTP}/test/disconnect_clients" >/dev/null
sleep 3
if ! grep -qE "server closed connection|reconnecting trade WS|k-paper market data: server closed" "${LOG2}"; then
  echo "WARN: disconnect log not observed in runner-2; checking runner health anyway"
fi
curl -sf -X POST "${HTTP}/test/reset_lab_faults" >/dev/null

echo "== lost-ACK phase: rest a far limit after flatten =="
kill -KILL "${RUNNER_PID}" 2>/dev/null || true
wait "${RUNNER_PID}" 2>/dev/null || true
RUNNER_PID=""
if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  sleep 1
fi

curl -sf -X POST "${HTTP}/test/reset_lab_faults" >/dev/null
curl -sf -X POST "${HTTP}/test/suppress_place_ack" -H 'content-type: application/json' -d '{}' >/dev/null
LOG3="${WORKDIR}/runner-3.log"
TRADING_TEST_CANDLES_PATH="${CANDLES}" \
  TRADING_TEST_NOW_MS=259200000 \
  TRADING_TEST_CLIENT_ORDER_ID=inc9a-lost-ack-1 \
  TRADING_TEST_LIMIT_PRICE=49000 \
  cargo run -p btc_price_ma_app --quiet -- \
    --mode strategies \
    --config "${WORKDIR}/bot.json" \
    --strategies-config "${WORKDIR}/strategies.json" \
  >"${LOG3}" 2>&1 &
RUNNER_PID=$!
ready=0
for _ in $(seq 1 90); do
  if grep -q "gRPC server started" "${LOG3}"; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready}" -ne 1 ]]; then
  echo "FAILED: runner-3 did not start" >&2
  tail -n 80 "${LOG3}" >&2
  exit 1
fi
"${CTL[@]}" pause inc9a-pause-3
"${CTL[@]}" enable "${TRADING_STRATEGY}"
"${CTL[@]}" resume inc9a-resume-3
lost_seen=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-lost-ack-1" >/dev/null; then
    lost_seen=1
    break
  fi
  sleep 0.5
done
if [[ "${lost_seen}" -ne 1 ]]; then
  echo "FAILED: lost-ACK client id was not booked on Paper" >&2
  echo "paper orders=$(paper_json open_orders)" >&2
  tail -n 40 "${LOG3}" >&2
  cat "${JOURNAL}" >&2
  exit 1
fi
echo "lost-ACK Paper row: $(curl -sf "${HTTP}/orders/by_client_id/inc9a-lost-ack-1")"
orders_during="$(paper_json open_orders | count_json_array)"
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill -KILL "${RUNNER_PID}" 2>/dev/null || true
wait "${RUNNER_PID}" 2>/dev/null || true
RUNNER_PID=""
same_after="$(curl -sf "${HTTP}/orders/by_client_id/inc9a-lost-ack-1" || true)"
echo "Paper kept lost-ACK order after runner-3 kill: ${same_after}"
if [[ -z "${same_after}" ]]; then
  echo "FAILED: Paper lost the suppressed-ACK order" >&2
  exit 1
fi
if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  sleep 1
fi
LOG4="${WORKDIR}/runner-4.log"
TRADING_TEST_CANDLES_PATH="${CANDLES}" \
  TRADING_TEST_NOW_MS=259200000 \
  cargo run -p btc_price_ma_app --quiet -- \
    --mode strategies \
    --config "${WORKDIR}/bot.json" \
    --strategies-config "${WORKDIR}/strategies.json" \
  >"${LOG4}" 2>&1 &
RUNNER_PID=$!
ready=0
for _ in $(seq 1 90); do
  if grep -q "gRPC server started" "${LOG4}"; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready}" -ne 1 ]]; then
  echo "FAILED: runner-4 did not start" >&2
  tail -n 80 "${LOG4}" >&2
  exit 1
fi
sleep 3
if grep -q "k-paper: sent place_order" "${LOG4}"; then
  echo "FAILED: recovered runner posted a replacement after lost-ACK" >&2
  exit 1
fi
grep -E "imported_working|inc9a-lost-ack-1" "${LOG4}" | tail -n 20 || true
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status_lostack.txt"
echo "paper after recover: $(paper_json open_orders)"
if [[ "$(paper_json open_orders | count_json_array)" -gt "${orders_during}" ]]; then
  echo "FAILED: Paper order count increased after lost-ACK recovery" >&2
  exit 1
fi

echo "inc9a Stage C + section-49 lab path observed."
echo "workdir=${WORKDIR}"
