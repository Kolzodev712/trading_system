#!/usr/bin/env bash
# LEGACY: Build a clean reviewed-export tree (no .git). Does not publish or rewrite history.
#
# SUPERSEDED as the publication process (2026-11-27). See docs/CURRENT_PUBLICATION_POLICY.md.
# Public remotes include full history and research/. This script strips research/ and must
# NOT be used to redefine what is public. Retained only as an optional archival/scan aid.
set -euo pipefail
echo "WARNING: prepare_public_export.sh is a LEGACY optional tool; it is not the current publication workflow." >&2
echo "See docs/CURRENT_PUBLICATION_POLICY.md" >&2

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="${1:-$ROOT/artifacts/public-export/trading_system}"
MANIFEST="$ROOT/scripts/public_export/EXPORT_MANIFEST.md"

if [[ ! -f "$MANIFEST" ]]; then
  echo "missing $MANIFEST" >&2
  exit 1
fi

rm -rf "$DEST"
mkdir -p "$DEST"

# Copy tracked-ish tree with excludes. Prefer git archive when available.
cd "$ROOT"
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git archive HEAD | tar -x -C "$DEST"
else
  echo "not a git repo; refusing blind copy" >&2
  exit 1
fi

# Drop private research and export-helper (re-add manifest only).
rm -rf "$DEST/research"
mkdir -p "$DEST/scripts/public_export"
cp "$MANIFEST" "$DEST/scripts/public_export/EXPORT_MANIFEST.md"
cp "$ROOT/scripts/public_export/prepare_public_export.sh" "$DEST/scripts/public_export/"

# Strip research workspace members from Cargo.toml
python3 - <<'PY' "$DEST/Cargo.toml"
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
# Remove research member lines
text2 = re.sub(r'\n\s*"research/research_engine",?', '\n', text)
text2 = re.sub(r'\n\s*"research/research_cli",?', '\n', text2)
# Collapse accidental blank double commas in members — normalize trailing commas later via cargo
path.write_text(text2)
print("rewrote", path)
PY

# Public justfile: drop research package targets from default test recipes.
python3 - <<'PY' "$DEST/justfile"
import pathlib, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "cargo test -p strategy_core -p research_engine -p research_cli -p trading_engine -p runner -p btc_price_ma_app --all-targets",
    "cargo test -p strategy_core -p trading_engine -p runner -p btc_price_ma_app --all-targets",
)
# Research isolation graph still forbids research_* in runner/trading_engine.
# Remove research script recipes that require private trees.
lines = []
skip = False
for line in text.splitlines(True):
    if line.startswith("research-") or line.startswith("research_"):
        skip = True
        continue
    if skip:
        if line.startswith(" ") or line.startswith("\t") or line.strip() == "":
            continue
        skip = False
    if "research_artifacts.py" in line or "research_git_guard.py" in line:
        continue
    lines.append(line)
path.write_text("".join(lines))
print("rewrote justfile")
PY

# Ensure LICENSE/NOTICE present
test -f "$DEST/LICENSE"
test -f "$DEST/NOTICE"
test ! -d "$DEST/research"

echo "export ready: $DEST"
echo "next: (cd $DEST && cargo check -p runner -p trading_engine -p btc_price_ma_app)"
