from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path
from typing import Iterable

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import (
    SrsMovementError,
    apply_srs_command,
    fuel_delta_for_movement_raw_cost,
    reveal_full_observation,
    run_srs_commands,
)
from experiments.galactic_exodus.srs.generate import create_sector
from experiments.galactic_exodus.srs.log import (
    MOVE_ACCEPTED,
    MOVE_REJECTED,
    OBSERVATION_UPDATED,
    STOPPED_BEFORE_IMPASSABLE,
)
from experiments.galactic_exodus.srs.model import (
    CostMode,
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsActualMap,
    SrsCell,
    SrsCommand,
    SrsGameState,
    SrsObjectState,
    SrsObjectType,
    SrsTerrainType,
)


REPO_ROOT = Path(__file__).resolve().parents[3]


def make_state(
    *,
    sector_type: SectorType = SectorType.NORMAL,
    sector_seed: int = 1001,
    entry_edge: Direction = Direction.S,
    blocked_edges: frozenset[Direction] = frozenset(),
    fuel: int = 0,
    max_fuel: int = 0,
) -> SrsGameState:
    contracts = load_default_contracts(REPO_ROOT)
    descriptor = SectorDescriptor(
        sector_id=f"{sector_type.value.lower()}-{sector_seed}",
        sector_type=sector_type,
        sector_seed=sector_seed,
        entry_edge=entry_edge,
        blocked_edges=blocked_edges,
    )
    state = create_sector(descriptor, contracts=contracts)
    return replace(
        _clear_objects(state),
        fuel=fuel,
        max_fuel=max_fuel,
    )


def replace_cell_terrain(
    state: SrsGameState,
    position: Position,
    terrain: SrsTerrainType,
) -> SrsGameState:
    rows = [list(row) for row in state.actual_map.cells]
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=terrain,
        object_id=current.object_id,
        actor_id=current.actor_id,
        warp_flags=current.warp_flags,
    )
    return replace(state, actual_map=_build_map(state, rows))


def place_object(
    state: SrsGameState,
    position: Position,
    object_type: SrsObjectType,
    object_id: str,
) -> SrsGameState:
    rows = [list(row) for row in state.actual_map.cells]
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=current.terrain,
        object_id=object_id,
        actor_id=current.actor_id,
        warp_flags=current.warp_flags,
    )
    return replace(
        state,
        actual_map=_build_map(state, rows),
        objects={
            **state.objects,
            object_id: SrsObjectState(
                object_id=object_id,
                object_type=object_type,
                position=position,
            ),
        },
    )


def _clear_objects(state: SrsGameState) -> SrsGameState:
    rows = []
    for row in state.actual_map.cells:
        rows.append(
            [
                SrsCell(
                    terrain=cell.terrain,
                    object_id=None,
                    actor_id=cell.actor_id,
                    warp_flags=cell.warp_flags,
                )
                for cell in row
            ]
        )
    return replace(state, actual_map=_build_map(state, rows), objects={})


def _build_map(state: SrsGameState, rows: list[list[SrsCell]]) -> SrsActualMap:
    return SrsActualMap(
        width=state.actual_map.width,
        height=state.actual_map.height,
        cells=tuple(tuple(row) for row in rows),
    )


def reveal_all_for_move_to(state: SrsGameState) -> SrsGameState:
    return reveal_full_observation(state)


def reveal_positions(
    state: SrsGameState,
    positions: Iterable[Position],
) -> SrsGameState:
    discovered_cells = frozenset(positions)
    known_cells = {
        position: state.actual_map.cell_at(position)
        for position in discovered_cells
    }
    return replace(
        state,
        known_state=replace(
            state.known_state,
            discovered_cells=discovered_cells,
            known_cells=known_cells,
        ),
        persistent_state=replace(
            state.persistent_state,
            discovered_cells=discovered_cells,
        ),
    )


class SrsEngineMovementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_move_route_consumes_one_turn(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 1)

    def test_turn_only_does_not_consume_fuel(self) -> None:
        state = make_state(fuel=7, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 7)
        self.assertEqual(result.events[0].payload["fuel_delta"], 0)
        self.assertEqual(result.events[0].payload["fuel_before"], 7)
        self.assertEqual(result.events[0].payload["fuel_after"], 7)

    def test_fuel_delta_for_turn_only_is_zero(self) -> None:
        self.assertEqual(
            fuel_delta_for_movement_raw_cost(
                40,
                cost_mode=CostMode.TURN_ONLY,
                contracts=self.contracts,
            ),
            0,
        )

    def test_fuel_delta_for_shared_fuel_uses_ceil_raw_cost_div_denominator(self) -> None:
        expectations = {
            0: 0,
            1: -1,
            10: -1,
            11: -2,
            20: -2,
            30: -3,
            40: -4,
        }

        for movement_raw_cost, expected_delta in expectations.items():
            with self.subTest(movement_raw_cost=movement_raw_cost):
                self.assertEqual(
                    fuel_delta_for_movement_raw_cost(
                        movement_raw_cost,
                        cost_mode=CostMode.SHARED_FUEL,
                        contracts=self.contracts,
                    ),
                    expected_delta,
                )

    def test_fuel_delta_rejects_negative_raw_cost(self) -> None:
        with self.assertRaisesRegex(SrsMovementError, "movement_raw_cost must be non-negative"):
            fuel_delta_for_movement_raw_cost(
                -1,
                cost_mode=CostMode.SHARED_FUEL,
                contracts=self.contracts,
            )

    def test_move_route_executes_longest_prefix_within_budget(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="MOVE_ROUTE",
                route=(Direction.N, Direction.N, Direction.N, Direction.N, Direction.N),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_position, Position(4, 4))
        self.assertEqual(result.events[0].event_type, MOVE_ACCEPTED)
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 40)
        self.assertEqual(result.events[0].payload["outcome"], "BUDGET_EXHAUSTED")

    def test_debris_cost_shortens_movement(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 7), SrsTerrainType.DEBRIS)

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="MOVE_ROUTE",
                route=(Direction.N, Direction.N, Direction.N, Direction.N),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_position, Position(4, 5))
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 40)

    def test_asteroid_field_cost_shortens_movement(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 7), SrsTerrainType.ASTEROID_FIELD)

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="MOVE_ROUTE",
                route=(Direction.N, Direction.N, Direction.N),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_position, Position(4, 6))
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 40)

    def test_stop_before_first_blocked_cell(self) -> None:
        state = replace_cell_terrain(
            make_state(
                sector_type=SectorType.RIFT,
                entry_edge=Direction.S,
                blocked_edges=frozenset({Direction.N}),
            ),
            Position(4, 7),
            SrsTerrainType.RIFT_BARRIER,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_position, state.player_position)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertEqual(result.events[0].event_type, STOPPED_BEFORE_IMPASSABLE)
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 0)
        self.assertEqual(result.events[0].payload["observation_updates"], [])

    def test_turn_only_payload_contains_fuel_before_and_after(self) -> None:
        state = make_state(fuel=2, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N, Direction.N, Direction.N)),
            contracts=self.contracts,
            cost_mode=CostMode.TURN_ONLY,
        )

        self.assertEqual(result.events[0].payload["cost_mode"], "TURN_ONLY")
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 40)
        self.assertEqual(result.events[0].payload["fuel_delta"], 0)
        self.assertEqual(result.events[0].payload["fuel_before"], 2)
        self.assertEqual(result.events[0].payload["fuel_after"], 2)

    def test_shared_fuel_consumes_ceil_raw_cost_div_10(self) -> None:
        state = make_state(fuel=7, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N, Direction.N, Direction.N)),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 3)
        self.assertEqual(result.events[0].payload["cost_mode"], "SHARED_FUEL")
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 40)
        self.assertEqual(result.events[0].payload["fuel_delta"], -4)
        self.assertEqual(result.events[0].payload["fuel_before"], 7)
        self.assertEqual(result.events[0].payload["fuel_after"], 3)

    def test_shared_fuel_clamps_state_fuel_to_zero(self) -> None:
        state = make_state(fuel=2, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N, Direction.N, Direction.N)),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 0)

    def test_shared_fuel_payload_keeps_rule_delta_even_when_clamped(self) -> None:
        state = make_state(fuel=2, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N, Direction.N, Direction.N)),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.events[0].payload["fuel_before"], 2)
        self.assertEqual(result.events[0].payload["fuel_delta"], -4)
        self.assertEqual(result.events[0].payload["fuel_after"], 0)

    def test_stop_before_after_partial_movement(self) -> None:
        state = place_object(make_state(), Position(4, 5), SrsObjectType.STAR, "star-blocker")

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="MOVE_ROUTE",
                route=(Direction.N, Direction.N, Direction.N, Direction.N),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_position, Position(4, 6))
        self.assertEqual(result.events[0].event_type, STOPPED_BEFORE_IMPASSABLE)
        self.assertEqual(result.events[0].payload["blocked_position"], [4, 5])
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 20)

    def test_shared_fuel_first_blocked_cell_costs_zero_fuel(self) -> None:
        state = replace_cell_terrain(
            make_state(
                sector_type=SectorType.RIFT,
                entry_edge=Direction.S,
                blocked_edges=frozenset({Direction.N}),
                fuel=5,
                max_fuel=9,
            ),
            Position(4, 7),
            SrsTerrainType.RIFT_BARRIER,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 5)
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 0)
        self.assertEqual(result.events[0].payload["fuel_delta"], 0)
        self.assertEqual(result.events[0].payload["fuel_before"], 5)
        self.assertEqual(result.events[0].payload["fuel_after"], 5)

    def test_shared_fuel_partial_blocked_counts_passable_cells_only(self) -> None:
        state = place_object(make_state(fuel=5, max_fuel=9), Position(4, 5), SrsObjectType.STAR, "star-blocker")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N, Direction.N, Direction.N)),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 3)
        self.assertEqual(result.events[0].event_type, STOPPED_BEFORE_IMPASSABLE)
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 20)
        self.assertEqual(result.events[0].payload["fuel_delta"], -2)
        self.assertEqual(result.events[0].payload["fuel_after"], 3)

    def test_budget_stop_is_move_accepted_not_blocked(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="MOVE_ROUTE",
                route=(Direction.N, Direction.N, Direction.N, Direction.N, Direction.W),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_ACCEPTED)
        self.assertIsNone(result.events[0].payload["blocked_position"])
        self.assertEqual(result.events[0].payload["outcome"], "BUDGET_EXHAUSTED")

    def test_shared_fuel_budget_stop_counts_executed_prefix_only(self) -> None:
        state = make_state(fuel=6, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="MOVE_ROUTE",
                route=(Direction.N, Direction.N, Direction.N, Direction.N, Direction.W),
            ),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 2)
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 40)
        self.assertEqual(result.events[0].payload["fuel_delta"], -4)
        self.assertEqual(result.events[0].payload["fuel_after"], 2)

    def test_successful_steps_update_observation(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N)),
            contracts=self.contracts,
        )

        self.assertEqual([event.event_type for event in result.events], [MOVE_ACCEPTED, OBSERVATION_UPDATED, OBSERVATION_UPDATED])
        self.assertEqual(result.events[0].payload["observation_updates"], [[4, 7], [4, 6]])
        self.assertEqual(result.state.known_state.visited_cells, frozenset({Position(4, 7), Position(4, 6)}))

    def test_partial_blocked_updates_observation_for_entered_cells_only(self) -> None:
        state = place_object(make_state(), Position(4, 5), SrsObjectType.STATION, "station-blocker")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N, Direction.N)),
            contracts=self.contracts,
        )

        self.assertEqual([event.event_type for event in result.events], [STOPPED_BEFORE_IMPASSABLE, OBSERVATION_UPDATED, OBSERVATION_UPDATED])
        self.assertEqual(result.events[0].payload["observation_updates"], [[4, 7], [4, 6]])
        self.assertNotIn(Position(4, 5), result.state.known_state.visited_cells)

    def test_rejected_command_no_turn_no_observation(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state, state)
        self.assertEqual(len(result.state.known_state.discovered_cells), 0)
        self.assertEqual([event.event_type for event in result.events], [MOVE_REJECTED])

    def test_shared_fuel_rejected_command_does_not_change_fuel(self) -> None:
        state = make_state(fuel=5, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT"),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 5)
        self.assertEqual(result.events[0].payload["cost_mode"], "SHARED_FUEL")
        self.assertEqual(result.events[0].payload["fuel_delta"], 0)
        self.assertEqual(result.events[0].payload["fuel_before"], 5)
        self.assertEqual(result.events[0].payload["fuel_after"], 5)

    def test_move_to_rejects_unknown_target(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(4, 7)),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_UNKNOWN_TARGET")
        self.assertEqual(result.events[0].payload["target_position"], [4, 7])
        self.assertEqual(result.events[0].payload["resolved_route"], [])
        self.assertEqual(result.state, state)

    def test_move_to_rejects_same_position(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=state.player_position),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_SAME_POSITION")
        self.assertEqual(result.events[0].payload["target_position"], [4, 8])
        self.assertEqual(result.events[0].payload["resolved_route"], [])
        self.assertEqual(result.state, state)

    def test_move_to_rejects_out_of_bounds_target(self) -> None:
        state = make_state()

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(-1, 8)),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_OUT_OF_BOUNDS")
        self.assertEqual(result.events[0].payload["target_position"], [-1, 8])
        self.assertEqual(result.events[0].payload["resolved_route"], [])
        self.assertEqual(result.state, state)

    def test_move_to_rejects_no_path(self) -> None:
        target = Position(4, 6)
        state = reveal_positions(make_state(), [target])

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=target),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_NO_PATH")
        self.assertEqual(result.events[0].payload["target_position"], [4, 6])
        self.assertEqual(result.events[0].payload["resolved_route"], [])
        self.assertEqual(result.state, state)

    def test_move_to_uses_known_cells_bfs_neighbor_order(self) -> None:
        target = Position(5, 7)
        state = reveal_positions(
            make_state(),
            [
                Position(4, 7),
                Position(5, 8),
                target,
            ],
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=target),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_ACCEPTED)
        self.assertEqual(result.events[0].payload["resolved_route"], ["N", "E"])
        self.assertEqual(result.events[0].payload["entered_cells"], [[4, 7], [5, 7]])

    def test_move_to_executes_resolved_route(self) -> None:
        state = reveal_all_for_move_to(make_state())

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(4, 6)),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_position, Position(4, 6))
        self.assertEqual(result.events[0].event_type, MOVE_ACCEPTED)
        self.assertEqual(result.events[0].payload["entered_cells"], [[4, 7], [4, 6]])
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 20)

    def test_shared_fuel_move_to_uses_resolved_route_cost(self) -> None:
        state = reveal_all_for_move_to(make_state(fuel=6, max_fuel=9))

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(4, 6)),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 4)
        self.assertEqual(result.events[0].payload["command_type"], "MOVE_TO")
        self.assertEqual(result.events[0].payload["resolved_route"], ["N", "N"])
        self.assertEqual(result.events[0].payload["movement_raw_cost"], 20)
        self.assertEqual(result.events[0].payload["fuel_delta"], -2)
        self.assertEqual(result.events[0].payload["fuel_after"], 4)

    def test_move_to_observation_updates_entered_cells(self) -> None:
        state = reveal_all_for_move_to(make_state())

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(4, 6)),
            contracts=self.contracts,
        )

        self.assertEqual(
            [event.event_type for event in result.events],
            [MOVE_ACCEPTED, OBSERVATION_UPDATED, OBSERVATION_UPDATED],
        )
        self.assertEqual(result.events[0].payload["observation_updates"], [[4, 7], [4, 6]])
        self.assertEqual(result.state.known_state.visited_cells, frozenset({Position(4, 7), Position(4, 6)}))

    def test_move_to_payload_contains_target_and_resolved_route(self) -> None:
        state = reveal_all_for_move_to(make_state())

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(5, 7)),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["command_type"], "MOVE_TO")
        self.assertEqual(result.events[0].payload["target_position"], [5, 7])
        self.assertEqual(result.events[0].payload["resolved_route"], ["N", "E"])

    def test_move_to_budget_stop_uses_resolved_route_prefix(self) -> None:
        state = reveal_all_for_move_to(make_state())

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(4, 3)),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_ACCEPTED)
        self.assertEqual(result.events[0].payload["outcome"], "BUDGET_EXHAUSTED")
        self.assertEqual(result.events[0].payload["resolved_route"], ["N", "N", "N", "N", "N"])
        self.assertEqual(result.events[0].payload["entered_cells"], [[4, 7], [4, 6], [4, 5], [4, 4]])
        self.assertEqual(result.state.player_position, Position(4, 4))

    def test_move_to_replaces_1107_unimplemented_reject(self) -> None:
        state = reveal_all_for_move_to(make_state())

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_TO", target=Position(4, 7)),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, MOVE_ACCEPTED)
        self.assertNotEqual(result.events[0].payload["outcome"], "REJECTED_MOVE_TO_UNIMPLEMENTED")

    def test_impassable_star_blocks_movement(self) -> None:
        state = place_object(make_state(), Position(4, 7), SrsObjectType.STAR, "star-blocker")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, STOPPED_BEFORE_IMPASSABLE)
        self.assertEqual(result.events[0].payload["blocked_position"], [4, 7])

    def test_run_srs_commands_accumulates_events(self) -> None:
        state = make_state()

        result = run_srs_commands(
            state,
            (
                SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
                SrsCommand(command_type="INTERACT"),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 1)
        self.assertEqual(
            [event.event_type for event in result.events],
            [MOVE_ACCEPTED, OBSERVATION_UPDATED, MOVE_REJECTED],
        )

    def test_run_srs_commands_passes_shared_fuel_to_each_command(self) -> None:
        state = make_state(fuel=8, max_fuel=9)

        result = run_srs_commands(
            state,
            (
                SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N, Direction.N)),
                SrsCommand(command_type="INTERACT"),
            ),
            contracts=self.contracts,
            cost_mode=CostMode.SHARED_FUEL,
        )

        self.assertEqual(result.state.fuel, 6)
        self.assertEqual(result.events[0].payload["cost_mode"], "SHARED_FUEL")
        self.assertEqual(result.events[0].payload["fuel_delta"], -2)
        self.assertEqual(result.events[-1].payload["cost_mode"], "SHARED_FUEL")
        self.assertEqual(result.events[-1].payload["fuel_before"], 6)
        self.assertEqual(result.events[-1].payload["fuel_after"], 6)


if __name__ == "__main__":
    unittest.main()
