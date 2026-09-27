"""Model reference tests — kept under audit/, not in the Rust crate."""

from __future__ import annotations

import unittest
from decimal import Decimal

import sys
from pathlib import Path

_PKG = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(_PKG))

from sources.model_reference import log_spread, wants_long  # noqa: E402


class ModelReferenceTests(unittest.TestCase):
    def test_equality_is_flat(self) -> None:
        closes = [Decimal("100")] * 3
        self.assertFalse(wants_long(closes))
        self.assertEqual(log_spread(closes), 0.0)

    def test_product_form_matches_log_spread(self) -> None:
        closes = [
            Decimal("100"),
            Decimal("110"),
            Decimal("120"),
            Decimal("130"),
        ]
        self.assertTrue(wants_long(closes))
        self.assertGreater(log_spread(closes), 0.0)


if __name__ == "__main__":
    unittest.main()
