# Release candidate report — v0.1.0 (proposed)

**Status:** Stop before publication. Owner approval required to create public remotes,
change visibility, or publish images/tags.

**Date:** 2026-11-27  
**Decisions:** Design Log #017

## Component revisions (private remotes at report time)

| Component | Branch | Notes |
|-----------|--------|-------|
| trading_system | `main` | Apache 2.0; export script; Docker obsolete labeled |
| k-paper | `master` | Apache 2.0; Docker config packaging fixed |
| trading-system-ui | `main` | MIT retained |
| trading-system-showcase | `main` | Apache 2.0; `lab/` Compose migrated |
| trading-system-showcase-demo | `main` | Remains **private** demonstration source |

Exact SHAs: record at publish time with `git rev-parse HEAD` in each repo after push.

## Build / test evidence (executed this preparation)

| Check | Result |
|-------|--------|
| Public export `cargo check -p runner -p trading_engine -p gateway -p btc_price_ma_app -p exchange_engine -p market_data_engine` | PASS |
| Export `cargo tree -p runner` contains `research_*` | PASS (absent) |
| Export gitleaks (filesystem) | PASS (0 findings) |
| k-paper `cargo check --workspace` | PASS |
| UI `npm test` | PASS (73) |
| `just test-5r-graph` | PASS (run with this prep) |
| `just test-paper` | PASS (LAYER paper PASS, 2026-11-27) |
| `just test-fast` | PASS after sampling-window fix |

## Secret scan (gitleaks)

| Target | Findings |
|--------|----------|
| trading_system **git history** | 18 `generic-api-key` hits in historical paths (binance/exchange test configs and old bins) — **motivates reviewed export** |
| trading_system **export tree** | 0 |
| k-paper git | 0 |
| trading-system-ui git | 0 |
| trading-system-showcase git | 0 |
| showcase-demo local artifacts | credential-shaped files under `artifacts/build/` (gitignored) — do not publish |

**Action:** Rotate any credentials that may have been real in local `config/` or historical commits. Removing history from a public export does not invalidate leaked secrets.

## Licensing

| Repo | License |
|------|---------|
| trading_system | Apache 2.0 + NOTICE (T.L.P. Arc Entertainment Ltd.) |
| k-paper | Apache 2.0 + NOTICE |
| trading-system-ui | MIT |
| trading-system-showcase | Apache 2.0 + NOTICE |

## Known limitations

- Not production complete; not mainnet ready; Candidate #2 is not validated alpha.
- Root trading_system `Dockerfile` / `Dockerfile.cli` unsupported; use local cargo, `Dockerfile.paper-bin`, or `showcase/lab`.
- No GitHub Actions (owner decision).
- Research trees excluded from public export; remain in private original.
- Showcase `lab/` still builds from sibling source checkouts (no published images yet).
- Lost-ack showcase scenario remains limited (existing demo caveat).

## Proposed publish procedure (after approval)

1. Freeze SHAs on private `main`/`master`.
2. Run `scripts/public_export/prepare_public_export.sh` for trading_system; verify check + gitleaks.
3. Create **new** public repositories (reviewed export) with fresh git history from the export trees for trading_system and k-paper; push UI (MIT) as reviewed export or cleaned history after scan.
4. Push showcase (already public) including `lab/`.
5. Tag `v0.1.0` on public remotes only; do not retag over existing UI `v0.1.0` without a new version if needed (`v0.1.1`).
6. Verify clone + `lab` quickstart without private GitHub auth.
7. Keep private originals unchanged except continued development.

## Rollback

- Leave private remotes as source of truth.
- Delete or private-archive failed public remotes if a publish mistake occurs.
- Do not force-push cleaned history onto private originals.

## Post-release backlog

See Design Log / Increment 10 list (demand-driven only).
