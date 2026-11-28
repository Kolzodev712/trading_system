# Security

## Supported versions

Security fixes apply to the latest `main` branch of this public repository. There is no separate sanitized export or private-development-only support channel.

Authoritative publication posture: [`docs/CURRENT_PUBLICATION_POLICY.md`](docs/CURRENT_PUBLICATION_POLICY.md) (full history is public; research is included; legacy reviewed-export tooling is not the active process).

## Reporting a vulnerability

Use **GitHub private vulnerability reporting** (Security Advisories) for this repository:

https://github.com/Kolzodev712/trading_system/security/advisories/new

Do not open public issues that include credentials, wallet keys, or customer data.

This project is laboratory / research software. Maintainers triage reports as capacity allows; there is no guaranteed response SLA.

## Secrets

- Never commit API keys, JWT secrets, or wallet private keys.
- Rotate any credential that may have appeared in local `config/` or historical commits.
- Historical findings remain an owner action: see the publication policy security posture section.

## Scope notes

- Default laboratory deployments are paper-only and must not reach live exchanges.
- Enabling `PAPER_LAB_FAULTS` exposes unauthenticated fault-injection HTTP routes; keep them off on any shared network interface.
