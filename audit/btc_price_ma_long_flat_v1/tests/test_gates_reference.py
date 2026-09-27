"""Gate formula reference tests — kept under audit/, not in the Rust crate."""

from __future__ import annotations

import unittest
from decimal import Decimal

import sys
from pathlib import Path

_PKG = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(_PKG))

from sources.gates_reference import (  # noqa: E402
    BENCHMARK_COST_A_NET,
    max_primary_drawdown_gate,
    min_primary_net_for_retention_gate,
)


class GatesReferenceTests(unittest.TestCase):
    def test_preregistered_retention_threshold(self) -> None:
        self.assertEqual(
            min_primary_net_for_retention_gate(),
            Decimal("38.6884343791488"),
        )

    def test_preregistered_drawdown_cap(self) -> None:
        self.assertEqual(
            max_primary_drawdown_gate(),
            Decimal("31.07210057645617"),
        )

    def test_scaled_passive_matches_shipped_run_f(self) -> None:
        # f and scaled net from data/btc_price_ma_long_flat_v1/gates.json (Cost A run).
        primary_net = Decimal("20.6120701293059")
        f_recorded = Decimal("0.5448450809848760235550660848")
        scaled_recorded = Decimal("26.349003953106737096013977584")
        self.assertAlmostEqual(float(f_recorded), 0.544845080984876, places=12)
        self.assertGreater(scaled_recorded, primary_net)
        self.assertAlmostEqual(
            float(BENCHMARK_COST_A_NET * f_recorded),
            float(scaled_recorded),
            places=10,
        )


if __name__ == "__main__":
    unittest.main()
