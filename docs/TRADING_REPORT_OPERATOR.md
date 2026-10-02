# Trading report operator guide

The `trading-report` binary is a read-only observer of the Levels and Strategies runners. It does not issue trading-control RPCs, Resume, place Demo orders, or change strategy state. A scheduled run is a one-shot container plus a host timer. There is no resident reporting daemon.

`verified` means the available sources agreed for that snapshot. It is not trade approval, alias verification, or alpha validation.

## Commands

File-only development collect (no Mongo):

```bash
cargo run -p trading_report --bin trading-report -- collect \
  --config sample_config/report.toml \
  --out /tmp/report-dev
```

Scheduled delivery collect (files **and** Mongo). A successful Mongo write is required for the run to count as delivered:

```bash
export TRADING_REPORT_MONGO_URI='mongodb://report_writer@127.0.0.1:27017/trading_report'
export TRADING_REPORT_DEPLOYMENT_ID=paper-lab
cargo run -p trading_report --bin trading-report -- collect \
  --config sample_config/report.toml \
  --out runtime-data/reports/manual \
  --persist-mongo \
  --deployment-id paper-lab
```

Replay a saved report after a Mongo outage. This stores the original `report.json` bytes and `report_id`. It does not recollect venue state:

```bash
cargo run -p trading_report --bin trading-report -- persist \
  --from runtime-data/reports/2026-10-02T10-00-28Z \
  --deployment-id paper-lab
```

Read from Mongo only (no gRPC, REST, journal, or collect). stdout is JSON only; diagnostics go to stderr:

```bash
# Read-only credential. Do not use the writer URI here.
export TRADING_REPORT_MONGO_URI='mongodb://report_reader@127.0.0.1:27017/trading_report'
trading-report read-json --report-id tr-<uuid> --output -
trading-report read-json --latest --deployment-id paper-lab --output /tmp/report.json
```

`--latest` never chooses across deployments. An `unverified` document is returned with its original verdict. Missing, corrupt, or unavailable data exits nonzero (2 / 3 / 1).

One-shot scheduled run (unique UTC directory, lock, timeout, Mongo required, `latest` after both succeed):

```bash
trading-report scheduled \
  --config /app/report.toml \
  --reports-dir /reports \
  --timeout-secs 120 \
  --deployment-id paper-lab
```

## Environment and secrets

| Name | Role |
| --- | --- |
| `TRADING_REPORT_MONGO_URI` | Mongo URI. Never put this in TOML, report files, or logs. |
| `TRADING_REPORT_MONGO_URI_FILE` | Optional file containing the URI (Docker / systemd secret). |
| `TRADING_REPORT_MONGO_DB` | Database. Default `trading_report`. |
| `TRADING_REPORT_MONGO_COLLECTION` | Collection. Default `reports`. |
| `TRADING_REPORT_DEPLOYMENT_ID` | Deployment identity. Required for persist / scheduled / `--latest`. |
| `TRADING_REPORT_TEST_MONGO_URI` | Test override only. |

Writer example (create locally; do not commit):

```javascript
db.getSiblingDB("trading_report").createUser({
  user: "report_writer",
  pwd: "...",
  roles: [{ role: "readWrite", db: "trading_report" }]
})
```

Read-only credential for `read-json` and any future AI consumer:

```javascript
db.getSiblingDB("trading_report").createUser({
  user: "report_reader",
  pwd: "...",
  roles: [{ role: "read", db: "trading_report" }]
})
```

Store the writer URI in `secrets/trading_report_mongo_uri` or `/etc/trading-report.env`. The host Mongo in this lab is often `mongodb://127.0.0.1:27017` with no auth; still keep the value out of git.

## Container

`Dockerfile.report` builds the same `trading-report` binary. Compose service `trading-report` is on profile `report` and the `trading-network` stack network. It publishes no port and is not started by `docker compose up`.

```bash
# Does not start or restart either runner (--no-deps).
docker compose --profile report run --rm --no-deps trading-report
```

Mounts:

| Host / env | Container | Mode |
| --- | --- | --- |
| `TRADING_REPORT_CONFIG` (default `./config/report.toml`) | `/app/report.toml` | read-only |
| `TRADING_REPORT_LEVELS_JOURNAL` | `/journals/levels/submission-journal.jsonl` | read-only |
| `TRADING_REPORT_STRATEGIES_JOURNAL` | `/journals/strategies/submission-journal.jsonl` | read-only |
| `TRADING_REPORTS_DIR` (default `./runtime-data/reports`) | `/reports` | read-write |
| `TRADING_REPORT_MONGO_URI_FILE` | `/run/secrets/trading_report_mongo_uri` | secret |

Copy `sample_config/report.toml` to `config/report.toml` and point gRPC/REST hosts at the running Levels and Strategies containers on the stack network.

## systemd timer

Install on the host that runs Docker Compose for the stack:

```bash
sudo mkdir -p /opt/trading_system
sudo cp deploy/systemd/trading-report-collect.service /etc/systemd/system/
sudo cp deploy/systemd/trading-report-collect.timer /etc/systemd/system/
# WorkingDirectory in the unit is /opt/trading_system; keep compose files there
# or edit WorkingDirectory.
printf 'TRADING_REPORT_DEPLOYMENT_ID=paper-lab\n' | sudo tee /etc/trading-report.env
sudo systemctl daemon-reload
sudo systemctl enable --now trading-report-collect.timer
```

The timer fires every 15 minutes (`*:0/15`). Each fire is a one-shot `docker compose run --rm --no-deps` with `TimeoutStartSec=180` and `flock -n` so overlapping runs are skipped. The binary also takes `/reports/.collect.lock`.

Inspect logs:

```bash
systemctl list-timers trading-report-collect.timer
journalctl -u trading-report-collect.service -n 50
```

Each successful log line includes `report_id`, source `verdict`, `mongo=delivered`, and `out=`. `latest=` is printed after the pointer is published.

## Where `latest` lives

`TRADING_REPORTS_DIR/latest` is a symlink to the unique UTC directory of the last run whose **file output and Mongo write both succeeded**. An `unverified` report may be `latest`. An older `verified` report is never kept as current once a newer delivered run exists. A Mongo failure leaves the UTC directory on disk and does **not** move `latest`.

Replay that failed write:

```bash
trading-report persist --from runtime-data/reports/<utc-dir> --deployment-id paper-lab
# After Mongo accepts the original bytes, optionally:
ln -sfn <utc-dir> runtime-data/reports/latest
```

## Stored document

One immutable Mongo document per combined collection: `report_id`, `deployment_id`, `collected_at_utc`, `schema_version`, `verdict`, `actionable_for_analysis`, `subject_ids`, the exact UTF-8 `report.json`, and `report_json_sha256`. Indexes: unique `report_id`, and `(deployment_id, collected_at desc)` for latest-in-deployment. Same ID and identical JSON is idempotent. Same ID and different JSON fails. There is no TTL or overwrite. Documents larger than Mongo's 16 MiB BSON limit fail before insert.

Collection and persistence failures do not contact runner control RPCs and cannot start, stop, or restart the trading containers.
