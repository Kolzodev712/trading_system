#!/usr/bin/env bash
# Build the reporter image and collect from the two-runner Compose stack.
# Writer persist + read-only read-json, untrimmed byte compare.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
COMPOSE=(docker compose -f docker-compose.report.yml)
STACK="$ROOT/runtime-data/report-stack"
LEVELS="$STACK/levels"
STRATS="$STACK/strategies"
REPORTS="$STACK/reports"
SECRETS="$STACK/secrets"
export K_PAPER_DIR="${K_PAPER_DIR:-$ROOT/../k-paper}"
export REPORT_WRITER_PASSWORD="${REPORT_WRITER_PASSWORD:-lab-writer-pass}"
export REPORT_READER_PASSWORD="${REPORT_READER_PASSWORD:-lab-reader-pass}"
export RUNNER_MONGO_PASSWORD="${RUNNER_MONGO_PASSWORD:-lab-runner-pass}"
export MONGO_ROOT_PASSWORD="${MONGO_ROOT_PASSWORD:-lab-root-pass}"

if [[ ! -d "$K_PAPER_DIR" ]]; then
  echo "k-paper checkout not found at $K_PAPER_DIR" >&2
  exit 1
fi

mkdir -p "$LEVELS" "$STRATS" "$REPORTS" "$SECRETS"
cp -f sample_config/report-stack/levels/bot.json sample_config/report-stack/levels/levels.json "$LEVELS/"
cp -f sample_config/report-stack/strategies/bot.json sample_config/report-stack/strategies/strategies.json "$STRATS/"
cp -f scripts/inc9a_long_candles.json "$STRATS/candles.json"
: > "$LEVELS/submission-journal.jsonl"
: > "$STRATS/submission-journal.jsonl"

printf 'mongodb://report_writer:%s@mongodb:27017/trading_report?authSource=trading_report\n' \
  "$REPORT_WRITER_PASSWORD" > "$SECRETS/trading_report_mongo_uri"
printf 'mongodb://report_reader:%s@mongodb:27017/trading_report?authSource=trading_report\n' \
  "$REPORT_READER_PASSWORD" > "$SECRETS/trading_report_mongo_read_uri"
chmod 600 "$SECRETS/trading_report_mongo_uri" "$SECRETS/trading_report_mongo_read_uri"

echo "building host runner binaries"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo build -q -p runner --bin runner
cargo build -q -p btc_price_ma_app --bin btc_price_ma_app
BIN_DIR="$STACK/bin"
mkdir -p "$BIN_DIR"
cp -f "$CARGO_TARGET_DIR/debug/runner" "$BIN_DIR/runner"
cp -f "$CARGO_TARGET_DIR/debug/btc_price_ma_app" "$BIN_DIR/btc_price_ma_app"
chmod +x "$BIN_DIR/runner" "$BIN_DIR/btc_price_ma_app"

echo "building reporter image and paper-bin runtime"
"${COMPOSE[@]}" build trading-report trading-report-read levels strategies

echo "starting two-runner stack (does not start reporter)"
"${COMPOSE[@]}" up -d mongodb k-paper-levels k-paper-strategies levels strategies

echo "waiting for runner health"
for svc in levels strategies; do
  ok=0
  for _ in $(seq 1 40); do
    if docker inspect --format '{{.State.Health.Status}}' "$svc" 2>/dev/null | grep -q healthy; then
      ok=1
      break
    fi
    sleep 3
  done
  if [[ "$ok" -ne 1 ]]; then
    echo "$svc did not become healthy" >&2
    docker logs "$svc" | tail -80 >&2
    exit 1
  fi
done

echo "collect with authenticated writer"
"${COMPOSE[@]}" --profile report run --rm --no-deps trading-report

LATEST="$(readlink -f "$REPORTS/latest")"
SAVED="$LATEST/report.json"
test -f "$SAVED"
echo "latest=$LATEST"

echo "read-json with authenticated report_reader"
"${COMPOSE[@]}" --profile report-read run --rm --no-deps trading-report-read > /tmp/tr-compose-read.json
cmp -s "$SAVED" /tmp/tr-compose-read.json
echo "byte_equal=true bytes=$(wc -c < "$SAVED")"
python3 - <<'PY'
import json
from pathlib import Path
j=json.loads(Path("/tmp/tr-compose-read.json").read_bytes())
print("verdict", j["verdict"], "subjects", len(j["subjects"]))
PY
