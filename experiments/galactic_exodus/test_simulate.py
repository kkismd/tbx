import unittest
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

from experiments.galactic_exodus import simulate


def filled_cells(symbol: str = ".") -> simulate.Cells:
    return {
        (x, y): symbol
        for y in range(1, simulate.HEIGHT + 1)
        for x in range(1, simulate.WIDTH + 1)
    }


def make_map(
    *,
    cells: simulate.Cells,
    b_position: simulate.Position = (4, 4),
    r_positions: list[simulate.Position] | None = None,
    rift_edges: tuple[simulate.Edge, ...] = (),
) -> simulate.GalacticMap:
    map_cells = dict(cells)
    map_cells[simulate.SPECIAL_S] = "S"
    map_cells[simulate.SPECIAL_H] = "H"
    map_cells[b_position] = "B"
    resource_positions = [] if r_positions is None else list(r_positions)
    for position in resource_positions:
        map_cells[position] = "R"
    return simulate.GalacticMap(
        seed=0,
        resource_count=len(resource_positions),
        rift_density=0.0,
        b_position=b_position,
        r_positions=resource_positions,
        rift_edges=rift_edges,
        cells=map_cells,
    )


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

    def test_plain_movement_cost_is_one_for_all_known_symbols_and_rejects_unknown(self) -> None:
        self.assertEqual(simulate.plain_movement_cost("."), 1)
        self.assertEqual(simulate.plain_movement_cost("N"), 1)
        self.assertEqual(simulate.plain_movement_cost("A"), 1)
        self.assertEqual(simulate.plain_movement_cost("@"), 1)
        self.assertEqual(simulate.plain_movement_cost("B"), 1)
        self.assertEqual(simulate.plain_movement_cost("R"), 1)
        self.assertEqual(simulate.plain_movement_cost("S"), 1)
        self.assertEqual(simulate.plain_movement_cost("H"), 1)

        with self.assertRaisesRegex(ValueError, "unknown terrain symbol"):
            simulate.plain_movement_cost("?")


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

    def test_shortest_path_can_use_plain_cost_function(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        cells[(3, 1)] = "A"
        cells[(4, 1)] = "H"

        result = simulate.shortest_path(
            cells,
            (1, 1),
            (4, 1),
            cost_function=simulate.plain_movement_cost,
        )

        self.assertEqual(result, simulate.PathResult(cost=3, steps=3))


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
        cells = filled_cells(".")

        first = simulate.sample_rift_edges(42, 0.10, cells)
        second = simulate.sample_rift_edges(42, 0.10, cells)

        self.assertEqual(first, second)
        self.assertEqual(len(first), 11)
        self.assertEqual(len(set(first)), 11)

    def test_eligible_rift_edges_excludes_edges_with_no_plain_endpoint(self) -> None:
        cells = filled_cells("N")
        cells[(1, 1)] = "."
        cells[(2, 2)] = "."

        eligible = set(simulate.eligible_rift_edges(cells))

        self.assertIn(simulate.normalize_edge((1, 1), (2, 1)), eligible)
        self.assertIn(simulate.normalize_edge((2, 2), (2, 3)), eligible)
        self.assertNotIn(simulate.normalize_edge((3, 2), (3, 3)), eligible)
        self.assertNotIn(simulate.normalize_edge((3, 3), (4, 3)), eligible)

    def test_eligible_rift_edges_include_plain_to_special_and_plain_to_plain_edges(self) -> None:
        cells = filled_cells("N")
        cells[(1, 1)] = "S"
        cells[(2, 1)] = "."
        cells[(2, 2)] = "."
        cells[(3, 2)] = "B"

        eligible = set(simulate.eligible_rift_edges(cells))

        self.assertIn(simulate.normalize_edge((1, 1), (2, 1)), eligible)
        self.assertIn(simulate.normalize_edge((2, 1), (2, 2)), eligible)
        self.assertIn(simulate.normalize_edge((2, 2), (3, 2)), eligible)

    def test_sample_rift_edges_only_selects_edges_with_plain_endpoint(self) -> None:
        galactic_map = simulate.generate_map(42, 3)

        for start, goal in galactic_map.rift_edges:
            self.assertTrue(
                galactic_map.cells[start] == "." or galactic_map.cells[goal] == ".",
                msg=f"rift edge must touch plain space: {(start, goal)}",
            )

    def test_sample_rift_edges_raises_when_eligible_edges_are_insufficient(self) -> None:
        cells = filled_cells("N")
        cells[(1, 1)] = "."

        with self.assertRaisesRegex(ValueError, "eligible rift edges"):
            simulate.sample_rift_edges(42, 0.10, cells)


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
        self.assertEqual(analysis.best_cost, 16)
        self.assertEqual(analysis.best_path_length, 14)
        self.assertEqual(analysis.cost_to_base, 7)
        self.assertEqual(analysis.cost_base_to_goal, 9)
        self.assertEqual(analysis.best_cost_via_base, 16)
        self.assertEqual(analysis.best_cost_via_base, analysis.cost_to_base + analysis.cost_base_to_goal)
        self.assertEqual(analysis.best_cost_without_base, 16)
        self.assertEqual(analysis.base_route_advantage_raw, 0)
        self.assertFalse(analysis.base_is_mandatory)
        self.assertEqual(simulate.classify_verdict(analysis), "ACCEPT")

    def test_cost_contributions_are_zero_on_plain_map_without_rifts(self) -> None:
        galactic_map = make_map(cells=filled_cells("."))

        analysis = simulate.analyze_cost_contributions(galactic_map)

        self.assertEqual(
            analysis,
            simulate.CostContributionAnalysis(
                plain_cost=14,
                terrain_only_cost=14,
                full_cost=14,
                terrain_extra_cost=0,
                rift_detour_cost=0,
            ),
        )

    def test_cost_contributions_report_positive_terrain_extra_cost(self) -> None:
        cells = filled_cells(".")
        for y in range(1, simulate.HEIGHT + 1):
            cells[(4, y)] = "A"
        galactic_map = make_map(cells=cells, b_position=(5, 4))

        analysis = simulate.analyze_cost_contributions(galactic_map)

        self.assertEqual(analysis.plain_cost, 14)
        self.assertEqual(analysis.terrain_only_cost, 16)
        self.assertEqual(analysis.full_cost, 16)
        self.assertEqual(analysis.terrain_extra_cost, 2)
        self.assertEqual(analysis.rift_detour_cost, 0)

    def test_cost_contributions_report_positive_rift_detour_cost(self) -> None:
        blocked_edges = (
            simulate.normalize_edge((1, 4), (2, 4)),
            simulate.normalize_edge((3, 4), (4, 4)),
            simulate.normalize_edge((3, 6), (3, 7)),
            simulate.normalize_edge((6, 6), (7, 6)),
            simulate.normalize_edge((6, 7), (7, 7)),
            simulate.normalize_edge((7, 1), (7, 2)),
            simulate.normalize_edge((7, 6), (7, 7)),
            simulate.normalize_edge((7, 8), (8, 8)),
            simulate.normalize_edge((8, 6), (8, 7)),
        )
        galactic_map = make_map(cells=filled_cells("."), rift_edges=blocked_edges)

        analysis = simulate.analyze_cost_contributions(galactic_map)

        self.assertEqual(analysis.plain_cost, 14)
        self.assertEqual(analysis.terrain_only_cost, 14)
        self.assertEqual(analysis.full_cost, 16)
        self.assertEqual(analysis.terrain_extra_cost, 0)
        self.assertEqual(analysis.rift_detour_cost, 2)

    def test_cost_contributions_report_zero_rift_detour_when_rifts_do_not_change_best_cost(self) -> None:
        blocked_edges = (
            simulate.normalize_edge((8, 1), (8, 2)),
        )
        galactic_map = make_map(cells=filled_cells("."), rift_edges=blocked_edges)

        analysis = simulate.analyze_cost_contributions(galactic_map)

        self.assertEqual(analysis.full_cost, 14)
        self.assertEqual(analysis.rift_detour_cost, 0)

    def test_cost_contributions_keep_plain_and_terrain_costs_when_full_route_is_unreachable(self) -> None:
        blocked_edges = (
            simulate.normalize_edge(simulate.SPECIAL_S, (2, 1)),
            simulate.normalize_edge(simulate.SPECIAL_S, (1, 2)),
        )
        galactic_map = make_map(cells=filled_cells("."), rift_edges=blocked_edges)

        analysis = simulate.analyze_cost_contributions(galactic_map)

        self.assertEqual(analysis.plain_cost, 14)
        self.assertEqual(analysis.terrain_only_cost, 14)
        self.assertIsNone(analysis.full_cost)
        self.assertEqual(analysis.terrain_extra_cost, 0)
        self.assertIsNone(analysis.rift_detour_cost)

    def test_seed_42_cost_contributions_are_stable(self) -> None:
        galactic_map = simulate.generate_map(42, 3)

        analysis = simulate.analyze_cost_contributions(galactic_map)

        self.assertEqual(
            analysis,
            simulate.CostContributionAnalysis(
                plain_cost=14,
                terrain_only_cost=15,
                full_cost=16,
                terrain_extra_cost=1,
                rift_detour_cost=1,
            ),
        )

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
        self.assertIn("\nFUEL PARAMETERS\n", output)
        self.assertIn("\nFUEL ANALYSIS\n", output)
        self.assertIn("\nMAP\n", output)
        self.assertIn("\nCOSTS\n", output)
        self.assertIn("\nCOST CONTRIBUTIONS\n", output)
        self.assertIn("\nVERDICT\n", output)
        self.assertIn("  map_id: seed-42-rift-0.10-res-3", output)
        self.assertIn("  B: (4,4)", output)
        self.assertIn("  rift_density: 0.10", output)
        self.assertIn("  initial_fuel: 16", output)
        self.assertIn("  base_supply: 8", output)
        self.assertIn("  resource_supply: 5", output)
        self.assertIn("  fuel_feasible_direct: yes", output)
        self.assertIn("  fuel_feasible_via_base: yes", output)
        self.assertIn("  fuel_feasible_via_resource: yes", output)
        self.assertIn("  S_to_H_cost: 16", output)
        self.assertIn("  S_to_H_steps: 14", output)
        self.assertIn("  S_to_B_cost: 7", output)
        self.assertIn("  B_to_H_cost: 9", output)
        self.assertIn("  S_to_H_via_B_cost: 16", output)
        self.assertIn("  S_to_H_without_B_cost: 16", output)
        self.assertIn("  base_is_mandatory: no", output)
        self.assertIn("  plain_cost: 14", output)
        self.assertIn("  terrain_only_cost: 15", output)
        self.assertIn("  full_cost: 16", output)
        self.assertIn("  terrain_extra_cost: 1", output)
        self.assertIn("  rift_detour_cost: 1", output)
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
        self.assertIn("  plain_cost: 14", output)
        self.assertIn("  terrain_only_cost: 14", output)
        self.assertIn("  full_cost: N/A", output)
        self.assertIn("  terrain_extra_cost: 0", output)
        self.assertIn("  rift_detour_cost: N/A", output)
        self.assertIn("  verdict: REJECT_TOO_HARD", output)

    def test_format_output_reports_fuel_sections_for_custom_parameters(self) -> None:
        galactic_map = make_map(cells=filled_cells("."), b_position=(2, 1))

        output = simulate.format_output(
            galactic_map,
            initial_fuel=1,
            base_supply=13,
            resource_supply=5,
        )

        self.assertIn("  initial_fuel: 1", output)
        self.assertIn("  base_supply: 13", output)
        self.assertIn("  resource_supply: 5", output)
        self.assertIn("  fuel_feasible_direct: no", output)
        self.assertIn("  fuel_feasible_via_base: yes", output)
        self.assertIn("  fuel_feasible_via_resource: no", output)
        self.assertIn("  remaining_fuel_direct: N/A", output)
        self.assertIn("  remaining_fuel_via_base: 0", output)
        self.assertIn("  remaining_fuel_via_resource: N/A", output)
        self.assertIn("  remaining_fuel_at_goal: 0", output)
        self.assertIn("  required_supply: 13", output)
        self.assertIn("  best_cost_via_resource: N/A", output)
        self.assertIn("  best_resource_position: N/A", output)


class FuelAnalysisTests(unittest.TestCase):
    def test_direct_route_can_arrive_with_exactly_zero_fuel(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells(".")),
            initial_fuel=14,
            base_supply=0,
            resource_supply=0,
        )

        self.assertTrue(analysis.fuel_feasible_direct)
        self.assertEqual(analysis.remaining_fuel_direct, 0)
        self.assertEqual(analysis.remaining_fuel_at_goal, 0)
        self.assertEqual(analysis.required_supply, 0)

    def test_base_route_can_resupply_after_arriving_with_zero_fuel(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells("."), b_position=(2, 1)),
            initial_fuel=1,
            base_supply=13,
            resource_supply=0,
        )

        self.assertFalse(analysis.fuel_feasible_direct)
        self.assertTrue(analysis.fuel_feasible_via_base)
        self.assertEqual(analysis.remaining_fuel_via_base, 0)
        self.assertEqual(analysis.remaining_fuel_at_goal, 0)
        self.assertEqual(analysis.required_supply, 13)

    def test_base_route_fails_when_initial_fuel_cannot_reach_base(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells("."), b_position=(4, 4)),
            initial_fuel=5,
            base_supply=100,
            resource_supply=0,
        )

        self.assertFalse(analysis.fuel_feasible_via_base)
        self.assertIsNone(analysis.remaining_fuel_via_base)

    def test_base_route_fails_when_post_supply_fuel_is_still_insufficient(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells("."), b_position=(2, 1)),
            initial_fuel=1,
            base_supply=12,
            resource_supply=0,
        )

        self.assertFalse(analysis.fuel_feasible_via_base)
        self.assertIsNone(analysis.remaining_fuel_via_base)

    def test_resource_route_is_feasible_when_any_one_resource_plan_works(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(
                cells=filled_cells("."),
                r_positions=[(2, 1), (8, 1)],
            ),
            initial_fuel=1,
            base_supply=0,
            resource_supply=13,
        )

        self.assertTrue(analysis.fuel_feasible_via_resource)
        self.assertEqual(analysis.remaining_fuel_via_resource, 0)

    def test_resource_remaining_fuel_uses_best_feasible_resource(self) -> None:
        cells = filled_cells(".")
        cells[(3, 1)] = "A"
        analysis = simulate.analyze_fuel(
            make_map(
                cells=cells,
                r_positions=[(2, 1), (4, 1)],
            ),
            initial_fuel=10,
            base_supply=0,
            resource_supply=10,
        )

        self.assertTrue(analysis.fuel_feasible_via_resource)
        self.assertEqual(analysis.remaining_fuel_via_resource, 6)

    def test_best_resource_cost_ignores_current_fuel_feasibility(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(
                cells=filled_cells("."),
                r_positions=[(2, 1)],
            ),
            initial_fuel=0,
            base_supply=0,
            resource_supply=0,
        )

        self.assertFalse(analysis.fuel_feasible_via_resource)
        self.assertEqual(analysis.best_cost_via_resource, 14)
        self.assertEqual(analysis.best_resource_position, (2, 1))

    def test_resource_count_zero_returns_stable_empty_resource_values(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells(".")),
            initial_fuel=27,
            base_supply=10,
            resource_supply=5,
        )

        self.assertFalse(analysis.fuel_feasible_via_resource)
        self.assertIsNone(analysis.remaining_fuel_via_resource)
        self.assertIsNone(analysis.best_cost_via_resource)
        self.assertIsNone(analysis.best_resource_position)

    def test_required_supply_is_zero_when_direct_route_is_feasible(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells("."), b_position=(2, 1)),
            initial_fuel=20,
            base_supply=0,
            resource_supply=0,
        )

        self.assertEqual(analysis.required_supply, 0)

    def test_required_supply_uses_reachable_supply_stop_when_direct_route_fails(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(cells=filled_cells("."), b_position=(2, 1)),
            initial_fuel=10,
            base_supply=0,
            resource_supply=0,
        )

        self.assertEqual(analysis.required_supply, 4)

    def test_required_supply_is_none_when_no_supply_stop_is_reachable_with_initial_fuel(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(
                cells=filled_cells("."),
                b_position=(4, 4),
                r_positions=[(8, 1)],
            ),
            initial_fuel=1,
            base_supply=0,
            resource_supply=0,
        )

        self.assertIsNone(analysis.required_supply)

    def test_unreachable_second_leg_is_treated_as_none_safely(self) -> None:
        blocked_edges = (
            simulate.normalize_edge(simulate.SPECIAL_S, (1, 2)),
            simulate.normalize_edge((2, 1), (3, 1)),
            simulate.normalize_edge((2, 1), (2, 2)),
        )
        analysis = simulate.analyze_fuel(
            make_map(
                cells=filled_cells("."),
                r_positions=[(2, 1)],
                rift_edges=blocked_edges,
            ),
            initial_fuel=10,
            base_supply=0,
            resource_supply=10,
        )

        self.assertFalse(analysis.fuel_feasible_via_resource)
        self.assertIsNone(analysis.remaining_fuel_via_resource)
        self.assertIsNone(analysis.best_cost_via_resource)
        self.assertIsNone(analysis.best_resource_position)

    def test_negative_inputs_are_rejected(self) -> None:
        galactic_map = make_map(cells=filled_cells("."))

        with self.assertRaisesRegex(ValueError, "initial-fuel"):
            simulate.analyze_fuel(galactic_map, initial_fuel=-1, base_supply=0, resource_supply=0)
        with self.assertRaisesRegex(ValueError, "base-supply"):
            simulate.analyze_fuel(galactic_map, initial_fuel=0, base_supply=-1, resource_supply=0)
        with self.assertRaisesRegex(ValueError, "resource-supply"):
            simulate.analyze_fuel(galactic_map, initial_fuel=0, base_supply=0, resource_supply=-1)

    def test_equal_cost_resource_choice_is_deterministic(self) -> None:
        analysis = simulate.analyze_fuel(
            make_map(
                cells=filled_cells("."),
                r_positions=[(2, 1), (1, 2)],
            ),
            initial_fuel=0,
            base_supply=0,
            resource_supply=0,
        )

        self.assertEqual(analysis.best_cost_via_resource, 14)
        self.assertEqual(analysis.best_resource_position, (1, 2))


class CliDefaultsTests(unittest.TestCase):
    def test_parse_args_uses_phase1_default_recommendations(self) -> None:
        with patch.object(sys, "argv", ["simulate.py"]):
            args = simulate.parse_args()

        self.assertEqual(args.resource_count, 3)
        self.assertEqual(args.rift_density, 0.10)
        self.assertEqual(args.initial_fuel, 16)
        self.assertEqual(args.base_supply, 8)
        self.assertEqual(args.resource_supply, 5)

    def test_parse_args_preserves_explicit_argument_values(self) -> None:
        with patch.object(
            sys,
            "argv",
            [
                "simulate.py",
                "--seed",
                "99",
                "--resource-count",
                "1",
                "--rift-density",
                "0.15",
                "--initial-fuel",
                "24",
                "--base-supply",
                "10",
                "--resource-supply",
                "6",
            ],
        ):
            args = simulate.parse_args()

        self.assertEqual(args.seed, 99)
        self.assertEqual(args.resource_count, 1)
        self.assertEqual(args.rift_density, 0.15)
        self.assertEqual(args.initial_fuel, 24)
        self.assertEqual(args.base_supply, 10)
        self.assertEqual(args.resource_supply, 6)


class ReadmeCommandTests(unittest.TestCase):
    def run_command(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, *args],
            check=True,
            capture_output=True,
            text=True,
            cwd=Path(__file__).resolve().parents[2],
        )

    def test_readme_standard_simulate_command_runs(self) -> None:
        result = self.run_command(
            "experiments/galactic_exodus/simulate.py",
            "--seed",
            "42",
            "--resource-count",
            "3",
            "--rift-density",
            "0.10",
            "--initial-fuel",
            "16",
            "--base-supply",
            "8",
            "--resource-supply",
            "5",
        )

        self.assertIn("MAP ID", result.stdout)
        self.assertIn("  initial_fuel: 16", result.stdout)
        self.assertIn("  base_supply: 8", result.stdout)

    def test_readme_standard_metrics_command_runs(self) -> None:
        result = self.run_command(
            "experiments/galactic_exodus/metrics.py",
            "--seed-start",
            "1",
            "--seed-count",
            "10",
            "--rift-density",
            "0.10",
            "--resource-count",
            "3",
        )

        self.assertIn("PHASE 0 METRICS", result.stdout)
        self.assertIn("rift_density: 0.10", result.stdout)

    def test_readme_standard_fuel_metrics_command_runs(self) -> None:
        csv_output = Path(".tmp/readme-fuel-metrics.csv")
        markdown_output = Path(".tmp/readme-fuel-metrics.md")
        self.addCleanup(csv_output.unlink, missing_ok=True)
        self.addCleanup(markdown_output.unlink, missing_ok=True)

        result = self.run_command(
            "experiments/galactic_exodus/fuel_metrics.py",
            "--seed-start",
            "1",
            "--seed-count",
            "10",
            "--rift-density",
            "0.10",
            "--initial-fuels",
            "14,16,18",
            "--base-supplies",
            "8,10",
            "--resource-supply",
            "5",
            "--resource-counts",
            "0,1,3",
            "--csv-output",
            str(csv_output),
            "--markdown-output",
            str(markdown_output),
        )

        self.assertIn("FUEL COMPARISON", result.stdout)
        self.assertTrue(csv_output.exists())
        self.assertTrue(markdown_output.exists())


if __name__ == "__main__":
    unittest.main()
