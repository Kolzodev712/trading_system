# Contributing

Thank you for interest in this trading laboratory.

## Ground rules

- Prefer small, focused pull requests with tests for functional changes.
- Do not commit secrets, live credentials, personal `.env` files, or private operational configs.
- Do not claim production, mainnet, or validated-alpha readiness without matching evidence.
- **Research is public** under the current publication policy (`docs/CURRENT_PUBLICATION_POLICY.md`): the `research/` framework, existing candidates, and research logic are intentionally in this repository. Still do **not** commit proprietary third-party material, confidential research without permission, credentials, or restricted datasets.
- AI-assisted patches are welcome only if you reviewed and can explain the change. Large generated diffs without justification may be rejected.
- Financial correctness and execution safety take precedence over feature completeness.

## High-scrutiny areas

Open a discussion (or issue tagged `needs-design`) **before** changing:

- Risk management, execution authorization, order submission / acknowledgement handling
- Journaling, reconciliation, recovery
- Authentication, exchange credentials, financial accounting

## Getting started

1. Toolchain: see `rust-toolchain.toml` (currently `1.98.1`).
2. Fork and create a branch from `main`.
3. Pick an issue labeled `good first issue` or `help wanted`, or propose a change in an issue first.
4. Discuss non-trivial or architectural work before coding.

## Local verification (no GitHub Actions)

```bash
just fmt                 # or: cargo fmt --all
just clippy-strict       # or scoped clippy
just test-fast           # component + research unit gates
just test-5r-graph       # architectural dependency checks
# Paper (needs Mongo + sibling k-paper checkout):
just test-paper
```

Integrated lab (optional): https://github.com/Kolzodev712/trading-system-showcase/tree/main/lab

Report which commands you ran in the pull request.

## Pull requests

- Link the related issue.
- Summarize the problem, approach, and architectural impact.
- List verification commands and outcomes.
- Call out breaking changes and known limitations.

## Related projects

- https://github.com/Kolzodev712/k-paper — paper exchange simulator
- https://github.com/Kolzodev712/trading-system-ui — operator console
- https://github.com/Kolzodev712/trading-system-showcase — integrated lab and docs

## Reporting problems

- Bugs / features: GitHub Issues (use templates when available). Open contribution issues: https://github.com/Kolzodev712/trading_system/labels/good%20first%20issue
- Security: see [SECURITY.md](SECURITY.md) — use GitHub private vulnerability reporting; do not file public issues for vulnerabilities.
- Conduct: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

Contributions are accepted under the Apache License 2.0 (see `LICENSE` and `NOTICE`).
