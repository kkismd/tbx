from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import apply_srs_command
from experiments.galactic_exodus.srs.generate import EDGE_POSITIONS, create_sector
from experiments.galactic_exodus.srs.log import WARP_EXIT_ACCEPTED, WARP_EXIT_REJECTED
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsActualMap,
    SrsCell,
    SrsCommand,
    SrsGameState,
    SrsTerrainType,
    derive_lrs_blocked_routes,
)


REPO_ROOT = Path(__file__).resolve().parents[3]


def make_state(
    *,
    sector_type: SectorType = SectorType.NORMAL,
    sector_seed: int = 1001,
    entry_edge: Direction = Direction.S,
    blocked_edges: frozenset[Direction] = frozenset(),
    player_position: Position | None = None,
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
    if player_position is None:
        return state
    return replace(state, player_position=player_position)


def replace_cell(
    state: SrsGameState,
    position: Position,
    *,
    terrain: SrsTerrainType | None = None,
    warp_flags: frozenset[Direction] | None = None,
) -> SrsGameState:
    rows = [list(row) for row in state.actual_map.cells]
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=current.terrain if terrain is None else terrain,
        object_id=current.object_id,
        actor_id=current.actor_id,
        warp_flags=current.warp_flags if warp_flags is None else warp_flags,
    )
    actual_map = SrsActualMap(
        width=state.actual_map.width,
        height=state.actual_map.height,
        cells=tuple(tuple(row) for row in rows),
    )
    return replace(state, actual_map=actual_map)


class SrsEngineWarpTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_warp_exit_requires_exit_direction(self) -> None:
        with self.assertRaisesRegex(ValueError, "WARP_EXIT requires an exit_direction"):
            SrsCommand(command_type="WARP_EXIT")

    def test_warp_exit_requires_current_cell_direction_flag(self) -> None:
        state = make_state(player_position=Position(4, 4))

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.N),
            contracts=self.contracts,
        )

        self.assertEqual(result.state, state)
        self.assertEqual(result.events[0].event_type, WARP_EXIT_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_NO_WARP_FLAG")

    def test_warp_exit_accepts_from_flagged_edge_center(self) -> None:
        state = make_state(entry_edge=Direction.S)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 1)
        self.assertEqual(result.state.player_position, state.player_position)
        self.assertEqual(result.events[0].event_type, WARP_EXIT_ACCEPTED)
        self.assertEqual(result.events[0].payload["outcome"], "ACCEPTED")

    def test_warp_exit_rejects_blocked_edge(self) -> None:
        state = make_state(
            sector_type=SectorType.RIFT,
            sector_seed=4001,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N}),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.N),
            contracts=self.contracts,
        )

        self.assertEqual(result.state, state)
        self.assertEqual(result.events[0].event_type, WARP_EXIT_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_BLOCKED_EDGE")

    def test_warp_exit_rejects_wrong_direction_on_other_edge(self) -> None:
        state = make_state(entry_edge=Direction.S)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.N),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_NO_WARP_FLAG")

    def test_warp_exit_consumes_turn_on_success(self) -> None:
        state = make_state(entry_edge=Direction.E)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.E),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, state.srs_turn + 1)

    def test_warp_exit_rejected_does_not_consume_turn(self) -> None:
        state = make_state(player_position=Position(4, 4))

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.E),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, state.srs_turn)

    def test_warp_exit_does_not_change_fuel_or_position(self) -> None:
        state = replace(make_state(entry_edge=Direction.W), fuel=5, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.W),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 5)
        self.assertEqual(result.state.max_fuel, 9)
        self.assertEqual(result.state.player_position, state.player_position)

    def test_warp_exit_event_payload_fields(self) -> None:
        state = make_state(entry_edge=Direction.N)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.N),
            contracts=self.contracts,
        )

        self.assertEqual(
            result.events[0].payload,
            {
                "command_type": "WARP_EXIT",
                "exit_direction": "N",
                "start_position": [4, 8],
                "warp_position": [4, 8],
                "sector_id": state.descriptor.sector_id,
                "generated_map_id": state.persistent_state.generated_map_id,
                "outcome": "ACCEPTED",
            },
        )

    def test_warp_exit_rejects_out_of_bounds_player_position_first(self) -> None:
        state = make_state(entry_edge=Direction.S)
        state = replace(state, player_position=Position(-1, 8))

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_OUT_OF_BOUNDS")

    def test_warp_exit_prefers_blocked_edge_before_warp_flag_check(self) -> None:
        state = make_state(
            sector_type=SectorType.RIFT,
            sector_seed=4002,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N}),
        )
        blocked_center = EDGE_POSITIONS[Direction.N]
        state = replace(state, player_position=blocked_center)
        state = replace_cell(
            state,
            blocked_center,
            terrain=SrsTerrainType.FLOOR,
            warp_flags=frozenset({Direction.N}),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.N),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_BLOCKED_EDGE")

    def test_rift_blocked_edge_derived_to_lrs(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="rift-1",
            sector_type=SectorType.RIFT,
            sector_seed=9001,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N, Direction.E}),
        )

        self.assertEqual(
            derive_lrs_blocked_routes(descriptor),
            frozenset(
                {
                    ("rift-1", Direction.N),
                    ("rift-1", Direction.E),
                }
            ),
        )

    def test_non_rift_has_no_lrs_blocked_routes(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="normal-1",
            sector_type=SectorType.NORMAL,
            sector_seed=9002,
            entry_edge=Direction.S,
        )

        self.assertEqual(derive_lrs_blocked_routes(descriptor), frozenset())


if __name__ == "__main__":
    unittest.main()
