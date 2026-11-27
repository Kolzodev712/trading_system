# Contributing

Thank you for interest in the trading laboratory.

## Ground rules

- Prefer small, reviewable changes with tests.
- Do not commit secrets, live credentials, or personal `.env` files.
- Do not claim production, mainnet, or alpha readiness without matching evidence.
- Research archives under `research/` are **not** part of the public export; keep new proprietary research out of public patches.

## Development

- Toolchain: see `rust-toolchain.toml` (currently `1.98.1`).
- Format: `just fmt` / `cargo fmt --all`
- Lint: `just clippy-strict` (or `cargo clippy -p <crate> -- -D warnings`)
- Component tests: `just test-fast` (local). Paper process workflows: `just test-paper` (requires Mongo + sibling `k-paper`).
- There is **no** GitHub Actions CI for this project by owner decision; run the local recipes before opening a change.

## Public export

See `scripts/public_export/EXPORT_MANIFEST.md`. Public trees omit `research/`.

## License

Contributions are accepted under the Apache License 2.0 (see `LICENSE` and `NOTICE`).
