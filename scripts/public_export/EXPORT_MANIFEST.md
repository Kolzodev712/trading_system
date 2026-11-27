# Public export manifest (reviewed export)

Owner decisions: Design Log #017. This file defines what may enter a **new**
public repository produced by `prepare_public_export.sh`. The private original
keeps full history and research.

## Include

- Workspace crates required for CEX Paper / Demo laboratory paths:
  `libs/*` (except nothing under research), `src/*` engines, `cli`,
  `tests/workflow_harness`, `scripts/deepbook` (DEX optional archive),
  `sample_config/` with placeholders only, `docs/`, `architecture/`,
  `justfile`, `rust-toolchain.toml`, `Cargo.toml` / `Cargo.lock`,
  `LICENSE`, `NOTICE`, `README.md`, `.gitignore`.
- Strategy composition: `src/btc_price_ma_app`, `libs/strategy_core`,
  default runner strategy factories.
- Design logs under `.cursor/design-log/` that are non-confidential
  (optional; default **exclude** `.cursor/` from public export).

## Exclude (private / non-redistributable)

- `research/` entire tree (`research_engine`, `research_cli`, candidates,
  datasets, Python research tests, research scripts tied to private ledgers).
- Local gitignored `config/`, `secrets/`, `runtime-data/`, `trading_keys/`.
- `artifacts/` workflow outputs.
- Operational Bybit Demo evidence packs under `docs/inc10-evidence-*` if any
  contain credentials (scan before include; default exclude `docs/inc*-evidence*`).
- Cursor agent stores, `.git-rewrite/`, personal `.env` files.

## Cargo.toml transform

Public `Cargo.toml` workspace `members` must **omit**:

- `research/research_engine`
- `research/research_cli`

Root package / `just` recipes that invoke research packages must be adjusted or
documented as private-only in the private tree.

## Acceptance

- Export directory builds: `cargo check -p runner -p trading_engine -p gateway -p btc_price_ma_app -p exchange_engine -p market_data_engine` with default features.
- `cargo tree -p runner -e normal` has no `research_engine` / `research_cli`.
- No path under `research/` present in the export.
