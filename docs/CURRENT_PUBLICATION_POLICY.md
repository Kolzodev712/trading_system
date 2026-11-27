# Current publication policy (authoritative)

**Effective:** 2026-11-27  
**Supersedes:** reviewed-export-as-publication-process described in Design Log #017 decision 4/6, Design Log #018, `scripts/public_export/EXPORT_MANIFEST.md`, and the pre-flip sections of `RELEASE_CANDIDATE.md`.

Those documents remain **historical records**. They must not be used as the active release workflow.

## What is public

| Repository | Visibility | License | History | Research / strategies |
|------------|------------|---------|---------|------------------------|
| `trading_system` | public | Apache 2.0 | full git history | **included** (framework, candidates, datasets as present in tree) |
| `k-paper` | public | Apache 2.0 | full git history | n/a |
| `trading-system-ui` | public | MIT | full git history | n/a |
| `trading-system-showcase` | public | Apache 2.0 | full git history | lab + docs |
| `trading-system-showcase-demo` | **private** | — | — | demonstration source only |

## Explicit non-claims

- Not production complete; not mainnet qualified.
- Research candidates are **not** validated alpha.
- No GitHub Actions; verification is local / documented commands only.
- Showcase is the ecosystem integration and distribution repository.

## Active release workflow

1. Develop on the public repositories (or private forks if desired).
2. Pin compatible revisions in the showcase compatibility manifest (Stabilization 2+).
3. Qualify locally (showcase `make qualify` + component `just` / npm procedures).
4. Prepare release notes and tags **only after owner authorization**.
5. Do **not** rewrite history, delete remotes, or remove existing releases as part of normal release.

## Legacy: reviewed export tooling

`scripts/public_export/prepare_public_export.sh` and `EXPORT_MANIFEST.md` describe an **optional legacy** mechanism that builds a research-stripped tree. That mechanism:

- is **not** the current publication process;
- must **not** be used to strip research from the public remotes;
- may remain for archival / differential scanning experiments only.

## Security posture (publication)

Historical `gitleaks` findings in `trading_system` git history remain a **open owner action**: rotate/revoke any credentials that may have been real. Stabilization does not treat rotation as verified until the owner confirms.

Third-party dataset redistribution rights are a separate owner review item; do not delete research trees to “fix” redistribution uncertainty.
