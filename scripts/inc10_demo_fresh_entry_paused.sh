#!/usr/bin/env bash
# Increment 10: paused Demo procedure for the reviewed fresh-entry fixture only.
# Loads scripts/inc10_demo_fresh_entry_candles.json. Real clock. Live book.
# Does not Resume. Does not enable. INC10_ENABLE=1 is rejected.
# Retains 0.001 BTC / 1 working-order bounds, evidence preservation, and
# independently verified-flat checks. The five-minute measured path stays in
# scripts/inc10_bybit_demo.sh and is not invoked here.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKDIR="${INC10_WORKDIR:-/tmp/inc10-demo-fresh-entry-paused}"
SOURCE_BOT="${INC10_BOT:-$ROOT/config/bot.json}"
LOG="${WORKDIR}/runner.log"
JOURNAL="${WORKDIR}/submission-journal.jsonl"
LEDGER="${WORKDIR}/execution-ledger.jsonl"
TOKEN_FILE="${WORKDIR}/operator.token"
SNAPSHOT_SRC="${ROOT}/config/cex/runtime-snapshot.json"
REVIEWED_CANDLES="${ROOT}/scripts/inc10_demo_fresh_entry_candles.json"
REVIEWED_SPEC="${ROOT}/scripts/inc10_demo_fresh_entry_fixture.json"
HOLD_CANDLES="${ROOT}/scripts/inc10_demo_hold_candles.json"
# Last completed bar ends 2026-09-29T00:00:00Z; stale at +86400s.
FRESH_UNTIL_MS=1790726400000

export TRADING_STRATEGY="${TRADING_STRATEGY:-BTCPriceMALongFlatV1}"
export TRADING_SYMBOL="${TRADING_SYMBOL:-BTC/USDT}"
export TRADING_GRPC="${TRADING_GRPC:-http://127.0.0.1:50051}"
export TRADING_OPERATOR_ID="${TRADING_OPERATOR_ID:-inc10}"
export TRACE_JAEGER_DISABLED="${TRACE_JAEGER_DISABLED:-1}"

mkdir -p "${WORKDIR}" "${ROOT}/config/cex"

if [[ "${INC10_RESET:-0}" == "1" ]]; then
  echo "FAILED: INC10_RESET=1 is not allowed for this bounded demo procedure (evidence must be preserved)" >&2
  exit 1
fi
if [[ "${INC10_ENABLE:-0}" == "1" ]]; then
  echo "FAILED: INC10_ENABLE is not authorized for this paused fixture procedure; no Resume" >&2
  exit 1
fi
if [[ -n "${TRADING_TEST_NOW_MS:-}" ]]; then
  echo "FAILED: TRADING_TEST_NOW_MS is not authorized; this procedure uses the real clock" >&2
  exit 1
fi
if [[ ! -f "${REVIEWED_CANDLES}" || ! -f "${REVIEWED_SPEC}" ]]; then
  echo "FAILED: reviewed fresh-entry fixture missing (${REVIEWED_CANDLES})" >&2
  exit 1
fi
python3 - "${REVIEWED_CANDLES}" "${REVIEWED_SPEC}" "${FRESH_UNTIL_MS}" <<'PY'
import json, sys, time
from datetime import datetime, timezone
candles_path, spec_path, until_ms = sys.argv[1], sys.argv[2], int(sys.argv[3])
now_ms = int(time.time() * 1000)
if now_ms >= until_ms:
    until = datetime.fromtimestamp(until_ms / 1000, timezone.utc).isoformat()
    raise SystemExit(
        f"FAILED: fresh-entry window ended at {until}; this transition is stale under 86400s"
    )
spec = json.loads(open(spec_path).read())
if spec.get("kind") != "fresh_entry":
    raise SystemExit("FAILED: fixture spec kind must be fresh_entry")
if "TRADING_TEST_NOW_MS" not in (spec.get("forbidden_env") or []):
    raise SystemExit("FAILED: fixture spec must forbid TRADING_TEST_NOW_MS")
candles = json.loads(open(candles_path).read())
if len(candles) != 29:
    raise SystemExit(f"FAILED: expected 29 completed dailies, got {len(candles)}")
last = candles[-1]
if str(last.get("close")) != "110":
    raise SystemExit("FAILED: last completed close must be 110 (enter-long)")
print(
    f"reviewed fixture still fresh: now_ms={now_ms} until_ms={until_ms} "
    f"last_start_ms={last.get('timestamp_ms')} n={len(candles)}"
)
PY
CANON="$(realpath "${REVIEWED_CANDLES}")"
HOLD_CANON="$(realpath "${HOLD_CANDLES}" 2>/dev/null || true)"
if [[ -n "${TRADING_TEST_CANDLES_PATH:-}" ]]; then
  GOT="$(realpath "${TRADING_TEST_CANDLES_PATH}")"
  if [[ "${GOT}" == "${HOLD_CANON}" ]]; then
    echo "FAILED: Hold diagnostic candles are not accepted by this procedure" >&2
    exit 1
  fi
  if [[ "${GOT}" != "${CANON}" ]]; then
    echo "FAILED: TRADING_TEST_CANDLES_PATH must be the reviewed fixture ${CANON} (got ${GOT})" >&2
    exit 1
  fi
fi
export TRADING_TEST_CANDLES_PATH="${CANON}"
echo "synthetic fixture path=${TRADING_TEST_CANDLES_PATH} (Hold fixture rejected; clock is real)"

mkdir -p "${WORKDIR}/logs"
# Archive prior runner logs; do not delete journal, ledger, snapshot, or evidence logs.
if compgen -G "${WORKDIR}/runner*.log" >/dev/null; then
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  for f in "${WORKDIR}"/runner*.log; do
    mv "${f}" "${WORKDIR}/logs/$(basename "${f}").${ts}"
  done
  echo "archived prior runner logs under ${WORKDIR}/logs"
fi

if [[ ! -f "${SOURCE_BOT}" ]]; then
  echo "FAILED: missing ${SOURCE_BOT} (gitignored demo bot with demo:true keys)" >&2
  exit 1
fi

python3 - "${SOURCE_BOT}" "${WORKDIR}/bot.json" "${JOURNAL}" "${LEDGER}" <<'PY'
import json, sys
src, dest, journal, ledger = sys.argv[1:5]
bot = json.loads(open(src).read())
eng = bot.setdefault("exchange_engine", {})
if str(eng.get("exchange", "")).strip().lower() != "bybit":
    raise SystemExit("FAILED: source bot must use exchange_engine.exchange=Bybit")
cfg = eng.setdefault("config", {})
if cfg.get("demo") is not True:
    raise SystemExit("FAILED: source bot must set exchange_engine.config.demo=true")
if cfg.get("testnet") is True:
    raise SystemExit("FAILED: source bot must not set testnet when demo=true")
inst = str(cfg.get("instrumentType") or cfg.get("instrument_type") or "")
if inst.lower() != "futures":
    raise SystemExit("FAILED: source bot must be Futures (USDT linear)")
pairs = cfg.get("pairs") or []
if len(pairs) != 1:
    raise SystemExit("FAILED: source bot must configure exactly one pair (BTC/USDT linear)")
base = str(pairs[0].get("base", "")).upper()
quote = str(pairs[0].get("quote", "")).upper()
if (base, quote) != ("BTC", "USDT"):
    raise SystemExit("FAILED: source bot pair must be BTC/USDT")
bot["execution_mode"] = "exchange"
bot["submission_journal_path"] = journal
bot["execution_ledger_path"] = ledger
bot.setdefault("mongo", {})["bot_id"] = "inc10"
bot.setdefault("reconciliation", {})
bot["reconciliation"]["exchange_level_interval_ms"] = 5000
bot["reconciliation"]["trading_level_interval_ms"] = 5000
bot["reconciliation"]["enabled"] = True
open(dest, "w").write(json.dumps(bot, indent=2) + "\n")
print("inc10 bot written (credentials not printed); Bybit DEMO futures BTC/USDT linear only")
print(f"journal={journal} ledger={ledger} (preserved; INC10_RESET is rejected)")
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
    "total_max_position": "0.001",
    "max_total_orders": 1,
    "max_notional": "150",
    "max_order_qty": "0.001"
  }
}
EOF

# Envoy Lua reads this bind mount as a non-root worker. Grant that uid read via ACL
# (or the file's group). Do not chmod 0644 / other-readable.
envoy_proxy_uid() {
  local pid user uid
  if [[ -n "${INC10_ENVOY_UID:-}" ]]; then
    printf '%s\n' "${INC10_ENVOY_UID}"
    return 0
  fi
  pid="$(docker inspect docker-grpc-web-proxy-1 --format '{{.State.Pid}}' 2>/dev/null || true)"
  if [[ -n "${pid}" && "${pid}" != "0" && -r "/proc/${pid}/status" ]]; then
    uid="$(awk '/^Uid:/{print $2}' "/proc/${pid}/status")"
    if [[ -n "${uid}" ]]; then
      printf '%s\n' "${uid}"
      return 0
    fi
  fi
  user="$(docker image inspect envoyproxy/envoy:v1.31-latest --format '{{.Config.User}}' 2>/dev/null || true)"
  user="${user%%:*}"
  if [[ "${user}" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "${user}"
    return 0
  fi
  # Official envoy image USER envoy is uid 101.
  printf '101\n'
}

restrict_operator_token_file() {
  local path="$1"
  local uid
  chmod 600 "${path}"
  uid="$(envoy_proxy_uid)"
  if command -v setfacl >/dev/null 2>&1; then
    if setfacl -m "u:${uid}:r" "${path}"; then
      echo "operator token mode=0600 ACL u:${uid}:r (not world-readable)"
      return 0
    fi
  fi
  if chgrp "${uid}" "${path}" 2>/dev/null && chmod 640 "${path}"; then
    echo "operator token mode=0640 group=${uid} (not world-readable)"
    return 0
  fi
  echo "FAILED: cannot grant Envoy uid ${uid} read on ${path} without world-readable bits" >&2
  echo "Set INC10_ENVOY_UID or install setfacl (acl package)." >&2
  exit 1
}

if [[ -z "${TRADING_OPERATOR_TOKEN:-}" && -f "${TOKEN_FILE}" ]]; then
  TRADING_OPERATOR_TOKEN="$(head -n1 "${TOKEN_FILE}")"
fi
if [[ -z "${TRADING_OPERATOR_TOKEN:-}" ]]; then
  TRADING_OPERATOR_TOKEN="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
fi
export TRADING_OPERATOR_TOKEN
printf '%s\n' "${TRADING_OPERATOR_TOKEN}" > "${TOKEN_FILE}"
restrict_operator_token_file "${TOKEN_FILE}"
echo "operator token file=${TOKEN_FILE} (not printed); Envoy: TRADING_OPERATOR_TOKEN_FILE=${TOKEN_FILE}"

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

require_open_order_rest_ok() {
  local log="$1"
  local i
  for i in $(seq 1 40); do
    if grep -q "Bootstrap: Replaced open orders from REST" "${log}"; then
      break
    fi
    sleep 0.25
  done
  if ! grep -q "Bootstrap: Replaced open orders from REST" "${log}"; then
    echo "FAILED: runner did not complete REST open-order bootstrap (${log})" >&2
    tail -n 80 "${log}" >&2
    exit 1
  fi
  # One exchange-level recon interval (script sets 5000ms) plus bootstrap.
  sleep 6
  if grep -qE 'Failed to fetch open orders|Invalid path or category mismatch|order/open-order' "${log}"; then
    echo "FAILED: runner open-order REST still failing (${log})" >&2
    grep -E 'Failed to fetch open orders|Invalid path|order/open-order|Replaced open orders' "${log}" | tail -n 20 >&2
    exit 1
  fi
  echo "runner open-order REST ok ($(grep -c 'Bootstrap: Replaced open orders from REST' "${log}") bootstrap; no 404/open-order path)"
}

preserve_snapshot_copy() {
  if [[ -f "${SNAPSHOT_SRC}" ]]; then
    cp -a "${SNAPSHOT_SRC}" "${WORKDIR}/runtime-snapshot.json"
    echo "snapshot preserved at ${SNAPSHOT_SRC} (copy ${WORKDIR}/runtime-snapshot.json)"
  else
    echo "no runtime snapshot yet (${SNAPSHOT_SRC})"
  fi
}

# Independent demo REST vs runner TSM. Credentials stay in the bot file; not printed.
compare_demo_account() {
  local tag="$1"
  local status="$2"
  local require_flat="${3:-0}"
  python3 - "${WORKDIR}/bot.json" "${status}" "${WORKDIR}/venue_${tag}.json" "${require_flat}" <<'PY'
import hashlib, hmac, json, re, sys, time, urllib.parse, urllib.request
from decimal import Decimal, InvalidOperation

bot_path, status, out_path, require_flat = sys.argv[1:5]
bot = json.loads(open(bot_path).read())
cfg = bot["exchange_engine"]["config"]
cred = cfg["credentials"]
key, secret = cred["apiKey"], cred["apiSecret"]
base = "https://api-demo.bybit.com"
recv = "5000"


def signed_get(path, params):
    qs = urllib.parse.urlencode(params)
    ts = str(int(time.time() * 1000))
    payload = ts + key + recv + qs
    sig = hmac.new(secret.encode(), payload.encode(), hashlib.sha256).hexdigest()
    req = urllib.request.Request(
        f"{base}{path}?{qs}",
        headers={
            "X-BAPI-API-KEY": key,
            "X-BAPI-TIMESTAMP": ts,
            "X-BAPI-RECV-WINDOW": recv,
            "X-BAPI-SIGN": sig,
        },
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        body = json.loads(resp.read().decode())
    if body.get("retCode") != 0:
        raise SystemExit(f"FAILED: Bybit demo {path} retCode={body.get('retCode')} retMsg={body.get('retMsg')}")
    return body.get("result") or {}


pos = signed_get("/v5/position/list", {"category": "linear", "symbol": "BTCUSDT"})
orders = signed_get("/v5/order/realtime", {"category": "linear", "symbol": "BTCUSDT"})


def qty_norm(value):
    try:
        return Decimal(str(value if value is not None else "0"))
    except (InvalidOperation, ValueError):
        return Decimal("0")


def side_norm(value):
    s = str(value or "").strip().lower()
    if s in ("buy", "long"):
        return "buy"
    if s in ("sell", "short"):
        return "sell"
    return s


def symbol_norm(value):
    return str(value or "").replace("/", "").replace("-", "").upper()


venue_positions = []
for p in pos.get("list") or []:
    qty = qty_norm(p.get("size") or "0")
    if qty == 0:
        continue
    venue_positions.append(
        {
            "symbol": symbol_norm(p.get("symbol")),
            "side": side_norm(p.get("side")),
            "qty": str(qty),
        }
    )
venue_positions.sort(key=lambda r: (r["symbol"], r["side"], r["qty"]))
order_rows = orders.get("list") or []
grpc_orders = 0
m = re.search(r"orders=(\d+)", status)
if m:
    grpc_orders = int(m.group(1))
grpc_pos_n = 0
m = re.search(r"positions=(\d+)", status)
if m:
    grpc_pos_n = int(m.group(1))
strat_pos = "0"
m = re.search(r"strategy .* pos=([0-9.-]+)", status)
if m:
    strat_pos = m.group(1)
grpc_positions = []
for m in re.finditer(
    r"position (\S+) qty=([0-9.-]+) side=(\S+)",
    status,
):
    qty = qty_norm(m.group(2))
    if qty == 0:
        continue
    grpc_positions.append(
        {
            "symbol": symbol_norm(m.group(1)),
            "side": side_norm(m.group(3)),
            "qty": str(qty),
        }
    )
grpc_positions.sort(key=lambda r: (r["symbol"], r["side"], r["qty"]))
venue = {
    "demo_positions": venue_positions,
    "demo_open_orders": len(order_rows),
    "grpc_orders": grpc_orders,
    "grpc_positions_n": grpc_pos_n,
    "grpc_positions": grpc_positions,
    "grpc_strategy_pos": strat_pos,
}
open(out_path, "w").write(json.dumps(venue, indent=2) + "\n")
print(
    f"independent demo REST: open_orders={len(order_rows)} positions={venue_positions} "
    f"grpc orders={grpc_orders} positions={grpc_positions} strategy_pos={strat_pos}"
)
if len(order_rows) != grpc_orders:
    raise SystemExit(
        f"FAILED: demo REST open orders {len(order_rows)} != grpc orders={grpc_orders}"
    )


def pos_key(row):
    return (row["symbol"], row["side"], qty_norm(row["qty"]))


if [pos_key(r) for r in venue_positions] != [pos_key(r) for r in grpc_positions]:
    raise SystemExit(
        f"FAILED: demo REST positions {venue_positions} != grpc positions {grpc_positions}"
    )
if len(venue_positions) != grpc_pos_n:
    raise SystemExit(
        f"FAILED: demo REST position count {len(venue_positions)} != grpc positions={grpc_pos_n}"
    )
net = Decimal("0")
for row in venue_positions:
    q = qty_norm(row["qty"])
    net += q if row["side"] == "buy" else -q
if qty_norm(strat_pos) != net:
    raise SystemExit(
        f"FAILED: strategy pos={strat_pos} != demo REST net qty {net}"
    )
if require_flat == "1":
    if order_rows or venue_positions or grpc_orders or grpc_positions or qty_norm(strat_pos) != 0:
        raise SystemExit(
            "FAILED: account is not independently flat "
            f"(rest_orders={len(order_rows)} rest_pos={venue_positions} "
            f"grpc_orders={grpc_orders} grpc_pos={grpc_positions} strat={strat_pos})"
        )
    print("independent demo REST and runner are flat (zero orders, zero position)")
PY
}

require_bounded_status() {
  local status="$1"
  python3 - "${status}" <<'PY'
import re, sys
from decimal import Decimal, InvalidOperation
status = sys.argv[1]
limit = Decimal("0.001")

def qty(value):
    try:
        return abs(Decimal(str(value)))
    except (InvalidOperation, ValueError):
        return Decimal("0")

orders = 0
m = re.search(r"orders=(\d+)", status)
if m:
    orders = int(m.group(1))
if orders > 1:
    raise SystemExit(f"FAILED: working orders {orders} exceeds bound 1")
strat = Decimal("0")
m = re.search(r"strategy .* pos=([0-9.-]+)", status)
if m:
    strat = qty(m.group(1))
if strat > limit:
    raise SystemExit(f"FAILED: strategy pos {strat} exceeds 0.001 BTC")
for m in re.finditer(r"position \S+ qty=([0-9.-]+)", status):
    if qty(m.group(1)) > limit:
        raise SystemExit(f"FAILED: position qty {m.group(1)} exceeds 0.001 BTC")
for m in re.finditer(r"order \S+ \S+ \S+ px=\S+ qty=([0-9.-]+)", status):
    if qty(m.group(1)) > limit:
        raise SystemExit(f"FAILED: working order qty {m.group(1)} exceeds 0.001 BTC")
print(f"bounded: orders={orders} strategy_abs_pos={strat} (<=0.001 BTC, <=1 working order)")
PY
}

require_operator_cmd() {
  local action="$1"
  local request_id="$2"
  local out rc
  set +e
  out="$("${CTL[@]}" "${action}" "${request_id}" 2>&1)"
  rc=$?
  set -e
  echo "${out}"
  if [[ "${rc}" -ne 0 ]]; then
    echo "FAILED: ${action} RPC failed (rc=${rc})" >&2
    return 1
  fi
  if ! echo "${out}" | grep -q 'accepted=true'; then
    echo "FAILED: ${action} was not accepted" >&2
    echo "${out}" >&2
    return 1
  fi
  return 0
}

record_evidence() {
  echo "== evidence paths =="
  echo "workdir=${WORKDIR}"
  echo "journal=${JOURNAL} ledger=${LEDGER}"
  if [[ -f "${JOURNAL}" ]]; then
    echo "journal lines=$(wc -l < "${JOURNAL}")"
    tail -n 30 "${JOURNAL}" || true
  else
    echo "journal: none yet"
  fi
  if [[ -f "${LEDGER}" ]]; then
    echo "ledger lines=$(wc -l < "${LEDGER}")"
    tail -n 30 "${LEDGER}" || true
  else
    echo "ledger: none yet"
  fi
}

end_measured_run() {
  echo "== end measured run: pause, cancel working, flatten, verify independently flat =="
  if ! require_operator_cmd pause "inc10-end-pause"; then
    record_evidence
    return 1
  fi
  if ! require_operator_cmd cancel "inc10-end-cancel"; then
    record_evidence
    return 1
  fi
  local i status resolved=0
  for i in $(seq 1 40); do
    status="$("${CTL[@]}" status)"
    if echo "${status}" | grep -q "orders=0"; then
      resolved=1
      break
    fi
    sleep 0.5
  done
  if [[ "${resolved}" -ne 1 ]]; then
    echo "FAILED: working orders still present after CancelWorking" >&2
    echo "${status}" >&2
    record_evidence
    return 1
  fi
  if ! require_operator_cmd flatten "inc10-end-flatten"; then
    record_evidence
    return 1
  fi
  local flat=0
  for i in $(seq 1 60); do
    status="$("${CTL[@]}" status)"
    if echo "${status}" | grep -q "orders=0" \
      && echo "${status}" | grep -q "positions=0" \
      && echo "${status}" | grep -qE 'pos=0 cap='; then
      flat=1
      break
    fi
    sleep 1
  done
  status="$("${CTL[@]}" status | tee "${WORKDIR}/grpc_status_end.txt")"
  echo "${status}"
  if [[ "${flat}" -ne 1 ]] || ! compare_demo_account end_flat "${status}" 1; then
    echo "FAILED: end-state is not independently flat (zero orders and zero position)" >&2
    record_evidence
    return 1
  fi
  require_status 'entries_paused=true' "${status}"
  require_status 'orders=0' "${status}"
  require_status 'positions=0' "${status}"
  preserve_snapshot_copy
  record_evidence
  echo "measured run end-state verified: paused, zero working orders, zero position (Demo REST and runner)"
  kill_runner
  return 0
}

cleanup() {
  if [[ "${INC10_KEEP:-}" == "1" ]]; then
    echo "INC10_KEEP=1: leaving runner pid=${RUNNER_PID:-}"
    return 0
  fi
  kill_runner || true
}
trap cleanup EXIT

echo "== package fixtures =="
echo "paused fresh-entry fixture procedure: only ${TRADING_TEST_CANDLES_PATH}; TRADING_TEST_NOW_MS unset; no Resume."
(cd "${ROOT}" && cargo test -q -p btc_price_ma_app --lib app_factory_builds_shadow_paper_and_bybit_exchange)
(cd "${ROOT}" && cargo test -q -p btc_price_ma_app --lib fresh_entry_fixture_last_completed_transitions_on_real_clock)
(cd "${ROOT}" && cargo test -q -p btc_price_ma_app --lib fresh_entry_script_fixture_last_completed_transitions_on_real_clock)
(cd "${ROOT}" && cargo test -q -p runner --lib bybit_exchange_allows_demo_rejects_mainnet)
(cd "${ROOT}" && cargo test -q -p runner --lib exchange_fixture_confined_to_paper_or_bybit_demo)
(cd "${ROOT}" && cargo test -q -p gateway --lib grpc::access)

echo "== build composition + grpc_ctl =="
(cd "${ROOT}" && cargo build -q -p btc_price_ma_app -p cli --bin grpc_ctl)

if command -v fuser >/dev/null 2>&1; then
  fuser -k 50051/tcp 2>/dev/null || true
  sleep 1
fi

echo "== start C2 on Bybit demo with entries paused (reviewed fresh-entry fixture) =="
start_runner "${LOG}"
sleep 2
if ! grep -q 'synthetic_candle_fixture' "${LOG}"; then
  echo "FAILED: runner log missing synthetic_candle_fixture (fixture was not loaded)" >&2
  grep -E 'candle|fixture|synthetic' "${LOG}" | tail -n 40 >&2 || true
  exit 1
fi
if grep -qE 'Bybit MAINNET' "${LOG}"; then
  echo "FAILED: runner log mentioned MAINNET" >&2
  exit 1
fi
status="$("${CTL[@]}" status)"
echo "${status}" | tee "${WORKDIR}/grpc_status_paused.txt"
require_status 'venue=BYBIT' "${status}"
require_status 'environment=DEMO' "${status}"
require_status 'execution_mode=exchange' "${status}"
require_status 'entries_paused=true' "${status}"
require_status "operator_id=${TRADING_OPERATOR_ID}" "${status}"
require_status 'model_reconstructed=true' "${status}"
require_status 'fresh_entry_eligible=true' "${status}"
require_status 'entry_freshness=fresh' "${status}"
python3 - "${status}" <<'PY'
import re, sys
status = sys.argv[1]
m = re.search(r"last_flat_to_long_age_secs=([0-9-]+)", status)
if not m:
    raise SystemExit("FAILED: status missing last_flat_to_long_age_secs")
age = int(m.group(1))
if age < 0 or age > 86400:
    raise SystemExit(f"FAILED: last_flat_to_long_age_secs={age} is not a fresh 86400s window")
print(f"hold gate fresh: last_flat_to_long_age_secs={age}")
PY
if grep -qE 'Bybit demo mode: placing order via REST' "${LOG}"; then
  echo "FAILED: runner placed while entries should be paused" >&2
  grep -E 'placing order' "${LOG}" >&2
  exit 1
fi
grep -E 'synthetic_candle_fixture|Bybit position mode|one-way|REST|private|gRPC server|entries start paused' "${LOG}" | tail -n 40 \
  | tee "${WORKDIR}/connectivity_snippets.txt" || true

echo "== runner paused REST open-order reconciliation =="
require_open_order_rest_ok "${LOG}"

echo "== independent demo account vs runner (paused) =="
compare_demo_account paused "${status}" 1
require_bounded_status "${status}"
if ! grep -q '"total_max_position": "0.001"' "${WORKDIR}/strategies.json" \
  || ! grep -q '"max_total_orders": 1' "${WORKDIR}/strategies.json" \
  || ! grep -q '"max_order_qty": "0.001"' "${WORKDIR}/strategies.json"; then
  echo "FAILED: strategies.json does not enforce 0.001 BTC / 1 working order bounds" >&2
  exit 1
fi

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
require_open_order_rest_ok "${WORKDIR}/runner-restart.log"
compare_demo_account restart "${status}" 1
require_status 'model_reconstructed=true' "${status}"
require_status 'fresh_entry_eligible=true' "${status}"
require_status 'entry_freshness=fresh' "${status}"
if ! grep -q 'synthetic_candle_fixture' "${WORKDIR}/runner-restart.log"; then
  echo "FAILED: restarted runner log missing synthetic_candle_fixture" >&2
  exit 1
fi
preserve_snapshot_copy
record_evidence

echo "${status}"
echo "inc10 paused fresh-entry fixture checks passed (synthetic signal, Hold gate fresh, connectivity, auth, runner REST recon, restart, independent REST flat, 0.001 BTC / 1-order bounds)."
echo "No Resume. No orders. Increment 10 remains open."
echo "workdir=${WORKDIR}"
echo "journal=${JOURNAL} ledger=${LEDGER} snapshot=${SNAPSHOT_SRC}"
echo "INC10_ENABLE is rejected by this procedure. Do not Resume until an authorized measured run."
INC10_KEEP=1
trap - EXIT
echo "INC10_KEEP runner pid=${RUNNER_PID}"
echo "UI Envoy token mount: TRADING_OPERATOR_TOKEN_FILE=${TOKEN_FILE}"
exit 0
