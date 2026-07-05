from __future__ import annotations

import csv
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from dataclasses import replace
from io import StringIO
from pathlib import Path
from unittest.mock import patch

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs import evaluate_policies as evaluator
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
    build_policy_summary_document,
    build_default_evaluation_cases,
    build_object_greedy_candidates,
    choose_explore_then_exit_command,
    choose_exit_greedy_command,
    choose_known_target_step,
    choose_object_greedy_command,
    evaluate_default_policies,
    first_known_route_step,
    is_known_passable_cell,
    iter_known_cardinal_neighbors,
    parse_args,
    route_on_known_cells,
    run_policy_evaluation_case,
    summarize_policy_runs,
    write_policy_runs_csv,
    write_policy_summary_json,
)
from experiments.galactic_exodus.srs.log import (
    INTERACT_ACCEPTED,
    INTERACT_REJECTED,
    MOVE_REJECTED,
    OBSERVATION_UPDATED,
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
    row_idx, col_idx = state.actual_map.indices_for(position)
    rows[row_idx][col_idx] = SrsCell(
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
                    rows[state.actual_map.indices_for(known_position)[0]][state.actual_map.indices_for(known_position)[1]]
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
        state = reveal_positions(make_state(), [Position(5, 9), Position(5, 8)])

        self.assertTrue(
            is_known_passable_cell(
                Position(5, 8),
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

    def test_known_passable_cell_rejects_impassable_terrain_and_celestial_objects(self) -> None:
        state = replace_cell_terrain(make_state(), Position(5, 8), SrsTerrainType.ASTEROID)
        state = reveal_positions(state, [Position(5, 8), Position(5, 7), Position(5, 6), Position(6, 8), Position(6, 7)])
        state = place_object(state, Position(5, 7), SrsObjectType.STAR, "star-a")
        state = place_object(state, Position(5, 6), SrsObjectType.PLANET, "planet-a")
        state = place_object(state, Position(6, 8), SrsObjectType.STATION, "station-a")
        state = place_object(state, Position(6, 7), SrsObjectType.RESOURCE_CACHE, "resource-a")
        state = reveal_positions(state, [Position(5, 8), Position(5, 7), Position(5, 6), Position(6, 8), Position(6, 7)])

        self.assertFalse(
            is_known_passable_cell(
                Position(5, 8),
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
        self.assertFalse(
            is_known_passable_cell(
                Position(5, 6),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertFalse(
            is_known_passable_cell(
                Position(6, 8),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )
        self.assertTrue(
            is_known_passable_cell(
                Position(6, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_salvage_is_treated_as_known_passable(self) -> None:
        state = place_object(make_state(), Position(5, 8), SrsObjectType.SALVAGE, "salvage-a")
        state = reveal_positions(state, [Position(5, 9), Position(5, 8)])

        self.assertTrue(
            is_known_passable_cell(
                Position(5, 8),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_route_on_known_cells_does_not_cross_undiscovered_cells(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(6, 9), Position(6, 8), Position(6, 7)],
        )

        self.assertEqual(
            route_on_known_cells(
                Position(5, 9),
                Position(6, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            ),
            (Direction.E, Direction.N, Direction.N),
        )
        self.assertIsNone(
            route_on_known_cells(
                Position(5, 9),
                Position(5, 7),
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
        )

    def test_first_known_route_step_uses_known_state_without_actual_map(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(5, 8), Position(5, 7)],
        )

        with patch(
            "experiments.galactic_exodus.srs.engine.route_to_known_target",
            side_effect=AssertionError("engine.route_to_known_target must not be used"),
        ):
            self.assertEqual(
                first_known_route_step(
                    Position(5, 9),
                    Position(5, 7),
                    known_cells=state.known_state.known_cells,
                    objects=state.objects,
                ),
                Direction.N,
            )

    def test_choose_known_target_step_applies_deterministic_tie_breaking(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(5, 8), Position(6, 9), Position(4, 9)],
        )

        self.assertEqual(
            choose_known_target_step(
                Position(5, 9),
                [Position(5, 8), Position(6, 9), Position(4, 9)],
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            ),
            (Position(5, 8), Direction.N),
        )
        self.assertEqual(
            choose_known_target_step(
                Position(5, 9),
                [Position(6, 9), Position(4, 9)],
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            ),
            (Position(4, 9), Direction.W),
        )

    def test_iter_known_cardinal_neighbors_is_stable(self) -> None:
        self.assertEqual(
            iter_known_cardinal_neighbors(Position(5, 9)),
            (
                (Direction.N, Position(5, 8)),
                (Direction.E, Position(6, 9)),
                (Direction.S, Position(5, 10)),
                (Direction.W, Position(4, 9)),
            ),
        )

    def test_route_on_known_cells_is_deterministic_for_same_input(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(5, 8), Position(6, 9), Position(6, 8)],
        )

        first = route_on_known_cells(
            Position(5, 9),
            Position(6, 8),
            known_cells=state.known_state.known_cells,
            objects=state.objects,
        )
        second = route_on_known_cells(
            Position(5, 9),
            Position(6, 8),
            known_cells=state.known_state.known_cells,
            objects=state.objects,
        )

        self.assertEqual(first, (Direction.N, Direction.E))
        self.assertEqual(first, second)


class ExitGreedyPolicyTests(unittest.TestCase):
    def test_policy_name_is_stable(self) -> None:
        self.assertEqual(EXIT_GREEDY_POLICY_NAME, "EXIT_GREEDY")

    def test_returns_warp_exit_when_current_cell_has_selected_exit(self) -> None:
        state = reveal_positions(make_state(entry_edge=Direction.S), [Position(5, 9)])

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.S)

        self.assertEqual(
            command,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
        )

    def test_returns_single_step_move_route_toward_nearest_known_warp_cell(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(6, 9), Position(7, 9)],
        )
        state = replace_cell_warp_flags(state, Position(7, 9), frozenset({Direction.N}))

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )
        self.assertEqual(command.route, (Direction.E,))

    def test_returns_no_action_when_selected_exit_warp_cell_is_undiscovered(self) -> None:
        state = reveal_positions(make_state(), [Position(5, 9), Position(5, 8)])

        self.assertIsNone(
            choose_exit_greedy_command(state, selected_exit_edge=Direction.N)
        )

    def test_returns_no_action_when_known_warp_cell_is_unreachable(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(6, 9), Position(7, 9)],
        )
        state = replace_cell_terrain(state, Position(6, 9), SrsTerrainType.ASTEROID)
        state = reveal_positions(state, [Position(5, 9), Position(6, 9), Position(7, 9)])
        state = replace_cell_warp_flags(state, Position(7, 9), frozenset({Direction.N}))

        self.assertIsNone(
            choose_exit_greedy_command(state, selected_exit_edge=Direction.N)
        )

    def test_does_not_detour_to_objects_before_exit(self) -> None:
        state = reveal_positions(
            place_object(make_state(), Position(5, 8), SrsObjectType.SALVAGE, "salvage-a"),
            [Position(5, 9), Position(5, 8), Position(6, 9), Position(7, 9)],
        )
        state = replace_cell_warp_flags(state, Position(7, 9), frozenset({Direction.N}))

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )

    def test_never_returns_move_to(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(6, 9), Position(7, 9)],
        )
        state = replace_cell_warp_flags(state, Position(7, 9), frozenset({Direction.N}))

        command = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertIsNotNone(command)
        self.assertNotEqual(command.command_type, "MOVE_TO")

    def test_is_deterministic_for_same_input(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(5, 8), Position(6, 9), Position(6, 8)],
        )
        state = replace_cell_warp_flags(state, Position(5, 8), frozenset({Direction.N}))
        state = replace_cell_warp_flags(state, Position(6, 9), frozenset({Direction.N}))

        first = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)
        second = choose_exit_greedy_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(first, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))
        self.assertEqual(first, second)

    def test_uses_known_state_without_actual_map(self) -> None:
        state = reveal_positions(
            make_state(),
            [Position(5, 9), Position(6, 9), Position(7, 9)],
        )
        state = replace_cell_warp_flags(state, Position(7, 9), frozenset({Direction.N}))

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
            Position(5, 9),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = place_object(state, Position(6, 9), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(5, 9), Position(6, 9)])

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
            Position(5, 9),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(5, 9), Position(5, 8), Position(5, 7)])
        state = replace_cell_warp_flags(state, Position(5, 7), frozenset({Direction.N}))

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
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
        state = reveal_positions(state, [Position(5, 9), Position(5, 8)])

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
        state = place_object(make_state(fuel=9, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
        state = reveal_positions(state, [Position(5, 9), Position(5, 8), Position(6, 9)])
        state = replace_cell_warp_flags(state, Position(6, 9), frozenset({Direction.N}))

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(
            command,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)),
        )

    def test_station_is_candidate_for_upgrade_even_when_fuel_is_full(self) -> None:
        state = place_object(make_state(fuel=9, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
        state = replace(state, player_state=replace(state.player_state, salvage=4))
        state = reveal_positions(state, [Position(5, 9), Position(5, 8)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertEqual(command, SrsCommand(command_type="INTERACT", target_object_id="station-1"))

    def test_known_unconsumed_salvage_is_a_candidate(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(6, 9), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(5, 9), Position(6, 9)])

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
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
        state = reveal_positions(state, [Position(5, 9), Position(5, 8), Position(6, 9)])

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
            Position(5, 7),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(5, 9), Position(5, 8), Position(5, 7)])

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
        state = reveal_positions(make_state(), [Position(5, 9), Position(5, 8), Position(5, 7)])
        state = replace_cell_warp_flags(state, Position(5, 7), frozenset({Direction.N}))

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
            Position(5, 9),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = place_object(state, Position(6, 9), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(5, 9), Position(6, 9)])

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
            Position(5, 7),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(5, 9), Position(5, 8)])

        candidates = build_object_greedy_candidates(
            state,
            contracts=self.contracts,
        )

        self.assertEqual(candidates, ())

    def test_never_returns_move_to(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(6, 9), SrsObjectType.SALVAGE, "salvage-1")
        state = reveal_positions(state, [Position(5, 9), Position(6, 9)])

        command = choose_object_greedy_command(
            state,
            contracts=self.contracts,
            selected_exit_edge=Direction.N,
        )

        self.assertIsNotNone(command)
        self.assertNotEqual(command.command_type, "MOVE_TO")

    def test_is_deterministic_for_same_input(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 9), SrsObjectType.SALVAGE, "salvage-a")
        state = place_object(state, Position(6, 9), SrsObjectType.SALVAGE, "salvage-b")
        state = reveal_positions(state, [Position(5, 9), Position(4, 9), Position(6, 9)])

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
            Position(5, 7),
            SrsObjectType.RESOURCE_CACHE,
            "resource-cache-1",
            fuel_restore=5,
        )
        state = reveal_positions(state, [Position(5, 9), Position(5, 8), Position(5, 7)])

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
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 4),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
                Position(5, 3),
                Position(4, 4),
                Position(6, 4),
            ],
        )
        state = replace_cell_terrain(state, Position(4, 5), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(6, 5), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(5, 6), SrsTerrainType.ASTEROID)
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 4),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
                Position(5, 3),
                Position(4, 4),
                Position(6, 4),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))

    def test_steps_into_unknown_when_current_position_is_frontier(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))

    def test_selected_exit_edge_breaks_unknown_direction_ties(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 6),
                Position(4, 5),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.E)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)))

    def test_falls_back_to_exit_greedy_after_max_explore_steps(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5), srs_turn=12)
        state = reveal_positions(
            state,
            [Position(x, y) for y in range(1, 10) for x in range(1, 10)],
        )
        state = replace_cell_warp_flags(state, Position(7, 5), frozenset({Direction.N}))

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.E,)))

    def test_falls_back_to_exit_greedy_when_no_frontier_exists(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [Position(x, y) for y in range(1, 10) for x in range(1, 10)],
        )
        state = replace_cell_warp_flags(state, Position(7, 5), frozenset({Direction.N}))

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertEqual(command, SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)))

    def test_returns_no_action_when_frontier_and_exit_are_unreachable(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 4),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
                Position(7, 5),
            ],
        )
        state = replace_cell_terrain(state, Position(5, 4), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(5, 6), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(4, 5), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(6, 5), SrsTerrainType.ASTEROID)
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 4),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
                Position(7, 5),
            ],
        )

        self.assertIsNone(
            choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)
        )

    def test_never_returns_interact_or_move_to(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
            ],
        )

        command = choose_explore_then_exit_command(state, selected_exit_edge=Direction.N)

        self.assertIsNotNone(command)
        self.assertNotIn(command.command_type, {"INTERACT", "MOVE_TO"})

    def test_uses_known_state_without_reading_unknown_cell_contents(self) -> None:
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
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
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(
            state,
            [
                Position(5, 5),
                Position(5, 6),
                Position(4, 5),
                Position(6, 5),
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
        return reveal_positions(make_state(entry_edge=Direction.S), [Position(5, 9)])

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
        state = replace(make_state(), player_position=Position(5, 5))
        state = reveal_positions(state, [Position(5, 5), Position(5, 4)])

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
                    SrsCommand(command_type="MOVE_TO", target=Position(1, 1)),
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
        repeated_command = SrsCommand(command_type="MOVE_TO", target=Position(1, 1))

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
        first = SrsCommand(command_type="MOVE_TO", target=Position(1, 1))
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
        if sector_type is SectorType.NEBULA:
            nebula_positions = [state.player_position]
            for event in events:
                center = event.payload.get("center")
                if isinstance(center, list) and len(center) == 2:
                    nebula_positions.append(Position(center[0], center[1]))
            for position in nebula_positions:
                state = replace_cell_terrain(state, position, SrsTerrainType.NEBULA)
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


class PolicyRunWriterTests(PolicyRunAggregationTests):
    def test_csv_writer_preserves_column_order_and_stable_sort(self) -> None:
        runs = [
            self.make_run_result(
                case_id="case-b",
                policy_name=OBJECT_GREEDY_POLICY_NAME,
                sector_type=SectorType.NEBULA,
                cost_mode=CostMode.SHARED_FUEL,
                outcome=PolicyRunOutcome.ABORTED_NO_POLICY_ACTION,
                srs_turn=7,
                discovered_object_ids=("salvage-1",),
                consumed_object_ids=frozenset({"salvage-1"}),
                events=(
                    make_turn_event(
                        srs_turn=1,
                        event_type=OBSERVATION_UPDATED,
                        payload={"center": [4, 4], "newly_discovered_count": 3, "total_discovered_count": 9},
                    ),
                    make_turn_event(
                        srs_turn=2,
                        event_type=WARP_EXIT_REJECTED,
                        payload={"outcome": "REJECTED_BLOCKED_EDGE"},
                    ),
                ),
            ),
            self.make_run_result(
                case_id="case-a",
                policy_name=EXIT_GREEDY_POLICY_NAME,
                sector_type=SectorType.NORMAL,
                cost_mode=CostMode.TURN_ONLY,
                outcome=PolicyRunOutcome.EXITED,
                srs_turn=5,
                discovered_object_ids=("resource-cache-1", "station-1"),
                consumed_object_ids=frozenset({"resource-cache-1"}),
                activated_object_ids=frozenset({"station-1"}),
                events=(
                    make_turn_event(
                        srs_turn=1,
                        event_type=OBSERVATION_UPDATED,
                        payload={"center": [4, 4], "newly_discovered_count": 5, "total_discovered_count": 25},
                    ),
                    make_turn_event(
                        srs_turn=2,
                        event_type=OBSERVATION_UPDATED,
                        payload={"center": [5, 4], "newly_discovered_count": 4, "total_discovered_count": 29},
                    ),
                ),
            ),
        ]

        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            output_path = Path(tmp_dir) / "nested" / "policy_runs.csv"
            write_policy_runs_csv(output_path, runs)
            with output_path.open(encoding="utf-8", newline="") as file:
                rows = list(csv.DictReader(file))
                fieldnames = list(rows[0].keys())

        self.assertEqual(
            fieldnames,
            [
                "case_id",
                "policy",
                "sector_type",
                "sector_seed",
                "entry_edge",
                "selected_exit_edge",
                "cost_mode",
                "outcome",
                "srs_turn_count",
                "command_count",
                "final_fuel",
                "max_fuel",
                "objects_discovered",
                "objects_acquired",
                "station_used",
                "resource_used",
                "salvage_acquired",
                "blocked_edge_attempt_count",
                "observation_5x5_count",
                "observation_3x3_count",
            ],
        )
        self.assertEqual([row["case_id"] for row in rows], ["case-a", "case-b"])
        self.assertEqual(rows[0]["station_used"], "1")
        self.assertEqual(rows[0]["resource_used"], "1")
        self.assertEqual(rows[0]["objects_discovered"], "2")
        self.assertEqual(rows[0]["objects_acquired"], "2")
        self.assertEqual(rows[0]["observation_5x5_count"], "2")
        self.assertEqual(rows[0]["observation_3x3_count"], "0")
        self.assertEqual(rows[1]["salvage_acquired"], "1")
        self.assertEqual(rows[1]["blocked_edge_attempt_count"], "1")
        self.assertEqual(rows[1]["observation_3x3_count"], "1")

    def test_json_summary_writer_uses_fixed_top_level_keys_and_indent(self) -> None:
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

        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            summary_path = Path(tmp_dir) / "nested" / "policy_summary.json"
            write_policy_summary_json(summary_path, runs)
            text = summary_path.read_text(encoding="utf-8")
            payload = json.loads(text)

        self.assertEqual(list(payload.keys()), ["run_count", "policies", "conditions", "metrics"])
        self.assertTrue(text.startswith('{\n  "run_count": 2,\n  "policies": {\n'))
        self.assertEqual(list(payload["conditions"].keys()), ["cost_modes", "sector_types", "outcomes"])
        self.assertNotIn("generated_at", text)
        self.assertNotIn(str(REPO_ROOT), text)
        self.assertNotIn("PosixPath", text)

    def test_same_input_produces_same_csv_and_json(self) -> None:
        runs = [
            self.make_run_result(
                case_id="case-b",
                policy_name=OBJECT_GREEDY_POLICY_NAME,
                sector_type=SectorType.BASE,
                cost_mode=CostMode.SHARED_FUEL,
                outcome=PolicyRunOutcome.EXITED,
                srs_turn=4,
            ),
            self.make_run_result(
                case_id="case-a",
                policy_name=EXIT_GREEDY_POLICY_NAME,
                sector_type=SectorType.NORMAL,
                cost_mode=CostMode.TURN_ONLY,
                outcome=PolicyRunOutcome.ABORTED_TURN_LIMIT,
                srs_turn=6,
            ),
        ]

        with tempfile.TemporaryDirectory(dir=".tmp") as first_tmp_dir:
            first_csv_path = Path(first_tmp_dir) / "policy_runs.csv"
            first_json_path = Path(first_tmp_dir) / "policy_summary.json"
            write_policy_runs_csv(first_csv_path, runs)
            write_policy_summary_json(first_json_path, runs)
            first_csv = first_csv_path.read_text(encoding="utf-8")
            first_json = first_json_path.read_text(encoding="utf-8")

        with tempfile.TemporaryDirectory(dir=".tmp") as second_tmp_dir:
            second_csv_path = Path(second_tmp_dir) / "policy_runs.csv"
            second_json_path = Path(second_tmp_dir) / "policy_summary.json"
            write_policy_runs_csv(second_csv_path, list(reversed(runs)))
            write_policy_summary_json(second_json_path, list(reversed(runs)))
            second_csv = second_csv_path.read_text(encoding="utf-8")
            second_json = second_json_path.read_text(encoding="utf-8")

        self.assertEqual(first_csv, second_csv)
        self.assertEqual(first_json, second_json)
        self.assertEqual(build_policy_summary_document(runs), build_policy_summary_document(list(reversed(runs))))


class EvaluatePoliciesCliTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(".tmp/evaluate_policies_cli_tests") / self._testMethodName
        self.root.mkdir(parents=True, exist_ok=True)

    def tearDown(self) -> None:
        shutil.rmtree(self.root, ignore_errors=True)

    def run_main(self, *args: str) -> tuple[int, str, str]:
        stdout = StringIO()
        stderr = StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = evaluator.main(list(args))
        return exit_code, stdout.getvalue(), stderr.getvalue()

    def test_parse_args_accepts_required_output_paths(self) -> None:
        args = parse_args(
            [
                "--output-runs",
                str(self.root / "policy_runs.csv"),
                "--output-summary",
                str(self.root / "policy_summary.json"),
            ]
        )

        self.assertEqual(args.output_runs, self.root / "policy_runs.csv")
        self.assertEqual(args.output_summary, self.root / "policy_summary.json")

    def test_main_writes_csv_and_json_to_requested_paths(self) -> None:
        output_runs = self.root / "results" / "policy_runs.csv"
        output_summary = self.root / "results" / "policy_summary.json"

        exit_code, stdout, stderr = self.run_main(
            "--output-runs",
            str(output_runs),
            "--output-summary",
            str(output_summary),
        )

        self.assertEqual(exit_code, 0)
        self.assertEqual(stdout, "")
        self.assertEqual(stderr, "")
        self.assertTrue(output_runs.is_file())
        self.assertTrue(output_summary.is_file())
        with output_runs.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        summary = json.loads(output_summary.read_text(encoding="utf-8"))
        self.assertEqual(len(rows), len(build_default_evaluation_cases()) * 3)
        self.assertEqual(summary["run_count"], len(rows))

    def test_main_creates_missing_parent_directories(self) -> None:
        output_runs = self.root / "deep" / "nested" / "policy_runs.csv"
        output_summary = self.root / "deep" / "nested" / "policy_summary.json"

        exit_code, _, stderr = self.run_main(
            "--output-runs",
            str(output_runs),
            "--output-summary",
            str(output_summary),
        )

        self.assertEqual(exit_code, 0)
        self.assertEqual(stderr, "")
        self.assertTrue(output_runs.exists())
        self.assertTrue(output_summary.exists())

    def test_same_cli_equivalent_processing_is_fully_reproducible(self) -> None:
        first_runs = self.root / "first" / "policy_runs.csv"
        first_summary = self.root / "first" / "policy_summary.json"
        second_runs = self.root / "second" / "policy_runs.csv"
        second_summary = self.root / "second" / "policy_summary.json"

        first_exit_code, _, first_stderr = self.run_main(
            "--output-runs",
            str(first_runs),
            "--output-summary",
            str(first_summary),
        )
        second_exit_code, _, second_stderr = self.run_main(
            "--output-runs",
            str(second_runs),
            "--output-summary",
            str(second_summary),
        )

        self.assertEqual(first_exit_code, 0)
        self.assertEqual(second_exit_code, 0)
        self.assertEqual(first_stderr, "")
        self.assertEqual(second_stderr, "")
        self.assertEqual(first_runs.read_text(encoding="utf-8"), second_runs.read_text(encoding="utf-8"))
        self.assertEqual(first_summary.read_text(encoding="utf-8"), second_summary.read_text(encoding="utf-8"))

    def test_main_returns_non_zero_on_write_failure(self) -> None:
        blocked_parent = self.root / "blocked"
        blocked_parent.write_text("not a directory", encoding="utf-8")

        exit_code, stdout, stderr = self.run_main(
            "--output-runs",
            str(blocked_parent / "policy_runs.csv"),
            "--output-summary",
            str(self.root / "policy_summary.json"),
        )

        self.assertEqual(exit_code, 1)
        self.assertEqual(stdout, "")
        self.assertIn("error:", stderr)

    def test_missing_required_arguments_exit_non_zero(self) -> None:
        with redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit) as cm:
                evaluator.main(
                    [
                        "--output-runs",
                        str(self.root / "policy_runs.csv"),
                    ]
                )
        self.assertNotEqual(cm.exception.code, 0)

    def test_evaluate_default_policies_runs_all_default_cases_and_policies(self) -> None:
        runs = evaluate_default_policies(contracts=load_default_contracts(REPO_ROOT))

        self.assertEqual(len(runs), len(build_default_evaluation_cases()) * 3)
        self.assertEqual(
            {run.policy_name for run in runs},
            {
                EXIT_GREEDY_POLICY_NAME,
                EXPLORE_THEN_EXIT_POLICY_NAME,
                OBJECT_GREEDY_POLICY_NAME,
            },
        )


if __name__ == "__main__":
    unittest.main()
