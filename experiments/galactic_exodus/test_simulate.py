from __future__ import annotations

from pathlib import Path
import sys
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parent))

import simulate


def make_cells() -> dict[tuple[int, int], str]:
    cells: dict[tuple[int, int], str] = {
        (x, y): "."
        for y in range(1, simulate.HEIGHT + 1)
        for x in range(1, simulate.WIDTH + 1)
    }
    cells[simulate.SPECIAL_S] = "S"
    cells[simulate.SPECIAL_H] = "H"
    cells[(4, 4)] = "B"
    cells[(2, 1)] = "A"
    return cells


class GalacticExodusSimulationTests(unittest.TestCase):
    def test_shortest_route_uses_destination_costs(self) -> None:
        result = simulate.shortest_route(make_cells(), simulate.SPECIAL_S, simulate.SPECIAL_H)

        self.assertTrue(result.reachable)
        self.assertEqual(result.best_cost, 14)
        self.assertEqual(result.best_path_length, 14)
        self.assertEqual(result.path[0], simulate.SPECIAL_S)
        self.assertEqual(result.path[-1], simulate.SPECIAL_H)
        self.assertNotIn((2, 1), result.path)

    def test_route_summary_reports_base_costs(self) -> None:
        galactic_map = simulate.GalacticMap(
            seed=0,
            resource_count=0,
            b_position=(4, 4),
            r_positions=[],
            cells=make_cells(),
        )

        routes = simulate.route_summary(galactic_map)

        self.assertEqual(routes["S_H"].best_cost, 14)
        self.assertEqual(routes["S_H"].best_path_length, 14)
        self.assertEqual(routes["S_B"].best_cost, 6)
        self.assertEqual(routes["B_H"].best_cost, 8)
        self.assertEqual(routes["S_B"].best_cost + routes["B_H"].best_cost, 14)

    def test_format_output_includes_costs_and_path(self) -> None:
        galactic_map = simulate.GalacticMap(
            seed=7,
            resource_count=0,
            b_position=(4, 4),
            r_positions=[],
            cells=make_cells(),
        )

        output = simulate.format_output(galactic_map, show_path=True)

        self.assertIn("COSTS:", output)
        self.assertIn("reachable: yes", output)
        self.assertIn("best_cost: 14", output)
        self.assertIn("cost_to_base: 6", output)
        self.assertIn("cost_base_to_goal: 8", output)
        self.assertIn("best_cost_via_base: 14", output)
        self.assertIn("BEST PATH:", output)
        self.assertIn("(1,1) ->", output)


if __name__ == "__main__":
    unittest.main()
