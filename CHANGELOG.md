# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

## [0.1.1] — 2026-11-27

### Changed

- Full-history public remotes including `research/`; reviewed-export tooling marked legacy (`docs/CURRENT_PUBLICATION_POLICY.md`).
- Public onboarding README; exchange capability matrix; strategy guide refresh.
- Owner confirmed credential rotation and public research/dataset redistribution for this release.

### Fixed

- `.dockerignore` no longer excludes `secret.rs` via overly broad `*secret*` patterns.
- README no longer claims nonexistent GitHub Actions; points at local `just` verification.

### Notes

- Root `Dockerfile` / `Dockerfile.cli` remain obsolete; supported containers live in `trading-system-showcase/lab`.
