# OSS release decisions (Increment 0 close-out)

## Problem / context

Owner decisions for the coordinated public open-source release preparation (see Increment 0 assessment). Needed before public/private separation, reviewed export, and relicensing.

## Decision

1. **Licensing:** `trading_system` and `k-paper` → **Apache License 2.0**. Copyright / NOTICE attribution: **T.L.P. Arc Entertainment Ltd.**
2. **`trading-system-ui`:** retain **MIT** (unchanged).
3. **`trading-system-showcase`:** add **Apache License 2.0** + NOTICE (docs/diagrams/presentation).
4. **History:** **reviewed public export** (new clean history for public remotes); keep original private repositories. Do not force-push cleaned history onto the private originals as the sole copy.
5. **Strategies:** keep **all live strategy / Candidate #2 composition public** for now (`btc_price_ma_app`, `strategy_core`, runner factories). No private strategy extraction required yet.
6. **Research archives:** keep **`research/candidates/*`, datasets, and research-only screens private** — omit from the public export.
7. **CI:** **no GitHub Actions**. Local `just` / documented commands only.
8. **Branches:** **`main` only** as the default integration branch for `trading_system` (remote `master` removed after `main` contained its tip + newer commits).
9. **Attribution:** ship NOTICE + README copyright lines with the Apache grant.

## Alternatives

- Full-history public remotes — rejected; secret/history risk too high without a confident scan of every revision.
- Private Candidate #2 app — deferred; owner prefers public examples while nothing is claimed as alpha.
- Apache on UI — rejected for now; MIT already in force.
- GitHub Actions in Inc 5 — rejected by owner preference.

## Consequences

- Public export tooling must **exclude** `research/` (and related artifacts) while **including** strategy composition examples.
- Relicense on the private trees is authorized now; **visibility change and new public remotes still require a later publish approval** after Inc 2 scan + Inc 9 report.
- No CI workflows will be added under this program unless the owner revisits decision 7.

## Verification

- LICENSE/NOTICE present and README/Cargo metadata match Apache-2.0 (trading_system, k-paper, showcase).
- GitHub default branch = `main`; remote `master` absent.
- Release tracker / assessment updated with locked decisions.

## Implementation results

- Applied Apache 2.0 LICENSE + NOTICE to `trading_system`, `k-paper`, `trading-system-showcase`; k-paper Cargo `license = "Apache-2.0"`; README/showcase copy updated.
- GitHub default branch set to `main`; remote `master` deleted (`main` was 6 commits ahead of former `master`).
- Local worktree still has a stale `master` ref at `/tmp/ts-cleanup` (`origin/master: gone`) — safe to remove later; not required for release.
- Prior mismatch fixes (false MIT README, Docker honesty, k-paper config packaging, UI main links) remain in the working trees pending commit/push.
