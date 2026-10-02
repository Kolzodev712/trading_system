#!/usr/bin/env bash
# One-shot reporter invocation used by the systemd service.
# Does not start or restart runners. Does not enable the timer.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${TRADING_REPORT_WORKDIR:-$ROOT}"
if [[ -n "${TRADING_REPORT_STACK_ENV:-}" && -f "${TRADING_REPORT_STACK_ENV}" ]]; then
  # Isolated gate env: paths and compose project for this stack only.
  set -a
  # shellcheck disable=SC1090
  source "${TRADING_REPORT_STACK_ENV}"
  set +a
fi
exec docker compose -f docker-compose.report.yml --profile report run --rm --no-deps trading-report
