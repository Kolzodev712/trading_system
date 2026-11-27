# OSS release preparation results (Increments 1–10)

> **SUPERSEDED IN PART (2026-11-27).** Prep results below are historical. Publication later flipped to full-history public remotes including research; see `docs/CURRENT_PUBLICATION_POLICY.md` and `docs/PUBLICATION_SECURITY_FINDINGS.md`. Reviewed-export tooling is legacy-only.

## Problem / context

Execute the open-source release program after Increment 0 decisions (#017), stopping before public visibility changes.

## Decision

Ship preparation artifacts on private remotes: Apache relicensing (already), public export tooling excluding research, secret-scan evidence, lab migration into showcase, repo standards docs, and a v0.1.0 release candidate report. No GitHub Actions. No publish.

## Implementation results

- **Inc 1:** `scripts/public_export/` manifest + `prepare_public_export.sh`; export omits `research/`; export `cargo check` of runner/trading_engine/gateway/C2/exchange/market_data PASS; no `research_*` in runner tree.
- **Inc 2:** gitleaks — export/UI/showcase/k-paper clean; trading_system **history** has 18 generic-api-key hits (reviewed export required). Licenses already Apache/MIT per #017.
- **Inc 3:** k-paper workspace check PASS; UI 73 tests PASS; root Docker remains obsolete (documented).
- **Inc 4:** `trading-system-showcase/lab/` migrated from private demo (interview guide / verification report omitted).
- **Inc 5:** `just test-paper` LAYER PASS; `trading_trace` sampling flake fixed; no CI added.
- **Inc 6–8:** CONTRIBUTING/SECURITY/CHANGELOG added on implementation repos + showcase; showcase README lab quickstart.
- **Inc 9:** `scripts/public_export/RELEASE_CANDIDATE.md` — stop before publish.
- **Inc 10:** `scripts/public_export/POST_RELEASE_BACKLOG.md` list only.

## Verification

See RELEASE_CANDIDATE.md tables. Paper: `paper-20261127T112116Z` LAYER PASS.

## Consequences

Owner must approve creation of public remotes / tags. Rotate any credentials that may have appeared in local or historical private history.
