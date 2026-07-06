from __future__ import annotations

import unittest

from experiments.galactic_exodus import display, engine, simulate
from experiments.galactic_exodus.display_reference import expected_lrs_display_snapshot, make_lrs_display_snapshot_state
from experiments.galactic_exodus.test_engine import filled_cells
from experiments.galactic_exodus.test_engine import make_actual_map
from experiments.galactic_exodus.test_engine import make_state


def make_snapshot_state() -> engine.GameState:
    return make_lrs_display_snapshot_state()


class DisplayTests(unittest.TestCase):
    def test_empty_known_map_shape(self) -> None:
        state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))

        rendered = display.render_lrs_border_light_map(state)
        lines = rendered.splitlines()

        self.assertEqual(len(lines), 18)
        self.assertEqual(lines[0], "  +---+---+---+---+---+---+---+---+")
        self.assertEqual(lines[16], "  +---+---+---+---+---+---+---+---+")
        self.assertEqual([line[0] for line in lines[1:16:2]], list("87654321"))
        self.assertEqual(lines[17], "    1   2   3   4   5   6   7   8")

    def test_current_position_uses_at_mark(self) -> None:
        state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))

        rendered = display.render_lrs_border_light_map(state)

        self.assertIn("1 | @   ?   ?   ?   ?   ?   ?   ? |", rendered)
        self.assertNotIn("P", rendered)

    def test_used_resource_uses_lowercase_r_only_when_known(self) -> None:
        actual_map = make_actual_map(
            cells=filled_cells("."),
            resource_positions=((2, 1), (3, 1)),
        )
        state = make_state(
            actual_map=actual_map,
            known_cells={
                (1, 1): "S",
                (2, 1): "R",
                (8, 8): "H",
            },
            used_resource_positions={(2, 1), (3, 1)},
        )

        rendered = display.render_lrs_border_light_map(state)

        self.assertIn("1 | @   r   ?   ?   ?   ?   ?   ? |", rendered)

    def test_vertical_known_rift_edge_is_rendered(self) -> None:
        state = make_snapshot_state()

        rendered = display.render_lrs_border_light_map(state)

        self.assertIn("6 | ?   .   R | .   ?   ?   ?   ? |", rendered)

    def test_horizontal_known_rift_edge_is_rendered(self) -> None:
        state = make_snapshot_state()

        rendered = display.render_lrs_border_light_map(state)

        self.assertIn("  +       +---+                   +", rendered)

    def test_actual_but_unknown_rift_is_hidden(self) -> None:
        hidden_edge = simulate.normalize_edge((3, 5), (3, 4))
        actual_map = make_actual_map(
            cells=filled_cells("."),
            rift_edges=(hidden_edge,),
        )
        state = make_state(actual_map=actual_map)

        rendered = display.render_lrs_border_light_map(state)

        self.assertNotIn("+---+", rendered.splitlines()[8])

    def test_known_open_route_is_not_rendered_as_blocker(self) -> None:
        open_edge = simulate.normalize_edge((1, 1), (2, 1))
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(open_edge,)),
            known_routes={open_edge: engine.ROUTE_OPEN},
        )

        rendered = display.render_lrs_border_light_map(state)

        self.assertIn("1 | @   ?   ?   ?   ?   ?   ?   ? |", rendered)
        self.assertEqual(rendered.splitlines()[15], "1 | @   ?   ?   ?   ?   ?   ?   ? |")

    def test_snapshot_matches_border_light_baseline(self) -> None:
        state = make_snapshot_state()

        rendered = display.render_lrs_border_light_map(state)

        self.assertEqual(rendered, expected_lrs_display_snapshot())


if __name__ == "__main__":
    unittest.main()
