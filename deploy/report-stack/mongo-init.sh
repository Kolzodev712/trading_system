#!/bin/bash
set -euo pipefail
mongosh --username "${MONGO_INITDB_ROOT_USERNAME}" --password "${MONGO_INITDB_ROOT_PASSWORD}" --authenticationDatabase admin <<EOF
use trading_report
db.createUser({
  user: "report_writer",
  pwd: "${REPORT_WRITER_PASSWORD}",
  roles: [{ role: "readWrite", db: "trading_report" }]
})
db.createUser({
  user: "report_reader",
  pwd: "${REPORT_READER_PASSWORD}",
  roles: [{ role: "read", db: "trading_report" }]
})
use trading_system
db.createUser({
  user: "runner",
  pwd: "${RUNNER_MONGO_PASSWORD}",
  roles: [{ role: "readWrite", db: "trading_system" }]
})
EOF
