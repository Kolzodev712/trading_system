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
PAPER_LOG="${WORKDIR}/k-paper.log"
CANDLES="${ROOT}/scripts/inc9a_long_candles.json"
JOURNAL="${WORKDIR}/submission-journal.jsonl"
LEDGER="${WORKDIR}/execution-ledger.jsonl"

export TRADING_STRATEGY="${TRADING_STRATEGY:-BTCPriceMALongFlatV1}"
export TRADING_SYMBOL="${TRADING_SYMBOL:-BTC/USDT}"
export TRADING_GRPC="${TRADING_GRPC:-http://127.0.0.1:50051}"
export TRACE_JAEGER_DISABLED="${TRACE_JAEGER_DISABLED:-1}"

mkdir -p "${WORKDIR}"
rm -f "${WORKDIR}"/runner*.log "${PAPER_LOG}" "${JOURNAL}" "${LEDGER}"
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
    "exchange_level_interval_ms": 1000,
    "trading_level_interval_ms": 1000,
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
      "max_market_data_age_ms": 5000,
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
  if [[ "${INC9A_KEEP:-}" == "1" ]]; then
    echo "INC9A_KEEP=1: leaving Paper pid=${PAPER_PID:-} runner pid=${RUNNER_PID:-}"
    return 0
  fi
  if declare -F kill_runner >/dev/null; then
    kill_runner
  fi
  if [[ -n "${PAPER_PID:-}" ]]; then
    local stack pid children=""
    stack="${PAPER_PID}"
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
    kill -TERM ${children} 2>/dev/null || true
  fi
  PAPER_PID=""
  pkill -TERM -f '/paper-exchange-server' 2>/dev/null || true
}
trap cleanup EXIT

paper_get() {
  local path="$1"
  local out
  if ! out="$(curl -sfS "${HTTP}/${path}")"; then
    echo "FAILED: Paper GET /${path}" >&2
    exit 1
  fi
  printf '%s' "${out}"
}

paper_post() {
  local path="$1"
  local body="${2:-}"
  if [[ -n "${body}" ]]; then
    curl -sfS -X POST "${HTTP}/${path}" -H 'content-type: application/json' -d "${body}" >/dev/null \
      || { echo "FAILED: Paper POST /${path}" >&2; exit 1; }
  else
    curl -sfS -X POST "${HTTP}/${path}" >/dev/null \
      || { echo "FAILED: Paper POST /${path}" >&2; exit 1; }
  fi
}

count_json_array() {
  python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d) if isinstance(d,list) else 0)'
}

paper_working_remaining() {
  python3 -c 'import json,sys
orders=json.load(sys.stdin)
rem=0.0
for o in orders:
    amt=float(o.get("amount") or o.get("quantity") or 0)
    filled=float(o.get("filledAmount") or o.get("filled_qty") or 0)
    rem += max(amt-filled, 0.0)
print(f"{rem:.6f}")'
}

journal_states() {
  python3 - <<'PY' "${JOURNAL}"
import json,sys
path=sys.argv[1]
states=[]
try:
    with open(path) as f:
        for line in f:
            line=line.strip()
            if not line:
                continue
            states.append(json.loads(line).get("state",""))
except FileNotFoundError:
    pass
print(" ".join(states))
PY
}

require_journal_state() {
  local want="$1"
  local states
  states="$(journal_states)"
  echo "journal states: ${states}"
  if ! echo " ${states} " | grep -q " ${want} "; then
    echo "FAILED: journal missing state ${want}" >&2
    cat "${JOURNAL}" >&2 || true
    exit 1
  fi
}

ledger_fill_ids() {
  python3 - <<'PY' "${LEDGER}"
import json,sys
path=sys.argv[1]
ids=[]
try:
    with open(path) as f:
        for line in f:
            line=line.strip()
            if not line:
                continue
            rec=json.loads(line)
            if rec.get("kind")=="fill" and rec.get("execution_id"):
                ids.append(rec["execution_id"])
except FileNotFoundError:
    pass
print(len(ids), len(set(ids)))
print(" ".join(ids))
PY
}

require_ledger_fills() {
  local min_unique="$1"
  local summary
  summary="$(ledger_fill_ids)"
  echo "ledger fills: ${summary}"
  local unique
  unique="$(echo "${summary}" | awk 'NR==1{print $2}')"
  if [[ -z "${unique}" || "${unique}" -lt "${min_unique}" ]]; then
    echo "FAILED: expected at least ${min_unique} unique ledger fills" >&2
    cat "${LEDGER}" >&2 || true
    exit 1
  fi
}

current_log() {
  echo "${CURRENT_LOG:-${LOG}}"
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

kill_runner() {
  # cargo run is not the binary; SIGKILL on the cargo pid leaves btc_price_ma_app
  # bound to Paper and 50051 (two runners consume one-shot lab faults).
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

compact_resolved_journal() {
  python3 - <<'PY' "${JOURNAL}"
import json,sys
path=sys.argv[1]
keep=[]
try:
    with open(path) as f:
        for line in f:
            line=line.strip()
            if not line:
                continue
            rec=json.loads(line)
            if rec.get("state") in ("acknowledged","rejected","terminal"):
                keep.append(line)
except FileNotFoundError:
    pass
with open(path,"w") as f:
    for line in keep:
        f.write(line+"\n")
PY
}

start_runner() {
  local log="$1"
  CURRENT_LOG="${log}"
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
    echo "FAILED: runner did not start gRPC (${log})" >&2
    tail -n 80 "${log}" >&2
    exit 1
  fi
}

wait_for_paper_activity() {
  local placed=0
  local orders="[]"
  local pos="[]"
  for _ in $(seq 1 40); do
    orders="$(paper_get open_orders)"
    pos="$(paper_get positions)"
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
    echo "FAILED: Candidate #2 did not produce a Paper order/position" >&2
    tail -n 80 "$(current_log)" >&2
    exit 1
  fi
}

wait_for_paper_fill() {
  local prev_trades prev_pos
  prev_trades="$(paper_get trades | count_json_array)"
  prev_pos="$(paper_get positions | count_json_array)"
  local trades="[]"
  local pos="[]"
  local i
  local forced=0
  for i in $(seq 1 60); do
    trades="$(paper_get trades)"
    pos="$(paper_get positions)"
    local n tpos
    n="$(echo "${trades}" | count_json_array)"
    tpos="$(echo "${pos}" | count_json_array)"
    if [[ "${n}" -gt "${prev_trades}" || "${tpos}" -gt "${prev_pos}" ]]; then
      echo "${trades}" > "${WORKDIR}/trades.json"
      echo "paper trades=${trades}"
      echo "paper positions=${pos}"
      return 0
    fi
    if [[ "${i}" -ge 8 && "${forced}" -eq 0 && "$(paper_get open_orders)" != "[]" ]]; then
      forced=1
      echo "resting Paper order still unfilled; forcing one lab_fill_open"
      curl -sfS -X POST "${HTTP}/test/lab_fill_open" \
        -H "content-type: application/json" \
        -d '{"qty":"0.001"}' >/dev/null || true
    fi
    sleep 0.5
  done
  echo "FAILED: expected a new Paper fill (trades=${prev_trades} pos=${prev_pos})" >&2
  echo "paper orders=$(paper_get open_orders)" >&2
  echo "paper positions=$(paper_get positions)" >&2
  echo "paper trades=$(paper_get trades)" >&2
  tail -n 80 "$(current_log)" >&2
  exit 1
}

freeze_entries() {
  local tag="$1"
  "${CTL[@]}" pause "inc9a-pause-freeze-${tag}" || true
  "${CTL[@]}" disable "${TRADING_STRATEGY}" || true
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
  echo "paper positions=$(paper_get positions)" >&2
  exit 1
}

enable_entries() {
  local tag="$1"
  "${CTL[@]}" pause "inc9a-pause-${tag}"
  "${CTL[@]}" enable "${TRADING_STRATEGY}"
  "${CTL[@]}" resume "inc9a-resume-${tag}"
}

flatten_to_flat() {
  local tag="$1"
  "${CTL[@]}" pause "inc9a-pause-flat-${tag}" || true
  "${CTL[@]}" flatten "inc9a-flatten-${tag}" || true
  wait_for_grpc_flat
  paper_post test/reset_lab_faults
}

operator_flatten_states() {
  python3 - <<'PY' "${JOURNAL}"
import json, sys
path = sys.argv[1]
latest = {}
try:
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            if rec.get("strategy_id") == "operator_flatten":
                latest[rec["client_order_id"]] = rec.get("state")
except FileNotFoundError:
    pass
print(json.dumps(latest, sort_keys=True))
PY
}

require_log() {
  local pattern="$1"
  local file
  file="$(current_log)"
  if ! grep -qE "${pattern}" "${file}"; then
    echo "FAILED: log ${file} missing /${pattern}/" >&2
    tail -n 80 "${file}" >&2
    exit 1
  fi
}

echo "== package / bounded fixtures =="
(cd "${ROOT}" && cargo test -q -p btc_price_ma_app --lib frozen_n28_runtime_transitions_match_offline -- --nocapture)
(cd "${ROOT}" && cargo test -q -p market_data_engine --lib delta_merges_only_when_sequence_continues)
(cd "${ROOT}" && cargo test -q -p trading_engine --lib aged_timestamp_is_stale)
(cd "${ROOT}" && cargo test -q -p trading_engine --lib timeout_place_error_stays_unknown)
(cd "${ROOT}" && cargo test -q -p trading_engine --lib rest_position_overwrites_lying_ws_update)
(cd "${ROOT}" && cargo test -q -p trading_engine --lib record_trade_uses_trade_id)
(cd "${ROOT}" && cargo test -q -p exchange_engine --lib duplicate_trade_id_does_not_consume_remainder)
(cd "${ROOT}" && cargo test -q -p exchange_engine --lib cancel_all_is_unverified_when_rest_fails)
(cd "${ROOT}" && cargo test -q -p trading_core --lib cancel_working_fails_when_venue_rest_is_unverified)

if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  fuser -k "${PORT}/tcp" 2>/dev/null || true
  sleep 1
fi

echo "building composition app + grpc_ctl + k-paper..."
(cd "${ROOT}" && cargo build -q -p btc_price_ma_app -p cli --bin grpc_ctl)
(cd "${KPAPER}" && cargo test -q -p paper-exchange-core --lib)
(cd "${KPAPER}" && cargo build -q -p paper-exchange-server)

start_paper
paper_post test/reset_lab_faults

orders_before="$(paper_get open_orders | count_json_array)"
echo "paper independent order count before runner: ${orders_before}"

echo "== start C2 composition on Paper Exchange futures (trading off) =="
start_runner "${LOG}"

CTL=(cargo run -q -p cli --bin grpc_ctl --)
echo "== pause then enable then resume =="
enable_entries 1
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status.txt"

echo "== wait for Paper order then fill =="
wait_for_paper_fill

if [[ ! -f "${JOURNAL}" ]]; then
  echo "FAILED: submission journal missing after place" >&2
  exit 1
fi
require_journal_state acknowledged
require_ledger_fills 1
cp "${JOURNAL}" "${WORKDIR}/journal-after-place.jsonl"
cp "${LEDGER}" "${WORKDIR}/ledger-after-place.jsonl"
paper_count="$(paper_get open_orders | count_json_array)"
trade_count="$(paper_get trades | count_json_array)"
echo "paper independent open_orders=${paper_count} trades=${trade_count}"
if [[ "${trade_count}" -lt 1 ]]; then
  echo "FAILED: Paper REST trades empty after fill" >&2
  exit 1
fi

echo "== runner restart while Paper keeps state =="
"${CTL[@]}" disable "${TRADING_STRATEGY}"
sleep 1
kill_runner
after_kill_orders="$(paper_get open_orders)"
after_kill_pos="$(paper_get positions)"
after_kill_trades="$(paper_get trades)"
echo "paper after runner kill: orders=${after_kill_orders} positions=${after_kill_pos} trades=${after_kill_trades}"
if [[ "${after_kill_orders}" == "[]" && "${after_kill_pos}" == "[]" && "${after_kill_trades}" == "[]" ]]; then
  echo "FAILED: Paper lost account state when the runner died" >&2
  exit 1
fi

echo "== relaunch runner (no enable; recover existing state) =="
start_runner "${WORKDIR}/runner-2.log"
sleep 3
grep -E "imported_working|recovery|Acknowledged|acknowledged" "${WORKDIR}/runner-2.log" | tail -n 20 || true
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status_restart.txt"
if grep -q "k-paper: sent place_order" "${WORKDIR}/runner-2.log"; then
  echo "FAILED: relaunched runner posted a replacement order" >&2
  exit 1
fi
if ! echo "$(paper_get positions)" | grep -q "0.001"; then
  echo "FAILED: Paper position missing after runner restart" >&2
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
echo "paper open_orders after flatten RPC: $(paper_get open_orders)"
echo "paper positions after flatten RPC: $(paper_get positions)"
wait_for_grpc_flat
"${CTL[@]}" resume inc9a-resume-2
resume_status="$("${CTL[@]}" status)"
echo "${resume_status}"

echo "== two-flatten cycle: new position, second FlattenAndPause, new client id =="
first_flatten_states="$(operator_flatten_states)"
echo "operator_flatten after first flatten: ${first_flatten_states}"
python3 - <<'PY' "${first_flatten_states}"
import json, sys
states = json.loads(sys.argv[1])
if not states:
    raise SystemExit("FAILED: no operator_flatten journal row after first flatten")
print("first flatten client ids:", ",".join(sorted(states)))
PY
enable_entries twoflatten
wait_for_paper_fill
second_pos_before="$(paper_get positions)"
echo "paper positions before second flatten: ${second_pos_before}"
if ! echo "${second_pos_before}" | grep -q "0.001"; then
  echo "FAILED: second cycle did not acquire a Paper position" >&2
  echo "${second_pos_before}" >&2
  exit 1
fi
"${CTL[@]}" flatten inc9a-flatten-2
wait_for_grpc_flat
second_flatten_states="$(operator_flatten_states)"
echo "operator_flatten after second flatten: ${second_flatten_states}"
python3 - <<'PY' "${first_flatten_states}" "${second_flatten_states}" "${JOURNAL}" "${LEDGER}"
import json, sys
first = json.loads(sys.argv[1])
second = json.loads(sys.argv[2])
new = [cid for cid in second if cid not in first]
if len(new) != 1:
    raise SystemExit(f"FAILED: expected one new flatten client id, first={first} second={second} new={new}")
for cid, state in first.items():
    if second.get(cid) != "terminal":
        raise SystemExit(f"FAILED: first flatten {cid} did not become terminal (was {state}, now {second.get(cid)})")
if second[new[0]] not in ("acknowledged", "submitted", "terminal"):
    raise SystemExit(f"FAILED: second flatten {new[0]} unexpected state {second[new[0]]}")
print("second flatten client id:", new[0], "state:", second[new[0]])
print("journal bytes", __import__("os").path.getsize(sys.argv[3]))
print("ledger bytes", __import__("os").path.getsize(sys.argv[4]))
PY
echo "paper open_orders after second flatten: $(paper_get open_orders)"
echo "paper positions after second flatten: $(paper_get positions)"
second_latch="$("${CTL[@]}" status)"
echo "${second_latch}"
if ! echo "${second_latch}" | grep -q "flatten_latched=true"; then
  echo "FAILED: second FlattenAndPause did not keep flatten_latched" >&2
  exit 1
fi
"${CTL[@]}" resume inc9a-resume-two-flatten
resume_two="$("${CTL[@]}" status)"
echo "${resume_two}"

echo "== explicit reject releases reservation =="
flatten_to_flat reject
paper_post test/reset_lab_faults
before_reject_orders="$(paper_get open_orders | count_json_array)"
before_reject_pos="$(paper_get positions | count_json_array)"
paper_post test/reject_next_place '{"reason":"lab_reject"}'
enable_entries reject
reject_seen=0
for _ in $(seq 1 30); do
  if grep -qE "Known place rejection|lab_reject" "$(current_log)"; then
    reject_seen=1
    break
  fi
  sleep 0.5
done
if [[ "${reject_seen}" -ne 1 ]]; then
  echo "FAILED: known reject was not observed on the runner" >&2
  tail -n 80 "$(current_log)" >&2
  exit 1
fi
require_journal_state rejected
after_reject_orders="$(paper_get open_orders | count_json_array)"
if [[ "${after_reject_orders}" -lt "${before_reject_orders}" ]]; then
  echo "FAILED: reject unexpectedly removed existing Paper orders" >&2
  exit 1
fi
echo "reject: paper orders ${before_reject_orders} -> ${after_reject_orders} positions_before=${before_reject_pos}"

echo "== stale book blocks exposure increase =="
flatten_to_flat stale
paper_post test/reset_lab_faults
paper_post test/stale_orderbook '{"age_secs":3600}'
enable_entries stale
stale_seen=0
for _ in $(seq 1 20); do
  if grep -q "Stale market data blocks exposure increase" "$(current_log)"; then
    stale_seen=1
    break
  fi
  sleep 0.5
done
if [[ "${stale_seen}" -ne 1 ]]; then
  echo "FAILED: stale MD did not block a new increase" >&2
  tail -n 80 "$(current_log)" >&2
  exit 1
fi
if [[ "$(paper_get open_orders)" != "[]" || "$(paper_get positions)" != "[]" ]]; then
  echo "FAILED: stale book allowed a Paper order or position" >&2
  echo "orders=$(paper_get open_orders) pos=$(paper_get positions)" >&2
  exit 1
fi
paper_post test/reset_lab_faults

echo "== duplicate fill is ledger-idempotent =="
flatten_to_flat dupfill
sleep 1
fills_before="$(ledger_fill_ids | awk 'NR==1{print $2}')"
paper_post test/reset_lab_faults
paper_post test/duplicate_next_fill
enable_entries dupfill
wait_for_paper_fill
freeze_entries dupfill
sleep 1
fills_after="$(ledger_fill_ids | awk 'NR==1{print $2}')"
echo "unique ledger fills before=${fills_before} after=${fills_after}"
if [[ "${fills_after}" -ne $((fills_before + 1)) ]]; then
  echo "FAILED: duplicate WS fill changed unique ledger count by $((fills_after - fills_before))" >&2
  ledger_fill_ids >&2
  exit 1
fi
if ! echo "$(paper_get positions)" | grep -q "0.001"; then
  echo "FAILED: duplicate fill changed Paper position" >&2
  echo "positions=$(paper_get positions)" >&2
  exit 1
fi
dup_status="$("${CTL[@]}" status)"
echo "${dup_status}"
if ! echo "${dup_status}" | grep -q "pos=0.001"; then
  echo "FAILED: grpc position after duplicate fill is not 0.001" >&2
  exit 1
fi
if echo "${dup_status}" | grep -q "pos=0.002"; then
  echo "FAILED: duplicate fill double-counted grpc position" >&2
  exit 1
fi

echo "== position-before-fill event order =="
flatten_to_flat pbf
sleep 1
fills_before="$(ledger_fill_ids | awk 'NR==1{print $2}')"
paper_post test/reset_lab_faults
paper_post test/position_before_fill
enable_entries pbf
wait_for_paper_fill
freeze_entries pbf
sleep 1
fills_after="$(ledger_fill_ids | awk 'NR==1{print $2}')"
echo "unique ledger fills before=${fills_before} after=${fills_after}"
if [[ "${fills_after}" -lt $((fills_before + 1)) ]]; then
  echo "FAILED: position-before-fill did not record a new ledger fill" >&2
  exit 1
fi
if [[ "${fills_after}" -gt $((fills_before + 1)) ]]; then
  echo "FAILED: position-before-fill double-counted ledger fills" >&2
  ledger_fill_ids >&2
  exit 1
fi
pbf_status="$("${CTL[@]}" status)"
if ! echo "${pbf_status}" | grep -q "pos=0.001"; then
  echo "FAILED: position-before-fill grpc qty" >&2
  echo "${pbf_status}" >&2
  exit 1
fi

echo "== private/public disconnect then reconnect =="
paper_post test/reset_lab_faults
paper_post test/disconnect_clients
disc_seen=0
for _ in $(seq 1 20); do
  if grep -qE "server closed connection|reconnecting trade WS|k-paper market data: server closed" "$(current_log)"; then
    disc_seen=1
    break
  fi
  sleep 0.5
done
if [[ "${disc_seen}" -ne 1 ]]; then
  echo "FAILED: disconnect/reconnect was not logged" >&2
  tail -n 80 "$(current_log)" >&2
  exit 1
fi
sleep 3
require_log "reconnecting trade WS"
if ! echo "$(paper_get positions)" | grep -q "0.001"; then
  echo "FAILED: Paper position lost across disconnect" >&2
  exit 1
fi
re_status="$("${CTL[@]}" status)"
echo "${re_status}"
if ! echo "${re_status}" | grep -q "pos=0.001"; then
  echo "FAILED: grpc position not reconciled after reconnect" >&2
  exit 1
fi

echo "== REST/WS disagreement: lying WS, REST unchanged, blocked until restore =="
enable_entries disagreement
paper_rest_before="$(paper_get positions)"
lie_orders_before="$(paper_get open_orders | count_json_array)"
lie_trades_before="$(paper_get trades | count_json_array)"
log_before="$(wc -l < "$(current_log)")"
paper_post test/emit_ws_position '{"quantity":"1","entry_price":"50000"}'
lie_seen=0
for _ in $(seq 1 20); do
  st="$("${CTL[@]}" status)"
  if echo "${st}" | grep -q "pos=1"; then
    lie_seen=1
    echo "${st}"
    break
  fi
  sleep 0.3
done
if [[ "${lie_seen}" -ne 1 ]]; then
  echo "FAILED: lying WS position did not reach grpc/TSM" >&2
  "${CTL[@]}" status >&2
  exit 1
fi
paper_rest_after="$(paper_get positions)"
if [[ "${paper_rest_after}" != "${paper_rest_before}" ]]; then
  echo "FAILED: emit_ws_position mutated Paper REST truth" >&2
  echo "before=${paper_rest_before} after=${paper_rest_after}" >&2
  exit 1
fi
sleep 2
if [[ "$(paper_get open_orders | count_json_array)" -gt "${lie_orders_before}" ]]; then
  echo "FAILED: new Paper orders during REST/WS disagreement" >&2
  echo "orders=$(paper_get open_orders)" >&2
  exit 1
fi
if [[ "$(paper_get trades | count_json_array)" -gt "${lie_trades_before}" ]]; then
  echo "FAILED: new Paper trades during REST/WS disagreement" >&2
  echo "trades=$(paper_get trades)" >&2
  exit 1
fi
require_log "REST/WS position disagreement"
if tail -n "+$((log_before + 1))" "$(current_log)" | grep -q "k-paper: sent place_order"; then
  echo "FAILED: runner placed during REST/WS disagreement" >&2
  tail -n "+$((log_before + 1))" "$(current_log)" >&2
  exit 1
fi
echo "disagreement: Paper REST unchanged, no new exposure; runner not Ready to increase"
paper_post test/disconnect_clients
restored=0
for _ in $(seq 1 30); do
  st="$("${CTL[@]}" status)"
  if echo "${st}" | grep -q "pos=0.001" && ! echo "${st}" | grep -q "pos=1"; then
    restored=1
    echo "${st}"
    break
  fi
  sleep 0.5
done
if [[ "${restored}" -ne 1 ]]; then
  echo "FAILED: reconnect snapshot did not restore REST qty on grpc" >&2
  "${CTL[@]}" status >&2
  echo "paper REST=$(paper_get positions)" >&2
  exit 1
fi

echo "== partial fill + duplicate fill together, then CancelWorking =="
flatten_to_flat partial
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
paper_post test/reset_lab_faults
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-partial-1
export TRADING_TEST_LIMIT_PRICE=49000
export TRADING_TEST_ORDER_QTY=0.002
start_runner "${WORKDIR}/runner-partial.log"
enable_entries partial
resting=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-partial-1" >/dev/null; then
    resting=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID TRADING_TEST_LIMIT_PRICE TRADING_TEST_ORDER_QTY
if [[ "${resting}" -ne 1 ]]; then
  echo "FAILED: partial-fill resting order was not booked" >&2
  tail -n 40 "${WORKDIR}/runner-partial.log" >&2
  exit 1
fi
fills_before="$(ledger_fill_ids | awk 'NR==1{print $2}')"
paper_post test/duplicate_next_fill
fill_res=$(curl -sfS -X POST "${HTTP}/test/lab_fill_open" \
  -H "content-type: application/json" \
  -d '{"qty":"0.001"}') \
  || { echo "FAILED: lab_fill_open" >&2; exit 1; }
echo "lab_fill_open=${fill_res}"
if ! echo "${fill_res}" | grep -q "PartiallyFilled"; then
  echo "FAILED: expected PartiallyFilled after lab fill" >&2
  exit 1
fi
sleep 1
require_journal_state acknowledged
fills_after="$(ledger_fill_ids | awk 'NR==1{print $2}')"
echo "unique ledger fills after partial+dup=${fills_after} (before=${fills_before})"
if [[ "${fills_after}" -ne $((fills_before + 1)) ]]; then
  echo "FAILED: duplicate partial fill changed unique ledger count by $((fills_after - fills_before))" >&2
  ledger_fill_ids >&2
  exit 1
fi
if ! echo "$(paper_get positions)" | grep -q "0.001"; then
  echo "FAILED: partial fill did not open 0.001 position" >&2
  echo "pos=$(paper_get positions)" >&2
  exit 1
fi
remain="$(paper_get open_orders | paper_working_remaining)"
echo "working remaining after duplicate partial=${remain}"
if [[ "${remain}" != "0.001000" ]]; then
  echo "FAILED: duplicate partial fill erased the working remainder" >&2
  echo "orders=$(paper_get open_orders)" >&2
  "${CTL[@]}" status >&2
  exit 1
fi
dup_partial_status="$("${CTL[@]}" status)"
echo "${dup_partial_status}"
if echo "${dup_partial_status}" | grep -q "orders=0"; then
  echo "FAILED: adapter dropped the working remainder after duplicate partial" >&2
  exit 1
fi
if echo "${dup_partial_status}" | grep -q "pos=0.002"; then
  echo "FAILED: duplicate partial fill double-counted grpc position" >&2
  exit 1
fi
cancel_out="$("${CTL[@]}" cancel inc9a-cancel-1)"
echo "${cancel_out}"
if ! echo "${cancel_out}" | grep -q "accepted=true"; then
  echo "FAILED: CancelWorking RPC was not accepted after duplicate partial" >&2
  exit 1
fi
cancelled=0
for _ in $(seq 1 20); do
  if [[ "$(paper_get open_orders)" == "[]" ]]; then
    cancelled=1
    break
  fi
  sleep 0.5
done
if [[ "${cancelled}" -ne 1 ]]; then
  echo "FAILED: Paper still has working orders after CancelWorking" >&2
  echo "orders=$(paper_get open_orders)" >&2
  exit 1
fi
if ! echo "$(paper_get positions)" | grep -q "0.001"; then
  echo "FAILED: CancelWorking changed the filled position" >&2
  exit 1
fi
cw_status="$("${CTL[@]}" status)"
echo "${cw_status}"
if ! echo "${cw_status}" | grep -q "orders=0"; then
  echo "FAILED: grpc still shows working orders after CancelWorking" >&2
  exit 1
fi
if echo "${cw_status}" | grep -q "pos=0.002"; then
  echo "FAILED: partial+cancel double-counted exposure" >&2
  exit 1
fi

echo "== remaining fill after partial =="
flatten_to_flat remaining
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
paper_post test/reset_lab_faults
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-remain-1
export TRADING_TEST_LIMIT_PRICE=49000
export TRADING_TEST_ORDER_QTY=0.002
start_runner "${WORKDIR}/runner-remain.log"
enable_entries remaining
resting=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-remain-1" >/dev/null; then
    resting=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID TRADING_TEST_LIMIT_PRICE TRADING_TEST_ORDER_QTY
if [[ "${resting}" -ne 1 ]]; then
  echo "FAILED: remaining-fill resting order was not booked" >&2
  tail -n 40 "${WORKDIR}/runner-remain.log" >&2
  exit 1
fi
"${CTL[@]}" pause "inc9a-pause-remain-fills"
fills_before="$(ledger_fill_ids | awk 'NR==1{print $2}')"
curl -sfS -X POST "${HTTP}/test/lab_fill_open" -H "content-type: application/json" -d '{"qty":"0.001"}' >/dev/null \
  || { echo "FAILED: first remaining lab_fill_open" >&2; exit 1; }
remain_res=$(curl -sfS -X POST "${HTTP}/test/lab_fill_open" \
  -H "content-type: application/json" \
  -d '{"qty":"0.001"}') \
  || { echo "FAILED: remaining lab_fill_open" >&2; exit 1; }
echo "remaining_fill=${remain_res}"
if ! echo "${remain_res}" | grep -q "Filled"; then
  echo "FAILED: expected Filled after remaining fill" >&2
  exit 1
fi
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
sleep 0.2
fills_after="$(ledger_fill_ids | awk 'NR==1{print $2}')"
if [[ "${fills_after}" -lt $((fills_before + 2)) ]]; then
  echo "FAILED: remaining fill unique ledger ${fills_after} (before=${fills_before})" >&2
  ledger_fill_ids >&2
  exit 1
fi
if [[ "$(paper_get open_orders)" != "[]" ]]; then
  echo "FAILED: remaining fill left a working order" >&2
  echo "orders=$(paper_get open_orders)" >&2
  exit 1
fi
if ! echo "$(paper_get positions)" | grep -q "0.002"; then
  echo "FAILED: remaining fill did not reach Paper pos 0.002" >&2
  echo "pos=$(paper_get positions)" >&2
  exit 1
fi

echo "== partial fill competing with remaining fill and CancelWorking =="
flatten_to_flat race
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
paper_post test/reset_lab_faults
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-race-1
export TRADING_TEST_LIMIT_PRICE=49000
export TRADING_TEST_ORDER_QTY=0.002
export TRADING_TEST_FORCE_PLACE=1
export TRADING_TEST_HOLD_INTENTS=1
start_runner "${WORKDIR}/runner-race.log"
unset TRADING_TEST_FORCE_PLACE TRADING_TEST_HOLD_INTENTS
enable_entries race
resting=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-race-1" >/dev/null; then
    resting=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID TRADING_TEST_LIMIT_PRICE TRADING_TEST_ORDER_QTY
if [[ "${resting}" -ne 1 ]]; then
  echo "FAILED: race resting order was not booked" >&2
  tail -n 40 "${WORKDIR}/runner-race.log" >&2
  exit 1
fi
"${CTL[@]}" pause "inc9a-pause-race-fills"
fills_before="$(ledger_fill_ids | awk 'NR==1{print $2}')"
curl -sfS -X POST "${HTTP}/test/lab_fill_open" -H "content-type: application/json" -d '{"qty":"0.001"}' >/dev/null \
  || { echo "FAILED: race partial lab_fill_open" >&2; exit 1; }
curl -sfS -X POST "${HTTP}/test/lab_fill_open" -H "content-type: application/json" -d '{"qty":"0.001"}' >/dev/null &
fill_pid=$!
cancel_out="$("${CTL[@]}" cancel inc9a-cancel-race)"
echo "${cancel_out}"
wait "${fill_pid}" || true
sleep 1
fills_after="$(ledger_fill_ids | awk 'NR==1{print $2}')"
echo "race unique ledger before=${fills_before} after=${fills_after}"
added=$((fills_after - fills_before))
if [[ "${added}" -ne 1 && "${added}" -ne 2 ]]; then
  echo "FAILED: fill/cancel race unique ledger ${fills_after} (before=${fills_before}); want +1 or +2" >&2
  ledger_fill_ids >&2
  exit 1
fi
if [[ "$(paper_get open_orders)" != "[]" ]]; then
  echo "FAILED: race left a working Paper order" >&2
  echo "orders=$(paper_get open_orders)" >&2
  exit 1
fi
race_pos="$(paper_get positions)"
echo "race Paper positions=${race_pos}"
if echo "${race_pos}" | grep -q "0.003"; then
  echo "FAILED: fill/cancel race over-filled Paper" >&2
  exit 1
fi
race_status="$("${CTL[@]}" status)"
echo "${race_status}"
if [[ "${added}" -eq 1 ]]; then
  if ! echo "${race_pos}" | grep -Eq '0\.001'; then
    echo "FAILED: one-fill race Paper qty is not 0.001" >&2
    echo "positions=${race_pos}" >&2
    exit 1
  fi
  if echo "${race_pos}" | grep -Eq '0\.002'; then
    echo "FAILED: one-fill race Paper qty looks like 0.002" >&2
    echo "positions=${race_pos}" >&2
    exit 1
  fi
  if ! echo "${race_status}" | grep -Eq 'qty=0\.001'; then
    echo "FAILED: one-fill race grpc qty is not 0.001" >&2
    echo "${race_status}" >&2
    exit 1
  fi
else
  if ! echo "${race_pos}" | grep -Eq '0\.002'; then
    echo "FAILED: two-fill race Paper qty is not 0.002" >&2
    echo "positions=${race_pos}" >&2
    exit 1
  fi
  if ! echo "${race_status}" | grep -Eq 'qty=0\.002'; then
    echo "FAILED: two-fill race grpc qty is not 0.002" >&2
    echo "${race_status}" >&2
    exit 1
  fi
fi

echo "== CancelWorking unverified when REST open_orders fails =="
flatten_to_flat restfail
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
paper_post test/reset_lab_faults
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-restfail-1
export TRADING_TEST_LIMIT_PRICE=49000
export TRADING_TEST_ORDER_QTY=0.002
start_runner "${WORKDIR}/runner-restfail.log"
enable_entries restfail
resting=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-restfail-1" >/dev/null; then
    resting=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID TRADING_TEST_LIMIT_PRICE TRADING_TEST_ORDER_QTY
if [[ "${resting}" -ne 1 ]]; then
  echo "FAILED: rest-fail resting order was not booked" >&2
  tail -n 40 "${WORKDIR}/runner-restfail.log" >&2
  exit 1
fi
paper_post test/fail_open_orders
cancel_out="$("${CTL[@]}" cancel inc9a-cancel-restfail)"
echo "${cancel_out}"
if echo "${cancel_out}" | grep -q "accepted=true"; then
  echo "FAILED: CancelWorking reported success without a REST venue check" >&2
  exit 1
fi
if ! echo "${cancel_out}" | grep -qiE "unverified|accepted=false|state=failed"; then
  echo "FAILED: CancelWorking did not report unverified/failed on REST error" >&2
  exit 1
fi
paper_post test/reset_lab_faults
if curl -sf "${HTTP}/orders/by_client_id/inc9a-restfail-1" >/dev/null; then
  cancel_out="$("${CTL[@]}" cancel inc9a-cancel-restfail-2)"
  echo "${cancel_out}"
  if ! echo "${cancel_out}" | grep -q "accepted=true"; then
    echo "FAILED: CancelWorking did not succeed after REST recovered" >&2
    exit 1
  fi
fi
cancelled=0
for _ in $(seq 1 20); do
  if [[ "$(paper_get open_orders)" == "[]" ]]; then
    cancelled=1
    break
  fi
  sleep 0.5
done
if [[ "${cancelled}" -ne 1 ]]; then
  echo "FAILED: Paper still has working orders after recovered CancelWorking" >&2
  echo "orders=$(paper_get open_orders)" >&2
  exit 1
fi

echo "== FlattenAndPause unverified when REST open_orders fails; recover after reset =="
flatten_to_flat flattenrestfail
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
paper_post test/reset_lab_faults
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-flatten-restfail-1
export TRADING_TEST_LIMIT_PRICE=49000
export TRADING_TEST_ORDER_QTY=0.002
start_runner "${WORKDIR}/runner-flatten-restfail.log"
enable_entries flattenrestfail
flatten_resting=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-flatten-restfail-1" >/dev/null; then
    flatten_resting=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID TRADING_TEST_LIMIT_PRICE TRADING_TEST_ORDER_QTY
if [[ "${flatten_resting}" -ne 1 ]]; then
  echo "FAILED: FlattenAndPause rest-fail resting order was not booked" >&2
  tail -n 40 "${WORKDIR}/runner-flatten-restfail.log" >&2
  exit 1
fi
flatten_cids_before="$(operator_flatten_states)"
paper_post test/fail_open_orders
flatten_fail_out="$("${CTL[@]}" flatten inc9a-flatten-restfail-rpc)"
echo "${flatten_fail_out}"
if echo "${flatten_fail_out}" | grep -q "accepted=true"; then
  echo "FAILED: FlattenAndPause reported success without a REST venue check" >&2
  exit 1
fi
if ! echo "${flatten_fail_out}" | grep -qiE "unverified|accepted=false|state=failed"; then
  echo "FAILED: FlattenAndPause did not report unverified/failed on REST error" >&2
  exit 1
fi
flatten_fail_status="$("${CTL[@]}" status)"
echo "${flatten_fail_status}"
if ! echo "${flatten_fail_status}" | grep -q "flatten_latched=true"; then
  echo "FAILED: FlattenAndPause REST failure released the flatten latch" >&2
  exit 1
fi
flatten_cids_after_fail="$(operator_flatten_states)"
python3 - <<'PY' "${flatten_cids_before}" "${flatten_cids_after_fail}"
import json, sys
before = json.loads(sys.argv[1])
after = json.loads(sys.argv[2])
new = [cid for cid in after if cid not in before]
if new:
    raise SystemExit(f"FAILED: FlattenAndPause minted reduce-only ids despite REST failure: {new}")
print("no new operator_flatten client id after REST failure")
PY
if ! curl -sf "${HTTP}/orders/by_client_id/inc9a-flatten-restfail-1" >/dev/null; then
  echo "FAILED: FlattenAndPause REST failure cancelled the working order without venue proof" >&2
  exit 1
fi
paper_post test/reset_lab_faults
flatten_recover_out="$("${CTL[@]}" flatten inc9a-flatten-restfail-recover)"
echo "${flatten_recover_out}"
flatten_recovered=0
for _ in $(seq 1 20); do
  if [[ "$(paper_get open_orders)" == "[]" ]]; then
    flatten_recovered=1
    break
  fi
  sleep 0.5
done
if [[ "${flatten_recovered}" -ne 1 ]]; then
  echo "FAILED: FlattenAndPause did not clear Paper working orders after REST recovered" >&2
  echo "orders=$(paper_get open_orders)" >&2
  echo "${flatten_recover_out}" >&2
  exit 1
fi
recover_status="$("${CTL[@]}" status)"
echo "${recover_status}"
if ! echo "${recover_status}" | grep -q "flatten_latched=true"; then
  echo "FAILED: recovered FlattenAndPause did not keep flatten_latched" >&2
  exit 1
fi

echo "== transient send failure keeps Unknown reservation =="
flatten_to_flat drop
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
compact_resolved_journal
paper_post test/reset_lab_faults
paper_post test/drop_next_place
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-drop-1
start_runner "${WORKDIR}/runner-drop.log"
enable_entries drop
drop_seen=0
for _ in $(seq 1 40); do
  if grep -q "open_order timeout" "${WORKDIR}/runner-drop.log"; then
    drop_seen=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID
if [[ "${drop_seen}" -ne 1 ]]; then
  echo "FAILED: drop_next_place did not produce a place timeout" >&2
  tail -n 80 "${WORKDIR}/runner-drop.log" >&2
  exit 1
fi
require_journal_state unknown
if curl -sf "${HTTP}/orders/by_client_id/inc9a-drop-1" >/dev/null; then
  echo "FAILED: dropped place was booked on Paper" >&2
  exit 1
fi
if [[ "$(paper_get open_orders)" != "[]" ]]; then
  echo "FAILED: drop_next_place booked an open order" >&2
  exit 1
fi

echo "== duplicate client id rejects the second place =="
kill_runner
compact_resolved_journal
paper_post test/reset_lab_faults
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-dup-cid
start_runner "${WORKDIR}/runner-dupcid.log"
enable_entries dupcid
wait_for_paper_fill
echo "first duplicate-cid row: $(curl -sfS "${HTTP}/orders/by_client_id/inc9a-dup-cid")"
flatten_to_flat dupcid
"${CTL[@]}" resume inc9a-resume-dupcid-2
dup_rej=0
for _ in $(seq 1 30); do
  if grep -qE "duplicate client_order_id|Known place rejection" "${WORKDIR}/runner-dupcid.log"; then
    dup_rej=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID
if [[ "${dup_rej}" -ne 1 ]]; then
  echo "FAILED: second place with the same client id was not rejected" >&2
  tail -n 80 "${WORKDIR}/runner-dupcid.log" >&2
  exit 1
fi
require_journal_state rejected
if [[ "$(paper_get open_orders | count_json_array)" -ne 0 ]]; then
  echo "FAILED: duplicate client id created extra Paper working orders" >&2
  echo "orders=$(paper_get open_orders)" >&2
  exit 1
fi

echo "== lost-ACK phase: rest a far limit after flatten =="
flatten_to_flat lostack
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
compact_resolved_journal
paper_post test/reset_lab_faults
paper_post test/suppress_place_ack "{}"
export TRADING_TEST_CLIENT_ORDER_ID=inc9a-lost-ack-1
export TRADING_TEST_LIMIT_PRICE=49000
start_runner "${WORKDIR}/runner-3.log"
enable_entries 3
lost_seen=0
for _ in $(seq 1 30); do
  if curl -sf "${HTTP}/orders/by_client_id/inc9a-lost-ack-1" >/dev/null; then
    lost_seen=1
    break
  fi
  sleep 0.5
done
unset TRADING_TEST_CLIENT_ORDER_ID TRADING_TEST_LIMIT_PRICE
if [[ "${lost_seen}" -ne 1 ]]; then
  echo "FAILED: lost-ACK client id was not booked on Paper" >&2
  echo "paper orders=$(paper_get open_orders)" >&2
  tail -n 40 "${WORKDIR}/runner-3.log" >&2
  cat "${JOURNAL}" >&2
  exit 1
fi
echo "lost-ACK Paper row: $(curl -sfS "${HTTP}/orders/by_client_id/inc9a-lost-ack-1")"
orders_during="$(paper_get open_orders | count_json_array)"
"${CTL[@]}" disable "${TRADING_STRATEGY}" || true
kill_runner
same_after="$(curl -sfS "${HTTP}/orders/by_client_id/inc9a-lost-ack-1")"
echo "Paper kept lost-ACK order after runner-3 kill: ${same_after}"
start_runner "${WORKDIR}/runner-4.log"
sleep 3
if grep -q "k-paper: sent place_order" "${WORKDIR}/runner-4.log"; then
  echo "FAILED: recovered runner posted a replacement after lost-ACK" >&2
  exit 1
fi
grep -E "imported_working|inc9a-lost-ack-1" "${WORKDIR}/runner-4.log" | tail -n 20 || true
"${CTL[@]}" status | tee "${WORKDIR}/grpc_status_lostack.txt"
if [[ "$(paper_get open_orders | count_json_array)" -gt "${orders_during}" ]]; then
  echo "FAILED: Paper order count increased after lost-ACK recovery" >&2
  exit 1
fi

"${CTL[@]}" pause inc9a-pause-browser || true
echo "inc9a Stage C + section-49 lab path asserted."
echo "workdir=${WORKDIR}"
echo "grpc CancelWorking asserted in the partial-fill race; INC9A_KEEP=1 leaves Paper+runner paused with the recovered lost-ACK order for browser CancelWorking."
