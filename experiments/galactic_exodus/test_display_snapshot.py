from __future__ import annotations

import unittest

from experiments.galactic_exodus import display
from experiments.galactic_exodus.display_reference import (
    expected_lrs_dense_rift_snapshot,
    expected_lrs_display_snapshot,
    make_lrs_dense_rift_snapshot_state,
    make_lrs_display_snapshot_state,
)


class DisplaySnapshotTests(unittest.TestCase):
    def test_lrs_display_snapshot_matches_1076_baseline(self) -> None:
        rendered = display.render_lrs_border_light_map(make_lrs_display_snapshot_state())

        self.assertEqual(rendered, expected_lrs_display_snapshot())

    def test_lrs_dense_rift_snapshot_keeps_junctions_and_hides_unknown_rift(self) -> None:
        rendered = display.render_lrs_border_light_map(make_lrs_dense_rift_snapshot_state())
        body_rows = rendered.splitlines()[1:16:2]
        row_six = rendered.splitlines()[5]

        self.assertEqual(rendered, expected_lrs_dense_rift_snapshot())
        self.assertEqual([len(line) for line in body_rows], [35] * 8)
        self.assertEqual(row_six.count("|"), 2)


if __name__ == "__main__":
    unittest.main()
