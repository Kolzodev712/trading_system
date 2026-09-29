#!/usr/bin/env bash
# Increment 10: Candidate #2 → Bybit demo. Entries stay paused until paused-path
# checks pass, then enable explicitly. Does not promote to mainnet.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKDIR="${INC10_WORKDIR:-/tmp/inc10-bybit-demo}"
SOURCE_BOT="${INC10_BOT:-$ROOT/config/bot.json}"
LOG="${WORKDIR}/runner.log"
JOURNAL="${WORKDIR}/submission-journal.jsonl"
LEDGER="${WORKDIR}/execution-ledger.jsonl"
TOKEN_FILE="${WORKDIR}/operator.token"

export TRADING_STRATEGY="${TRADING_STRATEGY:-BTCPriceMALongFlatV1}"
export TRADING_SYMBOL="${TRADING_SYMBOL:-BTC/USDT}"
export TRADING_GRPC="${TRADING_GRPC:-http://127.0.0.1:50051}"
export TRADING_OPERATOR_ID="${TRADING_OPERATOR_ID:-inc10}"
export TRACE_JAEGER_DISABLED="${TRACE_JAEGER_DISABLED:-1}"

mkdir -p "${WORKDIR}"
rm -f "${WORKDIR}"/runner*.log "${JOURNAL}" "${LEDGER}"
rm -f "${ROOT}/config/cex/runtime-snapshot.json"

if [[ ! -f "${SOURCE_BOT}" ]]; then
  echo "FAILED: missing ${SOURCE_BOT} (gitignored demo bot with demo:true keys)" >&2
  exit 1
fi

python3 - "${SOURCE_BOT}" "${WORKDIR}/bot.json" "${JOURNAL}" "${LEDGER}" <<'PY'
import json, sys
src, dest, journal, ledger = sys.argv[1:5]
bot = json.loads(open(src).read())
cfg = bot.setdefault("exchange_engine", {}).setdefault("config", {})
if cfg.get("demo") is not True:
    raise SystemExit("FAILED: source bot must set exchange_engine.config.demo=true")
if cfg.get("testnet") is True:
    raise SystemExit("FAILED: source bot must not set testnet when demo=true")
bot["execution_mode"] = "exchange"
bot["submission_journal_path"] = journal
bot["execution_ledger_path"] = ledger
bot.setdefault("mongo", {})["bot_id"] = "inc10"
bot.setdefault("reconciliation", {})
bot["reconciliation"]["exchange_level_interval_ms"] = 5000
bot["reconciliation"]["trading_level_interval_ms"] = 5000
bot["reconciliation"]["enabled"] = True
open(dest, "w").write(json.dumps(bot, indent=2) + "\n")
print("inc10 bot written (credentials not printed); demo=true futures BTC/USDT")
PY

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
      "price_history_size": 30,
      "min_price_history": 28,
      "min_order_size": "0.001",
      "step_size": "0.001",
      "tick_size": "0.1",
      "min_notional": "5.0",
      "lookback": 28,
      "max_entry_age_secs": 86400,
      "max_market_data_age_ms": 15000,
      "initially_enabled": true
    }
  ],
  "risk_limits": {
    "total_max_position": "0.002",
    "max_total_orders": 2,
    "max_notional": "200",
    "max_order_qty": "0.002"
  }
}
EOF

if [[ -z "${TRADING_OPERATOR_TOKEN:-}" ]]; then
  TRADING_OPERATOR_TOKEN="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
  export TRADING_OPERATOR_TOKEN
fi
printf '%s\n' "${TRADING_OPERATOR_TOKEN}" > "${TOKEN_FILE}"
chmod 600 "${TOKEN_FILE}"

CTL=(cargo run -q -p cli --bin grpc_ctl --)
CTL_UNAUTH=(env -u TRADING_OPERATOR_TOKEN cargo run -q -p cli --bin grpc_ctl --)

kill_runner() {
  if [[ -n "${RUNNER_PID:-}" ]]; then
    local stack pid children=""
    stack="${RUNNER_PID}"
    while [[ -n "${stack}" ]]; do
      set -- ${stack}
      pid="$1"
      shift || true
      stack="${*:-}"
      for c in $(pgrep -P "${pid}" 2>/dev/null || true); do
        stack="${stack} ${c}"
      done
      children="${children} ${pid}"
    done
    # shellcheck disable=SC2086
    kill -KILL ${children} 2>/dev/null || true
    wait "${RUNNER_PID}" 2>/dev/null || true
  fi
  RUNNER_PID=""
  pkill -KILL -f '/btc_price_ma_app ' 2>/dev/null || true
  if command -v fuser >/dev/null 2>&1; then
    fuser -k 50051/tcp 2>/dev/null || true
  fi
  local _i
  for _i in $(seq 1 20); do
    if ! fuser 50051/tcp >/dev/null 2>&1; then
      break
    fi
    fuser -k 50051/tcp 2>/dev/null || true
    sleep 0.25
  done
}

start_runner() {
  local log="$1"
  TRADING_OPERATOR_TOKEN="${TRADING_OPERATOR_TOKEN}" \
    TRADING_OPERATOR_ID="${TRADING_OPERATOR_ID}" \
    cargo run -p btc_price_ma_app --quiet -- \
      --mode strategies \
      --config "${WORKDIR}/bot.json" \
      --strategies-config "${WORKDIR}/strategies.json" \
    >"${log}" 2>&1 &
  RUNNER_PID=$!
  local ready=0
  for _ in $(seq 1 120); do
    if grep -q "gRPC server started" "${log}"; then
      ready=1
      break
    fi
    if grep -qE "startup set_position_mode failed|Bybit MAINNET|panic" "${log}"; then
      break
    fi
    sleep 0.5
  done
  if [[ "${ready}" -ne 1 ]]; then
    echo "FAILED: runner did not start gRPC (${log})" >&2
    tail -n 80 "${log}" >&2
    exit 1
  fi
}

require_status() {
  local pattern="$1"
  local status="$2"
  if ! echo "${status}" | grep -qE "${pattern}"; then
    echo "FAILED: status missing /${pattern}/" >&2
    echo "${status}" >&2
    exit 1
  fi
}

cleanup() {
  kill_runner || true
}
trap cleanup EXIT

echo "== package fixtures =="
(cd "${ROOT}" && cargo test -q -p btc_price_ma_app --lib app_factory_builds_shadow_paper_and_bybit_exchange)
(cd "${ROOT}" && cargo test -q -p runner --lib bybit_exchange_allows_demo_rejects_mainnet)
(cd "${ROOT}" && cargo test -q -p gateway --lib grpc::access)

echo "== build composition + grpc_ctl =="
(cd "${ROOT}" && cargo build -q -p btc_price_ma_app -p cli --bin grpc_ctl)

if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  sleep 1
fi

echo "== start C2 on Bybit demo with entries paused =="
start_runner "${LOG}"
sleep 2
status="$("${CTL[@]}" status)"
echo "${status}" | tee "${WORKDIR}/grpc_status_paused.txt"
require_status 'venue=BYBIT' "${status}"
require_status 'environment=DEMO' "${status}"
require_status 'execution_mode=exchange' "${status}"
require_status 'entries_paused=true' "${status}"
require_status "operator_id=${TRADING_OPERATOR_ID}" "${status}"
if grep -qE 'Bybit demo mode: placing order via REST' "${LOG}"; then
  echo "FAILED: runner placed while entries should be paused" >&2
  grep -E 'placing order' "${LOG}" >&2
  exit 1
fi
grep -E 'Bybit position mode|one-way|REST|private|gRPC server|entries start paused' "${LOG}" | tail -n 30 \
  | tee "${WORKDIR}/connectivity_snippets.txt" || true

echo "== unauthorized mutating RPC is denied =="
set +e
unauth_out="$("${CTL_UNAUTH[@]}" pause inc10-unauth 2>&1)"
unauth_rc=$?
set -e
echo "${unauth_out}" | tee "${WORKDIR}/unauth_pause.txt"
if echo "${unauth_out}" | grep -q 'accepted=true'; then
  echo "FAILED: pause succeeded without operator token" >&2
  exit 1
fi
if [[ "${unauth_rc}" -eq 0 ]] && ! echo "${unauth_out}" | grep -qiE 'unauthenticated|requires TRADING_OPERATOR|x-operator-token'; then
  echo "FAILED: unauthorized pause did not report auth failure" >&2
  exit 1
fi
if ! echo "${unauth_out}" | grep -qiE 'unauthenticated|requires TRADING_OPERATOR|x-operator-token'; then
  echo "FAILED: unauthorized pause did not report auth failure" >&2
  exit 1
fi

echo "== authorized PauseEntries while already paused =="
pause_out="$("${CTL[@]}" pause inc10-pause-check)"
echo "${pause_out}"
require_status 'accepted=true' "${pause_out}"
status="$("${CTL[@]}" status)"
require_status 'entries_paused=true' "${status}"

echo "== runner restart while demo account is retained (still paused) =="
kill_runner
start_runner "${WORKDIR}/runner-restart.log"
sleep 3
status="$("${CTL[@]}" status | tee "${WORKDIR}/grpc_status_restart.txt")"
require_status 'venue=BYBIT' "${status}"
require_status 'environment=DEMO' "${status}"
require_status 'entries_paused=true' "${status}"
if grep -qE 'Bybit demo mode: placing order via REST' "${WORKDIR}/runner-restart.log"; then
  echo "FAILED: relaunched paused runner placed an order" >&2
  exit 1
fi

echo "${status}"
echo "inc10 paused-path checks passed (connectivity, auth, restart)."
echo "workdir=${WORKDIR}"

if [[ "${INC10_ENABLE:-0}" != "1" ]]; then
  echo "INC10_ENABLE is not 1: leaving entries paused. Re-run with INC10_ENABLE=1 after UI review to start the controlled demo run."
  trap - EXIT
  echo "INC10_KEEP runner pid=${RUNNER_PID}"
  exit 0
fi

echo "== enable trading + Resume for controlled demo run =="
enable_out="$("${CTL[@]}" enable "${TRADING_STRATEGY}")"
echo "${enable_out}"
resume_out="$("${CTL[@]}" resume inc10-resume-1)"
echo "${resume_out}"
status="$("${CTL[@]}" status | tee "${WORKDIR}/grpc_status_enabled.txt")"
require_status 'entries_paused=false' "${status}"
require_status 'global_trading=true' "${status}"

echo "== measure for ${INC10_MEASURE_SECS:-45}s =="
sleep "${INC10_MEASURE_SECS:-45}"
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status_after_measure.txt"
if [[ -f "${LEDGER}" ]]; then
  echo "ledger:"; wc -l "${LEDGER}"; tail -n 20 "${LEDGER}" || true
fi
if [[ -f "${JOURNAL}" ]]; then
  echo "journal:"; wc -l "${JOURNAL}"; tail -n 20 "${JOURNAL}" || true
fi
grep -E 'placing order|order.create|Known place|unknown|fill|execution' "${WORKDIR}/runner-restart.log" "${LOG}" \
  | tail -n 40 | tee "${WORKDIR}/execution_snippets.txt" || true

echo "inc10 Stage D demo path asserted (controlled enable). Review evidence in ${WORKDIR}."
trap - EXIT
echo "INC10_KEEP runner pid=${RUNNER_PID}"
