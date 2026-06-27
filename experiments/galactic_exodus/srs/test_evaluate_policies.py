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
    EXPLORE_THEN_EXIT_POLICY_NAME,
    EXIT_GREEDY_POLICY_NAME,
    InitialRevealMode,
    OBJECT_GREEDY_POLICY_NAME,
    PolicyRunResult,
    PolicyRunOutcome,
    RevisitMode,
    build_default_evaluation_cases,
    build_object_greedy_candidates,
    choose_explore_then_exit_command,
    choose_exit_greedy_command,
    choose_known_target_step,
    choose_object_greedy_command,
    first_known_route_step,
    is_known_passable_cell,
    iter_known_cardinal_neighbors,
    route_on_known_cells,
    run_policy_evaluation_case,
    summarize_policy_runs,
)
from experiments.galactic_exodus.srs.log import (
    INTERACT_ACCEPTED,
    INTERACT_REJECTED,
    MOVE_REJECTED,
    OBJECT_CONSUMED,
    STATION_ACTIVATED,
    WARP_EXIT_ACCEPTED,
    WARP_EXIT_REJECTED,
    make_turn_event,
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


def place_object_with_metadata(
    state: SrsGameState,
    position: Position,
    object_type: SrsObjectType,
    object_id: str,
    *,
    fuel_restore: int | None = None,
) -> SrsGameState:
    state = place_object(state, position, object_type, object_id)
    if fuel_restore is None:
        return state
    return replace(
        state,
        objects={
            **state.objects,
            object_id: replace(state.objects[object_id], metadata={"fuel_restore": fuel_restore}),
        },
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


class ObjectGreedyPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_policy_name_is_stable(self) -> None:
        self.assertEqual(OBJECT_GREEDY_POLICY_NAME, "OBJECT_GREEDY")

    def test_prioritizes_known_unconsumed_resource_cache(self) -> None:
        state = place_object_with_metadata(
            make_state(fuel=2, max_fuel=9),
            Position(4, 8),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = place_object(state, Position(5, 8), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(4, 8), Position(5, 8)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
        )

    def test_resource_cache_is_excluded_at_full_fuel(self) -> None:
        state = place_object_with_metadata(
            make_state(fuel=9, max_fuel=9),
            Position(4, 8),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(4, 8), Position(4, 7), Position(4, 6)])
        state = replace_cell_warp_flags(state, Position(4, 6), frozenset({Direction.N}))

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
        )

    def test_known_unactivated_station_is_a_candidate(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")
        state = reveal_positions(state, [Position(4, 8), Position(4, 7)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
        )

    def test_station_is_excluded_at_full_fuel(self) -> None:
        state = place_object(make_state(fuel=9, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")
        state = reveal_positions(state, [Position(4, 8), Position(4, 7), Position(5, 8)])
        state = replace_cell_warp_flags(state, Position(5, 8), frozenset({Direction.N}))

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )

    def test_known_unconsumed_salvage_is_a_candidate(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 8), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(4, 8), Position(5, 8)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )

    def test_returns_interact_when_in_interaction_range(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")
        state = reveal_positions(state, [Position(4, 8), Position(4, 7), Position(5, 8)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
        )

    def test_returns_single_step_move_route_when_out_of_range(self) -> None:
        state = place_object_with_metadata(
            make_state(fuel=2, max_fuel=9),
            Position(4, 6),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(4, 8), Position(4, 7), Position(4, 6)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
        )
        self.assertEqual(command.route, (Direction.N,))

    def test_falls_back_to_exit_greedy_when_no_object_candidates_exist(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 8), Position(4, 7), Position(4, 6)])
        state = replace_cell_warp_flags(state, Position(4, 6), frozenset({Direction.N}))

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
        )

    def test_rejected_object_is_not_retried(self) -> None:
        state = place_object_with_metadata(
            make_state(fuel=2, max_fuel=9),
            Position(4, 8),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = place_object(state, Position(5, 8), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(4, 8), Position(5, 8)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
            rejected_object_ids={"resource-cache-1"},
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )

    def test_build_candidates_uses_known_cells_only(self) -> None:
        state = place_object_with_metadata(
            make_state(fuel=2, max_fuel=9),
            Position(4, 6),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(4, 8), Position(4, 7)])

        candidates = build_object_greedy_candidates(
            state,
            contracts=self.contracts,
        )

        self.assertEqual(candidates, ())

    def test_never_returns_move_to(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 8), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(4, 8), Position(5, 8)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertIsNotNone(command)
        self.assertNotEqual(command.command_type, "MOVE_TO")

    def test_is_deterministic_for_same_input(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(3, 8), SrsObjectType.SALVAGE, "salvage-a")
        state = place_object(state, Position(5, 8), SrsObjectType.SALVAGE, "salvage-b")
        state = reveal_positions(state, [Position(4, 8), Position(3, 8), Position(5, 8)])

        first = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )
        second = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(first, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.W,)))
        self.assertEqual(first, second)

    def test_uses_known_state_without_actual_map(self) -> None:
        state = place_object_with_metadata(
            make_state(fuel=2, max_fuel=9),
            Position(4, 6),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(4, 8), Position(4, 7), Position(4, 6)])

        with patch(
            "experiments.galactic_exodus.srs.model.SrsActualMap.cell_at",
            side_effect=AssertionError("actual_map must not be used"),
        ):
            self.assertEqual(
                choose_object_greedy_command(
                    state,
                    contracts=self.contracts,
                    selected_exit_edge=Direction.N,
                ),
                SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            )


class ExploreThenExitPolicyTests(unittest.TestCase):
    def test_policy_name_is_stable(self) -> None:
        self.assertEqual(EXPLORE_THEN_EXIT_POLICY_NAME, "EXPLORE_THEN_EXIT")

    def test_returns_single_step_move_route_toward_selected_frontier(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 3),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
                Position(4, 2),
                Position(3, 3),
                Position(5, 3),
            ],
        )
        state = replace_cell_terrain(state, Position(3, 4), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(5, 4), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(4, 5), SrsTerrainType.ASTEROID)
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 3),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
                Position(4, 2),
                Position(3, 3),
                Position(5, 3),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))

    def test_steps_into_unknown_when_current_position_is_frontier(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))

    def test_selected_exit_edge_breaks_unknown_direction_ties(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 5),
                Position(3, 4),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.E)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)))

    def test_falls_back_to_exit_greedy_after_max_explore_steps(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4), srs_turn=12)
        state = reveal_positions(
            state,
            [Position(x, y) for y in range(9) for x in range(9)],
        )
        state = replace_cell_warp_flags(state, Position(6, 4), frozenset({Direction.N}))

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)))

    def test_falls_back_to_exit_greedy_when_no_frontier_exists(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [Position(x, y) for y in range(9) for x in range(9)],
        )
        state = replace_cell_warp_flags(state, Position(6, 4), frozenset({Direction.N}))

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)))

    def test_returns_no_action_when_frontier_and_exit_are_unreachable(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 3),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
                Position(6, 4),
            ],
        )
        state = replace_cell_terrain(state, Position(4, 3), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(4, 5), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(3, 4), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(5, 4), SrsTerrainType.ASTEROID)
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 3),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
                Position(6, 4),
            ],
        )

        self.assertIsNone(
            choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)
        )

    def test_never_returns_interact_or_move_to(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertIsNotNone(command)
        self.assertNotIn(command.command_type, {"INTERACT", "MOVE_TO"})

    def test_uses_known_state_without_reading_unknown_cell_contents(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
            ],
        )

        with patch(
            "experiments.galactic_exodus.srs.model.SrsActualMap.cell_at",
            side_effect=AssertionError("actual_map.cell_at must not be used"),
        ):
            self.assertEqual(
                choose_explore_then_exit_command(state, selected_exit_edge=Direction.N),
                SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            )

    def test_is_deterministic_for_same_input(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(
            state,
            [
                Position(4, 4),
                Position(4, 5),
                Position(3, 4),
                Position(5, 4),
            ],
        )

        first = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)
        second = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(first, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))
        self.assertEqual(first, second)


class PolicyRunLoopTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def make_case(self, *, selected_exit_edge: Direction = Direction.S) -> EvaluationCase:
        return EvaluationCase(
            case_id=f"run-loop-{selected_exit_edge.value.lower()}",
            sector_id="normal-1001",
            sector_type=SectorType.NORMAL,
            sector_seed=1001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=selected_exit_edge,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=0,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.NONE,
            revisit_mode=RevisitMode.FIRST_VISIT,
        )

    def make_exit_ready_state(self) -> SrsGameState:
        return reveal_positions(make_state(entry_edge=Direction.S), [Position(4, 8)])

    def test_classifies_warp_exit_accepted_as_exited(self) -> None:
        case = self.make_case(selected_exit_edge=Direction.S)

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            result = run_policy_evaluation_case(
                case,
                EXIT_GREEDY_POLICY_NAME,
                contracts=self.contracts,
            )

        self.assertEqual(result.outcome, PolicyRunOutcome.EXITED)
        self.assertEqual(result.command_count, 1)
        self.assertEqual(result.action_sequence, (SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),))
        self.assertEqual(result.event_log[0].event_type, WARP_EXIT_ACCEPTED)

    def test_policy_returning_no_action_aborts_run(self) -> None:
        case = self.make_case()

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                return_value=None,
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.ABORTED_NO_POLICY_ACTION)
        self.assertEqual(result.command_count, 0)
        self.assertEqual(result.event_log, ())

    def test_max_srs_turn_aborts_before_querying_policy(self) -> None:
        case = self.make_case()
        state = replace(self.make_exit_ready_state(), srs_turn=3)

        with patch.object(EvaluationCase, "build_initial_state", return_value=state):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                side_effect=AssertionError("policy should not be queried"),
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                    max_srs_turn=3,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.ABORTED_TURN_LIMIT)
        self.assertEqual(result.command_count, 0)

    def test_max_commands_aborts_after_reaching_command_limit(self) -> None:
        case = self.make_case()
        state = replace(make_state(), player_position=Position(4, 4))
        state = reveal_positions(state, [Position(4, 4), Position(4, 3)])

        with patch.object(EvaluationCase, "build_initial_state", return_value=state):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                return_value=SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                    max_commands=1,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.ABORTED_TURN_LIMIT)
        self.assertEqual(result.command_count, 1)
        self.assertEqual(result.action_sequence, (SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),))

    def test_warp_exit_rejected_does_not_end_run(self) -> None:
        case = self.make_case(selected_exit_edge=Direction.S)

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                side_effect=[
                    SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.N),
                    SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
                ],
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.EXITED)
        self.assertEqual(result.command_count, 2)
        self.assertEqual(result.event_log[0].event_type, WARP_EXIT_REJECTED)
        self.assertEqual(result.event_log[-1].event_type, WARP_EXIT_ACCEPTED)

    def test_move_rejected_does_not_end_run(self) -> None:
        case = self.make_case(selected_exit_edge=Direction.S)

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                side_effect=[
                    SrsCommand(command_type="MOVE_TO", target=Position(0, 0)),
                    SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
                ],
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.EXITED)
        self.assertEqual(result.event_log[0].event_type, MOVE_REJECTED)
        self.assertEqual(result.event_log[-1].event_type, WARP_EXIT_ACCEPTED)

    def test_interact_rejected_does_not_end_run(self) -> None:
        case = self.make_case(selected_exit_edge=Direction.S)

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                side_effect=[
                    SrsCommand(command_type="INTERACT", target_object_id="missing-object"),
                    SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
                ],
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.EXITED)
        self.assertEqual(result.event_log[0].event_type, INTERACT_REJECTED)
        self.assertEqual(result.event_log[-1].event_type, WARP_EXIT_ACCEPTED)

    def test_repeated_invalid_action_is_suppressed_as_no_policy_action(self) -> None:
        case = self.make_case()
        repeated_command = SrsCommand(command_type="MOVE_TO", target=Position(0, 0))

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                side_effect=[repeated_command, repeated_command],
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                )

        self.assertEqual(result.outcome, PolicyRunOutcome.ABORTED_NO_POLICY_ACTION)
        self.assertEqual(result.command_count, 1)
        self.assertEqual(result.event_log[0].event_type, MOVE_REJECTED)
        self.assertEqual(result.action_sequence, (repeated_command,))

    def test_action_sequence_is_recorded_in_order(self) -> None:
        case = self.make_case(selected_exit_edge=Direction.S)
        first = SrsCommand(command_type="MOVE_TO", target=Position(0, 0))
        second = SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S)

        with patch.object(EvaluationCase, "build_initial_state", return_value=self.make_exit_ready_state()):
            with patch(
                "experiments.galactic_exodus.srs.evaluate_policies.choose_policy_command",
                side_effect=[first, second],
            ):
                result = run_policy_evaluation_case(
                    case,
                    EXIT_GREEDY_POLICY_NAME,
                    contracts=self.contracts,
                )

        self.assertEqual(result.action_sequence, (first, second))
        self.assertEqual(result.command_count, 2)

    def test_same_input_produces_same_run_result(self) -> None:
        case = self.make_case(selected_exit_edge=Direction.S)
        state = self.make_exit_ready_state()

        with patch.object(EvaluationCase, "build_initial_state", return_value=state):
            first = run_policy_evaluation_case(
                case,
                EXIT_GREEDY_POLICY_NAME,
                contracts=self.contracts,
            )
        with patch.object(EvaluationCase, "build_initial_state", return_value=state):
            second = run_policy_evaluation_case(
                case,
                EXIT_GREEDY_POLICY_NAME,
                contracts=self.contracts,
            )

        self.assertEqual(first, second)


class PolicyRunAggregationTests(unittest.TestCase):
    def make_case(
        self,
        *,
        case_id: str,
        sector_type: SectorType,
        cost_mode: CostMode,
    ) -> EvaluationCase:
        blocked_edges = frozenset({Direction.W}) if sector_type is SectorType.RIFT else frozenset()
        return EvaluationCase(
            case_id=case_id,
            sector_id=f"{sector_type.value.lower()}-{case_id}",
            sector_type=sector_type,
            sector_seed=1000,
            entry_edge=Direction.S,
            blocked_edges=blocked_edges,
            selected_exit_edge=Direction.N,
            cost_mode=cost_mode,
            initial_fuel=0,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.NONE,
            revisit_mode=RevisitMode.FIRST_VISIT,
        )

    def make_run_result(
        self,
        *,
        case_id: str,
        policy_name: str,
        sector_type: SectorType,
        cost_mode: CostMode,
        outcome: PolicyRunOutcome,
        srs_turn: int,
        events=(),
        consumed_object_ids=frozenset(),
        activated_object_ids=frozenset(),
        discovered_object_ids=(),
    ) -> PolicyRunResult:
        state = make_state()
        if discovered_object_ids:
            positions = [Position(4 + index, 4) for index, _ in enumerate(discovered_object_ids)]
            for object_id, position in zip(discovered_object_ids, positions, strict=True):
                object_type = {
                    "resource-cache-1": SrsObjectType.RESOURCE_CACHE,
                    "station-1": SrsObjectType.STATION,
                    "salvage-1": SrsObjectType.SALVAGE,
                }[object_id]
                state = place_object(state, position, object_type, object_id)
            state = reveal_positions(state, positions)
        final_state = replace(
            state,
            descriptor=replace(state.descriptor, sector_type=sector_type),
            srs_turn=srs_turn,
            persistent_state=replace(
                state.persistent_state,
                consumed_object_ids=consumed_object_ids,
                activated_object_ids=activated_object_ids,
                sector_type=sector_type,
            ),
        )
        return PolicyRunResult(
            evaluation_case=self.make_case(case_id=case_id, sector_type=sector_type, cost_mode=cost_mode),
            policy_name=policy_name,
            outcome=outcome,
            final_state=final_state,
            command_count=len(events),
            event_log=tuple(events),
            action_sequence=(),
        )

    def test_summarizes_run_counts_rates_and_percentiles(self) -> None:
        runs = [
            self.make_run_result(
                case_id="normal-turn-exit",
                policy_name=EXIT_GREEDY_POLICY_NAME,
                sector_type=SectorType.NORMAL,
                cost_mode=CostMode.TURN_ONLY,
                outcome=PolicyRunOutcome.EXITED,
                srs_turn=4,
                discovered_object_ids=("salvage-1",),
                consumed_object_ids=frozenset({"salvage-1"}),
                events=(
                    make_turn_event(
                        srs_turn=1,
                        event_type=INTERACT_ACCEPTED,
                        payload={"object_id": "salvage-1", "object_type": "SALVAGE", "outcome": "ACCEPTED"},
                    ),
                    make_turn_event(
                        srs_turn=1,
                        event_type=OBJECT_CONSUMED,
                        payload={"object_id": "salvage-1", "object_type": "SALVAGE"},
                    ),
                    make_turn_event(
                        srs_turn=4,
                        event_type=WARP_EXIT_ACCEPTED,
                        payload={"outcome": "ACCEPTED"},
                    ),
                ),
            ),
            self.make_run_result(
                case_id="base-shared-exit",
                policy_name=OBJECT_GREEDY_POLICY_NAME,
                sector_type=SectorType.BASE,
                cost_mode=CostMode.SHARED_FUEL,
                outcome=PolicyRunOutcome.EXITED,
                srs_turn=8,
                discovered_object_ids=("station-1",),
                activated_object_ids=frozenset({"station-1"}),
                events=(
                    make_turn_event(
                        srs_turn=3,
                        event_type=INTERACT_ACCEPTED,
                        payload={"object_id": "station-1", "object_type": "STATION", "outcome": "ACCEPTED"},
                    ),
                    make_turn_event(
                        srs_turn=3,
                        event_type=STATION_ACTIVATED,
                        payload={"object_id": "station-1", "object_type": "STATION"},
                    ),
                    make_turn_event(
                        srs_turn=8,
                        event_type=WARP_EXIT_ACCEPTED,
                        payload={"outcome": "ACCEPTED"},
                    ),
                ),
            ),
            self.make_run_result(
                case_id="resource-turn-limit",
                policy_name=OBJECT_GREEDY_POLICY_NAME,
                sector_type=SectorType.RESOURCE,
                cost_mode=CostMode.TURN_ONLY,
                outcome=PolicyRunOutcome.ABORTED_TURN_LIMIT,
                srs_turn=6,
                discovered_object_ids=("resource-cache-1",),
                consumed_object_ids=frozenset({"resource-cache-1"}),
                events=(
                    make_turn_event(
                        srs_turn=2,
                        event_type=INTERACT_ACCEPTED,
                        payload={"object_id": "resource-cache-1", "object_type": "RESOURCE_CACHE", "outcome": "ACCEPTED"},
                    ),
                    make_turn_event(
                        srs_turn=2,
                        event_type=OBJECT_CONSUMED,
                        payload={"object_id": "resource-cache-1", "object_type": "RESOURCE_CACHE"},
                    ),
                ),
            ),
            self.make_run_result(
                case_id="rift-no-policy",
                policy_name=EXPLORE_THEN_EXIT_POLICY_NAME,
                sector_type=SectorType.RIFT,
                cost_mode=CostMode.SHARED_FUEL,
                outcome=PolicyRunOutcome.ABORTED_NO_POLICY_ACTION,
                srs_turn=10,
                events=(
                    make_turn_event(
                        srs_turn=0,
                        event_type=WARP_EXIT_REJECTED,
                        payload={"outcome": "REJECTED_BLOCKED_EDGE"},
                    ),
                ),
            ),
        ]

        summary = summarize_policy_runs(runs)

        self.assertEqual(summary["run_count"], 4)
        self.assertEqual(summary["run_count_by_policy"][EXIT_GREEDY_POLICY_NAME], 1)
        self.assertEqual(summary["run_count_by_policy"][OBJECT_GREEDY_POLICY_NAME], 2)
        self.assertEqual(summary["run_count_by_cost_mode"], {"TURN_ONLY": 2, "SHARED_FUEL": 2})
        self.assertEqual(
            summary["run_count_by_outcome"],
            {
                "EXITED": 2,
                "ABORTED_TURN_LIMIT": 1,
                "ABORTED_NO_POLICY_ACTION": 1,
                "RESOURCE_DEPLETED": 0,
                "GENERATION_ERROR": 0,
            },
        )
        self.assertEqual(summary["run_count_by_sector_type"]["RIFT"], 1)
        self.assertEqual(summary["exit_rate"], 0.5)
        self.assertEqual(summary["median_srs_turn_count"], 7.0)
        self.assertEqual(summary["p90_srs_turn_count"], 10)
        self.assertEqual(summary["object_discovery_rate"], 0.75)
        self.assertEqual(summary["object_acquisition_rate"], 0.75)
        self.assertEqual(summary["station_use_rate"], 0.25)
        self.assertEqual(summary["resource_use_rate"], 0.25)
        self.assertEqual(summary["salvage_acquisition_rate"], 0.25)
        self.assertEqual(summary["blocked_edge_attempt_rate"], 0.25)
        self.assertEqual(summary["turn_limit_rate"], 0.25)
        self.assertEqual(summary["no_policy_action_rate"], 0.25)
        self.assertEqual(summary["turn_only_exit_rate"], 0.5)
        self.assertEqual(summary["shared_fuel_exit_rate"], 0.5)
        self.assertEqual(summary["turn_only_vs_shared_fuel_failure_delta"], 0.0)
        self.assertEqual(summary["by_policy"][EXIT_GREEDY_POLICY_NAME]["exit_rate"], 1.0)
        self.assertEqual(summary["by_cost_mode"]["TURN_ONLY"]["run_count"], 2)
        self.assertEqual(summary["by_sector_type"]["RESOURCE"]["resource_use_rate"], 1.0)
        self.assertEqual(summary["by_outcome"]["ABORTED_NO_POLICY_ACTION"]["run_count"], 1)

    def test_same_run_list_returns_same_summary_regardless_of_input_order(self) -> None:
        runs = [
            self.make_run_result(
                case_id="case-a",
                policy_name=EXIT_GREEDY_POLICY_NAME,
                sector_type=SectorType.NORMAL,
                cost_mode=CostMode.TURN_ONLY,
                outcome=PolicyRunOutcome.EXITED,
                srs_turn=2,
            ),
            self.make_run_result(
                case_id="case-b",
                policy_name=OBJECT_GREEDY_POLICY_NAME,
                sector_type=SectorType.BASE,
                cost_mode=CostMode.SHARED_FUEL,
                outcome=PolicyRunOutcome.ABORTED_NO_POLICY_ACTION,
                srs_turn=3,
            ),
        ]

        first = summarize_policy_runs(runs)
        second = summarize_policy_runs(list(reversed(runs)))

        self.assertEqual(first, second)
        serialized = json.dumps(first, sort_keys=True)
        self.assertNotIn(str(REPO_ROOT), serialized)
        self.assertNotIn("generated_at", serialized)
        self.assertNotIn("hostname", serialized)


if __name__ == "__main__":
    unittest.main()
