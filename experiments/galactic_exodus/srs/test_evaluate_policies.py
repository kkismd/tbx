from __future__ import annotations

import json
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.evaluate_policies import (
    EvaluationCase,
    EvaluationCaseError,
    InitialRevealMode,
    RevisitMode,
    build_default_evaluation_cases,
    choose_exit_greedy_command,
    choose_known_target_step,
    EXIT_GREEDY_POLICY_NAME,
    first_known_route_step,
    is_known_passable_cell,
    iter_known_cardinal_neighbors,
    route_on_known_cells,
)
from experiments.galactic_exodus.srs.model import (
    CostMode,
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsCell,
    SrsCommand,
    SrsGameState,
    SrsObjectType,
    SrsTerrainType,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object, reveal_positions, replace_cell_terrain


REPO_ROOT = Path(__file__).resolve().parents[3]


def replace_cell_warp_flags(
    state: SrsGameState,
    position: Position,
    warp_flags: frozenset[Direction],
) -> SrsGameState:
    rows = [list(row) for row in state.actual_map.cells]
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=current.terrain,
        object_id=current.object_id,
        actor_id=current.actor_id,
        warp_flags=warp_flags,
    )
    actual_map = replace(
        state.actual_map,
        cells=tuple(tuple(row) for row in rows),
    )
    return replace(
        state,
        actual_map=actual_map,
        known_state=replace(
            state.known_state,
            known_cells={
                known_position: (
                    rows[known_position.y][known_position.x]
                    if known_position == position
                    else known_cell
                )
                for known_position, known_cell in state.known_state.known_cells.items()
            },
        ),
    )


class EvaluationCaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_metadata_is_stable_and_jsonable(self) -> None:
        case = EvaluationCase(
            case_id="normal-turn-only-first-visit",
            sector_id="normal-1001",
            sector_type=SectorType.NORMAL,
            sector_seed=1001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.N,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=0,
            max_fuel=0,
            initial_reveal_mode=InitialRevealMode.LOCAL_MOVEMENT,
            revisit_mode=RevisitMode.FIRST_VISIT,
        )

        metadata = dict(case.metadata())

        self.assertEqual(
            metadata,
            {
                "case_id": "normal-turn-only-first-visit",
                "sector_id": "normal-1001",
                "sector_type": "NORMAL",
                "sector_seed": 1001,
                "entry_edge": "S",
                "blocked_edges": [],
                "selected_exit_edge": "N",
                "cost_mode": "TURN_ONLY",
                "initial_fuel": 0,
                "max_fuel": 0,
                "initial_reveal_mode": "LOCAL_MOVEMENT",
                "revisit_mode": "FIRST_VISIT",
            },
        )
        self.assertNotIn(str(REPO_ROOT), json.dumps(metadata))

    def test_rejects_selected_exit_edge_that_is_blocked(self) -> None:
        with self.assertRaisesRegex(EvaluationCaseError, "selected_exit_edge"):
            EvaluationCase(
                case_id="rift-invalid-selected-exit",
                sector_id="rift-4001",
                sector_type=SectorType.RIFT,
                sector_seed=4001,
                entry_edge=Direction.S,
                blocked_edges=frozenset({Direction.N}),
                selected_exit_edge=Direction.N,
                cost_mode=CostMode.TURN_ONLY,
                initial_fuel=0,
                max_fuel=0,
                initial_reveal_mode=InitialRevealMode.NONE,
                revisit_mode=RevisitMode.FIRST_VISIT,
            )

    def test_build_sector_descriptor(self) -> None:
        case = build_default_evaluation_cases()[0]

        descriptor = case.build_sector_descriptor()

        self.assertEqual(
            descriptor,
            SectorDescriptor(
                sector_id="normal-1001",
                sector_type=SectorType.NORMAL,
                sector_seed=1001,
                entry_edge=Direction.S,
                blocked_edges=frozenset(),
            ),
        )

    def test_build_initial_state(self) -> None:
        case = build_default_evaluation_cases()[2]

        state = case.build_initial_state(contracts=self.contracts)

        self.assertIsInstance(state, SrsGameState)
        self.assertEqual(state.descriptor.sector_id, "resource-3001")
        self.assertEqual(state.fuel, 2)
        self.assertEqual(state.max_fuel, 9)
        self.assertEqual(len(state.known_state.discovered_cells), 81)

    def test_nebula_local_case_uses_3x3_observation(self) -> None:
        case = next(
            candidate
            for candidate in build_default_evaluation_cases()
            if candidate.case_id == "nebula-local-3x3-first-visit"
        )

        state = case.build_initial_state(contracts=self.contracts)

        self.assertEqual(state.actual_map.cell_at(state.player_position).terrain.value, "NEBULA")
        self.assertEqual(len(state.known_state.discovered_cells), 9)

    def test_revisit_case_restores_deterministic_persistent_state(self) -> None:
        case = next(
            candidate
            for candidate in build_default_evaluation_cases()
            if candidate.case_id == "resource-cache-revisit"
        )

        state = case.build_initial_state(contracts=self.contracts)

        self.assertEqual(state.fuel, 2)
        self.assertEqual(state.max_fuel, 9)
        self.assertEqual(state.known_state.visited_cells, frozenset())
        self.assertEqual(state.persistent_state.consumed_object_ids, frozenset({"resource-cache-1"}))
        self.assertTrue(state.objects["resource-cache-1"].consumed)


class DefaultEvaluationCasesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.cases = build_default_evaluation_cases()

    def test_default_cases_are_not_empty(self) -> None:
        self.assertTrue(self.cases)

    def test_default_cases_include_turn_only_and_shared_fuel(self) -> None:
        cost_modes = {case.cost_mode for case in self.cases}

        self.assertIn(CostMode.TURN_ONLY, cost_modes)
        self.assertIn(CostMode.SHARED_FUEL, cost_modes)

    def test_default_cases_cover_required_sector_categories(self) -> None:
        sector_types = {case.sector_type for case in self.cases}

        self.assertTrue(
            {SectorType.NORMAL, SectorType.RESOURCE, SectorType.BASE, SectorType.RIFT}.issubset(sector_types)
        )

    def test_default_cases_cover_first_visit_revisit_blocked_edge_and_multiple_exits(self) -> None:
        revisit_modes = {case.revisit_mode for case in self.cases}

        self.assertIn(RevisitMode.FIRST_VISIT, revisit_modes)
        self.assertTrue(any(case.revisit_mode is not RevisitMode.FIRST_VISIT for case in self.cases))
        self.assertTrue(any(case.blocked_edges for case in self.cases))
        self.assertTrue(any(len(case.open_exit_edges()) > 1 for case in self.cases))

    def test_default_case_ids_are_unique_and_stable(self) -> None:
        case_ids = [case.case_id for case in self.cases]

        self.assertEqual(len(case_ids), len(set(case_ids)))
        self.assertEqual(case_ids, [case.metadata()["case_id"] for case in self.cases])


class KnownStateRoutingHelperTests(unittest.TestCase):
    def test_known_passable_cell_uses_known_cells_only(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 8), Position(4, 7)])

        self.assertTrue(
            is_known_passable_cell(
                Position(4, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertFalse(
            is_known_passable_cell(
                Position(4, 6),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_known_passable_cell_rejects_impassable_terrain_and_celestial_objects(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 7), SrsTerrainType.ASTEROID)
        state = reveal_positions(state, [Position(4, 7), Position(4, 6), Position(4, 5), Position(5, 7), Position(5, 6)])
        state = place_object(state, Position(4, 6), SrsObjectType.STAR, "star-a")
        state = place_object(state, Position(4, 5), SrsObjectType.PLANET, "planet-a")
        state = place_object(state, Position(5, 7), SrsObjectType.STATION, "station-a")
        state = place_object(state, Position(5, 6), SrsObjectType.RESOURCE_CACHE, "resource-a")
        state = reveal_positions(state, [Position(4, 7), Position(4, 6), Position(4, 5), Position(5, 7), Position(5, 6)])

        self.assertFalse(
            is_known_passable_cell(
                Position(4, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertFalse(
            is_known_passable_cell(
                Position(4, 6),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertFalse(
            is_known_passable_cell(
                Position(4, 5),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertFalse(
            is_known_passable_cell(
                Position(5, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertTrue(
            is_known_passable_cell(
                Position(5, 6),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_salvage_is_treated_as_known_passable(self) -> None:
        state = place_object(make_state(), Position(4, 7), SrsObjectType.SALVAGE, "salvage-a")
        state = reveal_positions(state, [Position(4, 8), Position(4, 7)])

        self.assertTrue(
            is_known_passable_cell(
                Position(4, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_route_on_known_cells_does_not_cross_undiscovered_cells(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(5, 8), Position(5, 7), Position(5, 6)],
        )

        self.assertEqual(
            route_on_known_cells(
                Position(4, 8),
                Position(5, 6),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            ),
            (Direction.E, Direction.N, Direction.N),
        )
        self.assertIsNone(
            route_on_known_cells(
                Position(4, 8),
                Position(4, 6),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_first_known_route_step_uses_known_state_without_actual_map(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(4, 7), Position(4, 6)],
        )

        with patch(
            "experiments.galactic_exodus.srs.engine.route_to_known_target",
            side_effect=AssertionError("engine.route_to_known_target must not be used"),
        ):
            self.assertEqual(
                first_known_route_step(
                    Position(4, 8),
                    Position(4, 6),
                    known_cells=state.known_state.known_cells,
                    objects=state.objects,
                ),
                Direction.N,
            )

    def test_choose_known_target_step_applies_deterministic_tie_breaking(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(4, 7), Position(5, 8), Position(3, 8)],
        )

        self.assertEqual(
            choose_known_target_step(
                Position(4, 8),
                [Position(4, 7), Position(5, 8), Position(3, 8)],
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            ),
            (Position(4, 7), Direction.N),
        )
        self.assertEqual(
            choose_known_target_step(
                Position(4, 8),
                [Position(5, 8), Position(3, 8)],
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            ),
            (Position(3, 8), Direction.W),
        )

    def test_iter_known_cardinal_neighbors_is_stable(self) -> None:
        self.assertEqual(
            iter_known_cardinal_neighbors(Position(4, 8)),
            (
                (Direction.N, Position(4, 7)),
                (Direction.E, Position(5, 8)),
                (Direction.S, Position(4, 9)),
                (Direction.W, Position(3, 8)),
            ),
        )

    def test_route_on_known_cells_is_deterministic_for_same_input(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(4, 7), Position(5, 8), Position(5, 7)],
        )

        first = route_on_known_cells(
            Position(4, 8),
            Position(5, 7),
            known_cells=state.known_state.known_cells,
            objects=state.objects,
        )
        second = route_on_known_cells(
            Position(4, 8),
            Position(5, 7),
            known_cells=state.known_state.known_cells,
            objects=state.objects,
        )

        self.assertEqual(first, (Direction.N, Direction.E))
        self.assertEqual(first, second)


class ExitGreedyPolicyTests(unittest.TestCase):
    def test_policy_name_is_stable(self) -> None:
        self.assertEqual(EXIT_GREEDY_POLICY_NAME, "EXIT_GREEDY")

    def test_returns_warp_exit_when_current_cell_has_selected_exit(self) -> None:
        state = reveal_positions(make_state(entry_edge=Direction.S), [Position(4, 8)])

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.S)

        self.assertEqual(
            command,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
        )

    def test_returns_single_step_move_route_toward_nearest_known_warp_cell(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(5, 8), Position(6, 8)],
        )
        state = replace_cell_warp_flags(state, Position(6, 8), frozenset({Direction.N}))

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )
        self.assertEqual(command.route, (Direction.E,))

    def test_returns_no_action_when_selected_exit_warp_cell_is_undiscovered(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 8), Position(4, 7)])

        self.assertIsNone(
            choose_exit_greedy_command(state, selected_exit_edge=Direction.N)
        )

    def test_returns_no_action_when_known_warp_cell_is_unreachable(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(5, 8), Position(6, 8)],
        )
        state = replace_cell_terrain(state, Position(5, 8), SrsTerrainType.ASTEROID)
        state = reveal_positions(state, [Position(4, 8), Position(5, 8), Position(6, 8)])
        state = replace_cell_warp_flags(state, Position(6, 8), frozenset({Direction.N}))

        self.assertIsNone(
            choose_exit_greedy_command(state, selected_exit_edge=Direction.N)
        )

    def test_does_not_detour_to_objects_before_exit(self) -> None:
        state = reveal_positions(
            place_object(make_state(), Position(4, 7), SrsObjectType.SALVAGE, "salvage-a"),
            [Position(4, 8), Position(4, 7), Position(5, 8), Position(6, 8)],
        )
        state = replace_cell_warp_flags(state, Position(6, 8), frozenset({Direction.N}))

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )

    def test_never_returns_move_to(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(5, 8), Position(6, 8)],
        )
        state = replace_cell_warp_flags(state, Position(6, 8), frozenset({Direction.N}))

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertIsNotNone(command)
        self.assertNotEqual(command.command_type, "MOVE_TO")

    def test_is_deterministic_for_same_input(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(4, 7), Position(5, 8), Position(5, 7)],
        )
        state = replace_cell_warp_flags(state, Position(4, 7), frozenset({Direction.N}))
        state = replace_cell_warp_flags(state, Position(5, 8), frozenset({Direction.N}))

        first = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)
        second = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(first, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))
        self.assertEqual(first, second)

    def test_uses_known_state_without_actual_map(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(4, 8), Position(5, 8), Position(6, 8)],
        )
        state = replace_cell_warp_flags(state, Position(6, 8), frozenset({Direction.N}))

        with patch(
            "experiments.galactic_exodus.srs.model.SrsActualMap.cell_at",
            side_effect=AssertionError("actual_map must not be used"),
        ):
            self.assertEqual(
                choose_exit_greedy_command(state, selected_exit_edge=Direction.N),
                SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
            )


if __name__ == "__main__":
    unittest.main()
