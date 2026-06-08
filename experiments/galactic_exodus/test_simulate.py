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

    def test_undirected_adjacent_edges_cover_full_grid_without_duplicates(self) -> None:
        edges = simulate.undirected_adjacent_edges()

        self.assertEqual(len(edges), simulate.TOTAL_UNDIRECTED_EDGES)
        self.assertEqual(len(set(edges)), simulate.TOTAL_UNDIRECTED_EDGES)
        self.assertEqual(edges[0], ((1, 1), (2, 1)))
        self.assertEqual(edges[1], ((1, 1), (1, 2)))


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

    def test_shortest_path_excludes_blocked_rift_edges(self) -> None:
        cells = filled_cells(".")
        cells[(4, 1)] = "H"
        blocked_edges = {
            simulate.normalize_edge((1, 1), (2, 1)),
            simulate.normalize_edge((2, 1), (3, 1)),
            simulate.normalize_edge((3, 1), (4, 1)),
        }

        result = simulate.shortest_path(cells, (1, 1), (4, 1), blocked_edges)

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

    def test_shortest_path_returns_none_when_rifts_cut_off_start(self) -> None:
        cells = filled_cells(".")
        blocked_edges = {
            simulate.normalize_edge((1, 1), (2, 1)),
            simulate.normalize_edge((1, 1), (1, 2)),
        }

        result = simulate.shortest_path(cells, (1, 1), (3, 1), blocked_edges)

        self.assertIsNone(result)

    def test_shortest_path_can_forbid_intermediate_nodes(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "B"
        cells[(3, 1)] = "H"

        result = simulate.shortest_path(cells, (1, 1), (3, 1), forbidden_nodes={(2, 1)})

        self.assertEqual(result, simulate.PathResult(cost=4, steps=4))


class RiftGenerationTests(unittest.TestCase):
    def test_rift_count_uses_total_undirected_edges(self) -> None:
        self.assertEqual(simulate.rift_count_for_density(0.0), 0)
        self.assertEqual(simulate.rift_count_for_density(0.10), 11)
        self.assertEqual(simulate.rift_count_for_density(0.50), 56)
        self.assertEqual(simulate.rift_count_for_density(1.0), simulate.TOTAL_UNDIRECTED_EDGES)

    def test_rift_density_validation_rejects_out_of_range_values(self) -> None:
        with self.assertRaisesRegex(ValueError, "rift-density"):
            simulate.rift_count_for_density(-0.01)
        with self.assertRaisesRegex(ValueError, "rift-density"):
            simulate.rift_count_for_density(1.01)

    def test_sample_rift_edges_is_deterministic_and_unique(self) -> None:
        first = simulate.sample_rift_edges(42, 0.10)
        second = simulate.sample_rift_edges(42, 0.10)

        self.assertEqual(first, second)
        self.assertEqual(len(first), 11)
        self.assertEqual(len(set(first)), 11)


class AnalysisAndOutputTests(unittest.TestCase):
    def test_seed_42_map_generation_is_unchanged(self) -> None:
        galactic_map = simulate.generate_map(42, 3)

        self.assertEqual(galactic_map.b_position, (4, 4))
        self.assertEqual(galactic_map.r_positions, [(5, 7), (3, 3), (3, 1)])
        self.assertEqual(galactic_map.rift_density, 0.10)
        self.assertEqual(len(galactic_map.rift_edges), 11)
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
        self.assertEqual(analysis.best_cost, 17)
        self.assertEqual(analysis.best_path_length, 14)
        self.assertEqual(analysis.cost_to_base, 8)
        self.assertEqual(analysis.cost_base_to_goal, 9)
        self.assertEqual(analysis.best_cost_via_base, 17)
        self.assertEqual(analysis.best_cost_via_base, analysis.cost_to_base + analysis.cost_base_to_goal)
        self.assertEqual(analysis.best_cost_without_base, 17)
        self.assertEqual(analysis.base_route_advantage_raw, 0)
        self.assertFalse(analysis.base_is_mandatory)
        self.assertEqual(simulate.classify_verdict(analysis), "ACCEPT")

    def test_analyze_paths_marks_base_as_mandatory_when_home_is_only_reachable_via_base(self) -> None:
        cells = filled_cells(".")
        b_position = (2, 1)
        blocked_edges = (
            simulate.normalize_edge(simulate.SPECIAL_S, (1, 2)),
            simulate.normalize_edge((2, 2), (3, 2)),
            simulate.normalize_edge((3, 1), (3, 2)),
        )
        cells[simulate.SPECIAL_S] = "S"
        cells[simulate.SPECIAL_H] = "H"
        cells[b_position] = "B"
        galactic_map = simulate.GalacticMap(
            seed=9,
            resource_count=0,
            rift_density=0.03,
            b_position=b_position,
            r_positions=[],
            rift_edges=blocked_edges,
            cells=cells,
        )

        analysis = simulate.analyze_paths(galactic_map)

        self.assertTrue(analysis.reachable)
        self.assertEqual(analysis.best_cost_via_base, analysis.cost_to_base + analysis.cost_base_to_goal)
        self.assertIsNone(analysis.best_cost_without_base)
        self.assertIsNone(analysis.base_route_advantage_raw)
        self.assertTrue(analysis.base_is_mandatory)
        self.assertEqual(simulate.classify_verdict(analysis), "REJECT_BASE_MANDATORY")

    def test_analyze_paths_rejects_too_hard_when_any_required_route_is_unreachable(self) -> None:
        cells = filled_cells(".")
        b_position = (2, 1)
        cells[simulate.SPECIAL_S] = "S"
        cells[simulate.SPECIAL_H] = "H"
        cells[b_position] = "B"
        galactic_map = simulate.GalacticMap(
            seed=11,
            resource_count=0,
            rift_density=0.04,
            b_position=b_position,
            r_positions=[],
            rift_edges=(
                simulate.normalize_edge(simulate.SPECIAL_S, (2, 1)),
                simulate.normalize_edge(simulate.SPECIAL_S, (1, 2)),
            ),
            cells=cells,
        )

        analysis = simulate.analyze_paths(galactic_map)

        self.assertFalse(analysis.reachable)
        self.assertIsNone(analysis.best_cost)
        self.assertIsNone(analysis.cost_to_base)
        self.assertEqual(analysis.cost_base_to_goal, 13)
        self.assertEqual(simulate.classify_verdict(analysis), "REJECT_TOO_HARD")

    def test_format_output_uses_required_sections_and_accept_verdict(self) -> None:
        output = simulate.format_output(simulate.generate_map(42, 3))

        self.assertIn("MAP ID\n", output)
        self.assertIn("\nOBJECTS\n", output)
        self.assertIn("\nPARAMETERS\n", output)
        self.assertIn("\nMAP\n", output)
        self.assertIn("\nCOSTS\n", output)
        self.assertIn("\nVERDICT\n", output)
        self.assertIn("  map_id: seed-42-rift-0.10-res-3", output)
        self.assertIn("  B: (4,4)", output)
        self.assertIn("  rift_density: 0.10", output)
        self.assertIn("  S_to_H_cost: 17", output)
        self.assertIn("  S_to_H_steps: 14", output)
        self.assertIn("  S_to_B_cost: 8", output)
        self.assertIn("  B_to_H_cost: 9", output)
        self.assertIn("  S_to_H_via_B_cost: 17", output)
        self.assertIn("  S_to_H_without_B_cost: 17", output)
        self.assertIn("  base_is_mandatory: no", output)
        self.assertIn("  verdict: ACCEPT", output)
        self.assertIn("  priority_1: REJECT_TOO_HARD", output)
        self.assertIn("  priority_2: REJECT_BASE_MANDATORY", output)
        self.assertIn("  priority_3: ACCEPT", output)
        self.assertIn("  note: ACCEPT is a minimal candidate verdict, not a final fun/balance judgment.", output)

    def test_format_output_marks_mandatory_base_with_yes(self) -> None:
        cells = filled_cells(".")
        b_position = (2, 1)
        blocked_edges = (
            simulate.normalize_edge(simulate.SPECIAL_S, (1, 2)),
            simulate.normalize_edge((2, 2), (3, 2)),
            simulate.normalize_edge((3, 1), (3, 2)),
        )
        cells[simulate.SPECIAL_S] = "S"
        cells[simulate.SPECIAL_H] = "H"
        cells[b_position] = "B"
        galactic_map = simulate.GalacticMap(
            seed=9,
            resource_count=0,
            rift_density=0.03,
            b_position=b_position,
            r_positions=[],
            rift_edges=blocked_edges,
            cells=cells,
        )

        output = simulate.format_output(galactic_map)

        self.assertIn("  base_is_mandatory: yes", output)
        self.assertIn("  verdict: REJECT_BASE_MANDATORY", output)

    def test_format_output_uses_na_for_unreachable_segments(self) -> None:
        cells = filled_cells(".")
        cells[simulate.SPECIAL_S] = "S"
        cells[simulate.SPECIAL_H] = "H"
        cells[(2, 1)] = "B"
        galactic_map = simulate.GalacticMap(
            seed=7,
            resource_count=0,
            rift_density=0.10,
            b_position=(2, 1),
            r_positions=[],
            rift_edges=(
                simulate.normalize_edge(simulate.SPECIAL_S, (2, 1)),
                simulate.normalize_edge(simulate.SPECIAL_S, (1, 2)),
                simulate.normalize_edge((2, 1), (3, 1)),
                simulate.normalize_edge((2, 1), (2, 2)),
            ),
            cells=cells,
        )

        output = simulate.format_output(galactic_map)

        self.assertIn("  S_to_H_cost: N/A", output)
        self.assertIn("  S_to_H_steps: N/A", output)
        self.assertIn("  S_to_B_cost: N/A", output)
        self.assertIn("  B_to_H_cost: N/A", output)
        self.assertIn("  S_to_H_via_B_cost: N/A", output)
        self.assertIn("  S_to_H_without_B_cost: N/A", output)
        self.assertIn("  base_route_advantage_raw: N/A", output)
        self.assertIn("  base_is_mandatory: no", output)
        self.assertIn("  verdict: REJECT_TOO_HARD", output)


if __name__ == "__main__":
    unittest.main()
