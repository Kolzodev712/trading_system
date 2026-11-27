# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Changed

- Relicensed under Apache License 2.0 with `NOTICE` attribution to T.L.P. Arc Entertainment Ltd.
- Documented reviewed public-export boundary (`scripts/public_export/`); research trees are omitted from public exports.
- Marked root `Dockerfile` / `Dockerfile.cli` obsolete; removed suppressed `cargo build || true` failure paths.
- Default GitHub branch is `main` only.

### Fixed

- Paper workflow harness instruments use tick-based market fields and enable `PAPER_LAB_FAULTS` for lab routes.
- CEX exchange state API lives in `trading_core` so `trading_engine` no longer depends on venue adapters (Design Log #016 Phase B).
