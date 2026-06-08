import unittest

from experiments.galactic_exodus import simulate


def filled_cells(symbol: str = ".") -> simulate.Cells:
    return {
        (x, y): symbol
        for y in range(1, simulate.HEIGHT + 1)
        for x in range(1, simulate.WIDTH + 1)
    }


class TerrainCostTests(unittest.TestCase):
    def test_terrain_cost_table_and_unknown_symbol(self) -> None:
        self.assertEqual(simulate.terrain_cost("."), 1)
        self.assertEqual(simulate.terrain_cost("N"), 2)
        self.assertEqual(simulate.terrain_cost("A"), 3)
        self.assertEqual(simulate.terrain_cost("@"), 2)
        self.assertEqual(simulate.terrain_cost("B"), 1)
        self.assertEqual(simulate.terrain_cost("R"), 1)
        self.assertEqual(simulate.terrain_cost("S"), 0)
        self.assertEqual(simulate.terrain_cost("H"), 1)

        with self.assertRaisesRegex(ValueError, "unknown terrain symbol"):
            simulate.terrain_cost("?")


class NeighborTests(unittest.TestCase):
    def test_neighbors_for_corner_edge_and_center(self) -> None:
        self.assertEqual(simulate.neighbors((1, 1)), [(1, 2), (2, 1)])
        self.assertEqual(simulate.neighbors((1, 4)), [(1, 5), (2, 4), (1, 3)])
        self.assertEqual(
            simulate.neighbors((4, 4)),
            [(4, 5), (5, 4), (4, 3), (3, 4)],
        )


class ShortestPathTests(unittest.TestCase):
    def test_shortest_path_on_plain_grid_is_fourteen_cost_and_steps(self) -> None:
        cells = filled_cells(".")
        cells[simulate.SPECIAL_S] = "S"
        cells[simulate.SPECIAL_H] = "H"

        result = simulate.shortest_path(cells, simulate.SPECIAL_S, simulate.SPECIAL_H)

        self.assertEqual(result, simulate.PathResult(cost=14, steps=14))

    def test_shortest_path_avoids_high_cost_route(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        cells[(3, 1)] = "A"
        cells[(4, 1)] = "H"

        result = simulate.shortest_path(cells, (1, 1), (4, 1))

        self.assertEqual(result, simulate.PathResult(cost=5, steps=5))

    def test_shortest_path_prefers_fewer_steps_when_costs_are_equal(self) -> None:
        cells = filled_cells("A")
        cells.update(
            {
                (1, 1): ".",
                (2, 1): ".",
                (3, 1): ".",
                (1, 2): ".",
                (2, 2): ".",
                (3, 2): "S",
                (1, 3): "S",
                (2, 3): "S",
                (3, 3): "S",
            }
        )

        result = simulate.shortest_path(cells, (1, 1), (3, 1))

        self.assertEqual(result, simulate.PathResult(cost=2, steps=2))


class AnalysisAndOutputTests(unittest.TestCase):
    def test_seed_42_map_generation_is_unchanged(self) -> None:
        galactic_map = simulate.generate_map(42, 3)

        self.assertEqual(galactic_map.b_position, (4, 4))
        self.assertEqual(galactic_map.r_positions, [(5, 7), (3, 3), (3, 1)])
        self.assertEqual(
            simulate.render_map(galactic_map.cells),
            "\n".join(
                [
                    ". . @ N . . N H",
                    ". N N N R N . .",
                    "N @ A A . . A .",
                    ". . N A . . . N",
                    ". . . B . . . @",
                    "A N R . . . N .",
                    ". N . N N . . .",
                    "S . R N . . . .",
                ]
            ),
        )

    def test_analyze_paths_reports_consistent_costs(self) -> None:
        galactic_map = simulate.generate_map(42, 3)

        analysis = simulate.analyze_paths(galactic_map)

        self.assertTrue(analysis.reachable)
        self.assertEqual(analysis.best_cost, 15)
        self.assertEqual(analysis.best_path_length, 14)
        self.assertEqual(analysis.cost_to_base, 6)
        self.assertEqual(analysis.cost_base_to_goal, 9)
        self.assertEqual(analysis.best_cost_via_base, 15)
        self.assertEqual(analysis.best_cost_via_base, analysis.cost_to_base + analysis.cost_base_to_goal)

    def test_format_output_includes_costs_section(self) -> None:
        output = simulate.format_output(simulate.generate_map(42, 3))

        self.assertIn("COSTS:", output)
        self.assertIn("  reachable: yes", output)
        self.assertIn("  best_cost: 15", output)
        self.assertIn("  best_path_length: 14", output)
        self.assertIn("  cost_to_base: 6", output)
        self.assertIn("  cost_base_to_goal: 9", output)
        self.assertIn("  best_cost_via_base: 15", output)


if __name__ == "__main__":
    unittest.main()
