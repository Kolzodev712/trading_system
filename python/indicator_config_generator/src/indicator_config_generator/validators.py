"""Validation for indicator config files."""

from __future__ import annotations

from indicator_config_generator.models import (
    CompositeIndicatorEntry,
    IndicatorEntry,
    IndicatorsFile,
    SimpleIndicatorEntry,
)


def validate_indicators_file(file: IndicatorsFile) -> list[str]:
    """Return a list of error messages (empty if valid)."""
    errors: list[str] = []
    ids: set[str] = set()
    simple_ids: set[str] = set()

    for entry in file.indicators:
        entry_id = entry.id
        if entry_id in ids:
            errors.append(f"duplicate indicator id: {entry_id}")
        ids.add(entry_id)
        if isinstance(entry, SimpleIndicatorEntry):
            simple_ids.add(entry_id)

    for entry in file.indicators:
        if not isinstance(entry, CompositeIndicatorEntry):
            continue
        for dep in entry.dependencies:
            if dep not in simple_ids:
                errors.append(
                    f"composite {entry.id}: dependency {dep!r} is not a simple indicator in this file"
                )
        if entry.pair:
            for dep in entry.dependencies:
                if not dep.endswith(f"_{entry.pair}"):
                    errors.append(
                        f"composite {entry.id}: dependency {dep!r} pair mismatch (expected *_{entry.pair})"
                    )

    errors.extend(_detect_cycles(file.indicators))
    return errors


def _detect_cycles(indicators: list[IndicatorEntry]) -> list[str]:
    composites = [e for e in indicators if isinstance(e, CompositeIndicatorEntry)]
    if not composites:
        return []

    graph: dict[str, list[str]] = {c.id: list(c.dependencies) for c in composites}
    errors: list[str] = []
    visiting: set[str] = set()
    visited: set[str] = set()

    def dfs(node: str, path: list[str]) -> None:
        if node in visited:
            return
        if node in visiting:
            errors.append(f"circular dependency: {' -> '.join(path + [node])}")
            return
        visiting.add(node)
        for nxt in graph.get(node, []):
            if nxt in graph:
                dfs(nxt, path + [node])
        visiting.remove(node)
        visited.add(node)

    for cid in graph:
        dfs(cid, [])
    return errors
