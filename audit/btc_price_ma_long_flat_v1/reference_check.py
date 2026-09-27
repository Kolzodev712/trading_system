#!/usr/bin/env python3
"""Independent reference check for BTCPriceMALongFlatV1 (candidate #2).

Implement decision rebuild and ledger reconciliation per AUDIT_BRIEF.md.
This entry point runs reference unit tests and exits non-zero until the
full audit is implemented.

Reproduce from the repository root:

    python3 audit/btc_price_ma_long_flat_v1/reference_check.py
    python3 -m unittest discover -s audit/btc_price_ma_long_flat_v1/tests -v

Audited scenarios (when complete): zero-trading-cost diagnostic and Cost A
for primary N=28, diagnostic N=14/N=70, and fixed-long benchmark.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "data" / "btc_price_ma_long_flat_v1"
OUT = DATA / "audit" / "zero_cost_and_cost_a"
BRIEF = Path(__file__).resolve().parent / "AUDIT_BRIEF.md"


def run_reference_tests() -> unittest.result.TestResult:
    loader = unittest.TestLoader()
    suite = loader.discover(
        start_dir=str(Path(__file__).resolve().parent / "tests"),
        pattern="test_*.py",
    )
    runner = unittest.TextTestRunner(verbosity=2)
    return runner.run(suite)


def main() -> int:
    print(f"Brief: {BRIEF.relative_to(ROOT)}")
    print(f"Expected run data: {DATA.relative_to(ROOT)}")
    print(f"Audit output (when implemented): {OUT.relative_to(ROOT)}")
    print()

    result = run_reference_tests()
    if not result.wasSuccessful():
        return 1

    if not DATA.is_dir():
        print(
            "SKIP: full ledger audit — experiment data directory missing. "
            "Run btc_price_ma_experiment locally, then implement rebuild per brief."
        )
        return 2

    print(
        "TODO: implement decision rebuild + simulation compare "
        "(reuse Donchian reference_check simulation core where possible)."
    )
    print("See AUDIT_BRIEF.md sections 5–8.")
    return 2


if __name__ == "__main__":
    sys.exit(main())
