#!/usr/bin/env bash
# Start an isolated Paper two-runner stack for trading-report.
# Never truncates an existing runner journal. Never attaches to a live stack.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

refuse() {
  echo "refuse: $*" >&2
  exit 1
}

existing_named_stack() {
  docker ps -a --format '{{.Names}}' 2>/dev/null | grep -qxE 'levels|strategies|k-paper-levels|k-paper-strategies|trading-report-mongodb'
}

if ! docker info >/dev/null 2>&1; then
  refuse "docker is required"
fi
if existing_named_stack; then
  refuse "named Paper report containers already exist; will not recreate or reset journals"
fi
if docker compose -f docker-compose.report.yml ps -q levels strategies 2>/dev/null | grep -q .; then
  refuse "docker-compose.report.yml stack is already running; will not reset journals"
fi

export K_PAPER_DIR="${K_PAPER_DIR:-$ROOT/../k-paper}"
export REPORT_WRITER_PASSWORD="${REPORT_WRITER_PASSWORD:-lab-writer-pass}"
export REPORT_READER_PASSWORD="${REPORT_READER_PASSWORD:-lab-reader-pass}"
export RUNNER_MONGO_PASSWORD="${RUNNER_MONGO_PASSWORD:-lab-runner-pass}"
export MONGO_ROOT_PASSWORD="${MONGO_ROOT_PASSWORD:-lab-root-pass}"
# This Compose file and gate are Paper lab only. Do not inherit a leftover test id.
export TRADING_REPORT_DEPLOYMENT_ID=paper-lab
if [[ ! -d "$K_PAPER_DIR" ]]; then
  refuse "k-paper checkout not found at $K_PAPER_DIR"
fi

RUN_ID="$(date -u +%Y%m%dt%H%M%Sz)-$$"
export COMPOSE_PROJECT_NAME="tr-gate-${RUN_ID}"
STACK="$ROOT/runtime-data/report-gate-${RUN_ID}"
DEFAULT_STACK="$ROOT/runtime-data/report-stack"
LEVELS="$STACK/levels"
STRATS="$STACK/strategies"
REPORTS="$STACK/reports"
SECRETS="$STACK/secrets"
BIN_DIR="$STACK/bin"
ENV_FILE="$STACK/stack.env"

if [[ "$STACK" == "$DEFAULT_STACK" || "$STACK" == "$DEFAULT_STACK"/* ]]; then
  refuse "gate must not use or reset the default report-stack directory"
fi
if [[ -e "$STACK" ]]; then
  refuse "isolated directory already exists: $STACK"
fi
mkdir -p "$LEVELS" "$STRATS" "$REPORTS" "$SECRETS" "$BIN_DIR"

create_absent() {
  local path="$1"
  if [[ -e "$path" ]]; then
    refuse "refusing to replace existing file $path"
  fi
  if [[ $# -gt 1 ]]; then
    cp --update=none "$2" "$path"
  else
    : > "$path"
  fi
}

create_absent "$LEVELS/bot.json" sample_config/report-stack/levels/bot.json
create_absent "$LEVELS/levels.json" sample_config/report-stack/levels/levels.json
create_absent "$STRATS/bot.json" sample_config/report-stack/strategies/bot.json
create_absent "$STRATS/strategies.json" sample_config/report-stack/strategies/strategies.json
create_absent "$STRATS/candles.json" scripts/inc9a_long_candles.json
create_absent "$LEVELS/submission-journal.jsonl"
create_absent "$STRATS/submission-journal.jsonl"

printf 'mongodb://report_writer:%s@mongodb:27017/trading_report?authSource=trading_report\n' \
  "$REPORT_WRITER_PASSWORD" > "$SECRETS/trading_report_mongo_uri"
printf 'mongodb://report_reader:%s@mongodb:27017/trading_report?authSource=trading_report\n' \
  "$REPORT_READER_PASSWORD" > "$SECRETS/trading_report_mongo_read_uri"
chmod 600 "$SECRETS/trading_report_mongo_uri" "$SECRETS/trading_report_mongo_read_uri"

export REPORT_STACK_LEVELS_DIR="$LEVELS"
export REPORT_STACK_STRATEGIES_DIR="$STRATS"
export REPORT_STACK_BIN_DIR="$BIN_DIR"
export TRADING_REPORTS_DIR="$REPORTS"
export TRADING_REPORT_MONGO_URI_FILE="$SECRETS/trading_report_mongo_uri"
export TRADING_REPORT_MONGO_READ_URI_FILE="$SECRETS/trading_report_mongo_read_uri"
export TRADING_REPORT_STACK_ENV="$ENV_FILE"

cat > "$ENV_FILE" <<EOF
COMPOSE_PROJECT_NAME=${COMPOSE_PROJECT_NAME}
K_PAPER_DIR=${K_PAPER_DIR}
REPORT_STACK_LEVELS_DIR=${LEVELS}
REPORT_STACK_STRATEGIES_DIR=${STRATS}
REPORT_STACK_BIN_DIR=${BIN_DIR}
TRADING_REPORTS_DIR=${REPORTS}
TRADING_REPORT_MONGO_URI_FILE=${SECRETS}/trading_report_mongo_uri
TRADING_REPORT_MONGO_READ_URI_FILE=${SECRETS}/trading_report_mongo_read_uri
TRADING_REPORT_DEPLOYMENT_ID=${TRADING_REPORT_DEPLOYMENT_ID}
REPORT_WRITER_PASSWORD=${REPORT_WRITER_PASSWORD}
REPORT_READER_PASSWORD=${REPORT_READER_PASSWORD}
RUNNER_MONGO_PASSWORD=${RUNNER_MONGO_PASSWORD}
MONGO_ROOT_PASSWORD=${MONGO_ROOT_PASSWORD}
EOF

COMPOSE=(docker compose -f docker-compose.report.yml)

echo "building host runner binaries"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo build -q -p runner --bin runner
cargo build -q -p btc_price_ma_app --bin btc_price_ma_app
cp -f "$CARGO_TARGET_DIR/debug/runner" "$BIN_DIR/runner"
cp -f "$CARGO_TARGET_DIR/debug/btc_price_ma_app" "$BIN_DIR/btc_price_ma_app"
chmod +x "$BIN_DIR/runner" "$BIN_DIR/btc_price_ma_app"

echo "building reporter image and paper-bin runtime"
"${COMPOSE[@]}" build trading-report trading-report-read levels strategies

echo "starting isolated two-runner stack ${COMPOSE_PROJECT_NAME} (does not start reporter)"
"${COMPOSE[@]}" up -d mongodb k-paper-levels k-paper-strategies levels strategies

echo "waiting for runner health"
for svc in levels strategies; do
  ok=0
  for _ in $(seq 1 40); do
    cid="$("${COMPOSE[@]}" ps -q "$svc")"
    if [[ -n "$cid" ]] && docker inspect --format '{{.State.Health.Status}}' "$cid" 2>/dev/null | grep -q healthy; then
      ok=1
      break
    fi
    sleep 3
  done
  if [[ "$ok" -ne 1 ]]; then
    echo "$svc did not become healthy" >&2
    "${COMPOSE[@]}" logs --tail=80 "$svc" >&2
    exit 1
  fi
done

echo "stack_env=${ENV_FILE}"
echo "reports_dir=${REPORTS}"
echo "isolated_stack=${STACK}"
echo "compose_project=${COMPOSE_PROJECT_NAME}"
echo "journals were created empty in a new directory; existing stacks were not touched"
