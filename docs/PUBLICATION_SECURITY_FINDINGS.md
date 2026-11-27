# Publication security findings — resolution status

**Date:** 2026-11-27  
**Scan:** `gitleaks detect` via `zricethezav/gitleaks:latest` against `/home/kolzo-dev/Desktop/trading_system` (477 commits).  
**Result:** 18 findings. **Secret values are not reproduced here.**

## Status legend

| Status | Meaning |
|--------|---------|
| OPEN_OWNER | Requires owner rotation/revocation confirmation; not verified locally |
| OPEN_REVIEW | Requires owner policy decision |
| MITIGATED | Technical control applied in-repo |
| N/A | Not applicable under current policy |

## Historical credential findings (CONFIRMED)

| Field | Value |
|-------|-------|
| Count | 18 |
| RuleID | `generic-api-key` (all) |
| Categories | Binance-related config/test paths (10); exchange config/test paths (8) |
| Paths (no values) | `src/exchange_engine/config/binance_config.json` (7); `src/trading_engine/config/exchange.json` (5); `src/trading_engine/src/bin/test_exchange_engine.rs` (2); `src/exchange_engine/src/bin/test_binance_rest_auth.rs` (2); `src/exchange_engine/src/exchanges/binance/websocket/types/common.rs` (1); `src/exchange_engine/src/bin/test_shit.rs` (1) |
| Resolution | **OWNER_CONFIRMED (2026-11-27)** — owner stated credentials are rotated |
| Actions not taken | No credential use attempted; history not rewritten (owner locked decision) |

### Owner confirmation

- **2026-11-27:** Owner confirmed historical credentials have been rotated. Release record may treat F01 as closed for publication purposes.
- Residual risk: old values remain in git history; rotation is the control (history rewrite declined).

## Research / datasets

| Item | Status |
|------|--------|
| Keep research framework + candidates + datasets in public tree | Locked owner decision — **do not remove** to satisfy old export manifest |
| Third-party dataset redistribution rights | **OWNER_CONFIRMED (2026-11-27)** — owner stated public research (including datasets) is OK |
| Confidentiality of research screens/ledgers | Covered by owner public-research confirmation unless a specific path is later flagged |

## Related non-credential publication controls (tracked elsewhere)

| Item | Tracker | Status |
|------|---------|--------|
| Lab path / staging hygiene | Stabilization 2 | open |
| Standalone port binds / auth defaults | Stabilization 5 | open |
| Release-policy doc consistency | Stabilization 1 | in progress |

## Local scan artifact

A machine-readable report may exist under `artifacts/public-export/gitleaks-history.json` (gitignored/untracked). **Do not commit it** — it contains secret material.
