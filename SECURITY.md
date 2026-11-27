# Security

## Supported versions

This laboratory software is not a hosted service. Security fixes apply to the latest
`main` branch of the private development repository and to reviewed public export tags
when those exist.

## Reporting a vulnerability

Please report suspected vulnerabilities privately to the maintainers (repository
owner on GitHub). Do not open public issues that include credentials, wallet keys,
or customer data.

## Secrets

- Never commit API keys, JWT secrets, or wallet private keys.
- Rotate any credential that may have appeared in local `config/` or historical commits.
- A reviewed public export is used because historical commits may contain credential-shaped material even when `HEAD` is clean.

## Scope notes

- Default laboratory deployments are paper-only and must not reach live exchanges.
- Enabling `PAPER_LAB_FAULTS` exposes unauthenticated fault-injection HTTP routes; keep them off on any shared network interface.
