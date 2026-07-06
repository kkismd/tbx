from __future__ import annotations

import unittest

from experiments.galactic_exodus.display_reference import (
    expected_srs_display_snapshot,
    expected_srs_symbol_contract_snapshot,
    make_srs_display_snapshot_state,
    make_srs_symbol_contract_snapshot_state,
)
from experiments.galactic_exodus.srs.render import render_display_map


class SrsDisplaySnapshotTests(unittest.TestCase):
    def test_srs_display_snapshot_matches_1076_baseline(self) -> None:
        rendered = render_display_map(make_srs_display_snapshot_state())

        self.assertEqual(rendered, expected_srs_display_snapshot())

    def test_srs_symbol_contract_snapshot(self) -> None:
        rendered = render_display_map(make_srs_symbol_contract_snapshot_state())

        self.assertEqual(rendered, expected_srs_symbol_contract_snapshot())


if __name__ == "__main__":
    unittest.main()
