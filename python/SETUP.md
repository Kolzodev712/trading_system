# Python Workspace Setup

## Overview

This Python workspace uses `uv` for dependency management, similar to how Cargo works for Rust.

## Why `uv`?

1. **Fast**: Written in Rust, extremely fast dependency resolution
2. **Modern**: Uses PEP 621 (pyproject.toml) standard
3. **Workspace support**: Similar to Cargo workspaces
4. **Lock files**: Reproducible builds (uv.lock)
5. **Simple**: Single tool for virtualenv, dependency management, running scripts

## Installation

```bash
# Install uv
curl -LsSf https://astral.sh/uv/install.sh | sh

# Or via pip
pip install uv
```

## Quick Start

```bash
cd python

# Sync dependencies (creates .venv, installs packages)
uv sync

# Run a script
uv run indicator-generate --help

# Add a dependency
uv add package-name

# Update dependencies
uv lock --upgrade
```

## Structure

```
python/
├── pyproject.toml                    # Workspace root (like Cargo.toml)
├── uv.lock                          # Lock file (like Cargo.lock)
├── indicator_config_generator/      # Package 1
│   ├── pyproject.toml              # Package dependencies
│   └── src/                        # Package source
└── strategy_dsl/                    # Package 2
    ├── pyproject.toml              # Package dependencies
    └── src/                        # Package source
```

## Commands

### Dependency Management

```bash
# Add dependency to specific package
cd indicator_config_generator
uv add pydantic

# Add dependency to workspace root
cd ..
uv add --workspace some-package

# Remove dependency
uv remove package-name

# Update all dependencies
uv lock --upgrade
```

### Running Scripts

```bash
# Run script from package
uv run indicator-generate ...

# Run Python script directly
uv run python script.py

# Run with package context
uv run --package indicator-config-generator python script.py
```

### Development

```bash
# Install with dev dependencies
uv sync --dev

# Run tests
uv run pytest

# Format code
uv run ruff format .

# Lint code
uv run ruff check .

# Type check
uv run mypy .
```

## Comparison: Cargo vs uv

| Feature | Cargo (Rust) | uv (Python) |
|---------|-------------|-------------|
| Config file | `Cargo.toml` | `pyproject.toml` |
| Lock file | `Cargo.lock` | `uv.lock` |
| Workspace | `[workspace]` | `[tool.uv.workspace]` |
| Dependencies | `[dependencies]` | `[project.dependencies]` |
| Run scripts | `cargo run` | `uv run` |
| Add deps | `cargo add` | `uv add` |

## Migration from indicator_config_generator/

When ready to implement:

```bash
# Move README and any existing files
mv ../indicator_config_generator/README.md indicator_config_generator/README.md

# Start implementing
cd indicator_config_generator/src
# Create Python modules here
```

## Version Pinning

Dependencies are specified in `pyproject.toml`:

```toml
dependencies = [
    "pydantic>=2.0.0",      # Minimum version
    "pydantic~=2.0.0",      # Compatible release (>=2.0.0, <2.1.0)
    "pydantic==2.0.0",      # Exact version
]
```

The `uv.lock` file pins exact versions for reproducibility.

## CI/CD Integration

```yaml
# .github/workflows/python.yml
- name: Install uv
  run: pip install uv

- name: Install dependencies
  run: uv sync

- name: Run tests
  run: uv run pytest

- name: Lint
  run: uv run ruff check .
```

## Benefits

1. **Centralized**: All Python dependencies in one place
2. **Reproducible**: Lock file ensures same versions everywhere
3. **Fast**: uv is much faster than pip
4. **Modern**: Uses Python packaging standards
5. **Workspace**: Multiple packages, shared dependencies
6. **Simple**: One tool for everything

