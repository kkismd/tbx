from __future__ import annotations

import unittest
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import (
    known_cell_at,
    observation_area,
    observation_size_for_terrain,
    restore_srs_state,
    reveal_full_observation,
    reveal_observation,
    snapshot_srs_state,
)
from experiments.galactic_exodus.srs.generate import create_sector
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsActualMap,
    SrsCell,
    SrsGameState,
    SrsTerrainType,
)


REPO_ROOT = Path(__file__).resolve().parents[3]


def make_state(
    *,
    sector_type: SectorType = SectorType.NORMAL,
    sector_seed: int = 1001,
    entry_edge: Direction = Direction.S,
    blocked_edges: frozenset[Direction] = frozenset(),
) -> SrsGameState:
    contracts = load_default_contracts(REPO_ROOT)
    descriptor = SectorDescriptor(
        sector_id=f"{sector_type.value.lower()}-{sector_seed}",
        sector_type=sector_type,
        sector_seed=sector_seed,
        entry_edge=entry_edge,
        blocked_edges=blocked_edges,
    )
    return create_sector(descriptor, contracts=contracts)


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
    actual_map = SrsActualMap(
        width=state.actual_map.width,
        height=state.actual_map.height,
        cells=tuple(tuple(row) for row in rows),
    )
    return SrsGameState(
        descriptor=state.descriptor,
        actual_map=actual_map,
        known_state=state.known_state,
        persistent_state=state.persistent_state,
        player_position=state.player_position,
        objects=state.objects,
        srs_turn=state.srs_turn,
        fuel=state.fuel,
        max_fuel=state.max_fuel,
    )


class SrsObservationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_observation_size_for_floor_is_5(self) -> None:
        self.assertEqual(
            observation_size_for_terrain(SrsTerrainType.FLOOR, self.contracts),
            5,
        )

    def test_observation_size_for_nebula_is_3(self) -> None:
        self.assertEqual(
            observation_size_for_terrain(SrsTerrainType.NEBULA, self.contracts),
            3,
        )

    def test_observation_area_clips_at_map_edge(self) -> None:
        state = make_state()

        area = observation_area(state.actual_map, center=Position(0, 0), size=5)

        self.assertEqual(len(area), 9)
        self.assertIn(Position(0, 0), area)
        self.assertIn(Position(2, 2), area)
        self.assertNotIn(Position(3, 3), area)

    def test_observation_area_rejects_out_of_bounds_center(self) -> None:
        state = make_state()

        with self.assertRaisesRegex(ValueError, "out of bounds"):
            observation_area(state.actual_map, center=Position(-1, 0), size=5)

    def test_full_observation_reveals_all_cells(self) -> None:
        state = make_state()

        revealed = reveal_full_observation(state)

        self.assertEqual(len(revealed.known_state.discovered_cells), 81)
        self.assertEqual(len(revealed.known_state.known_cells), 81)
        self.assertEqual(revealed.persistent_state.discovered_cells, revealed.known_state.discovered_cells)
        self.assertEqual(revealed.known_state.visited_cells, frozenset())

    def test_local_movement_floor_reveals_5x5(self) -> None:
        state = make_state()

        revealed = reveal_observation(
            state,
            center=Position(4, 4),
            contracts=self.contracts,
        )

        self.assertEqual(len(revealed.known_state.discovered_cells), 25)
        self.assertEqual(len(revealed.known_state.known_cells), 25)
        self.assertEqual(revealed.persistent_state.discovered_cells, revealed.known_state.discovered_cells)

    def test_local_movement_nebula_reveals_3x3(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 4), SrsTerrainType.NEBULA)

        revealed = reveal_observation(
            state,
            center=Position(4, 4),
            contracts=self.contracts,
        )

        self.assertEqual(len(revealed.known_state.discovered_cells), 9)
        self.assertEqual(len(revealed.known_state.known_cells), 9)

    def test_known_map_is_cumulative(self) -> None:
        state = make_state()
        first = reveal_observation(
            state,
            center=Position(4, 4),
            contracts=self.contracts,
        )

        second = reveal_observation(
            first,
            center=Position(6, 4),
            contracts=self.contracts,
        )

        self.assertEqual(len(first.known_state.discovered_cells), 25)
        self.assertEqual(len(second.known_state.discovered_cells), 35)
        self.assertTrue(first.known_state.discovered_cells.issubset(second.known_state.discovered_cells))

    def test_reveal_observation_marks_center_visited(self) -> None:
        state = make_state()

        revealed = reveal_observation(
            state,
            center=Position(4, 4),
            contracts=self.contracts,
        )

        self.assertEqual(revealed.known_state.visited_cells, frozenset({Position(4, 4)}))

    def test_rejected_command_does_not_reveal_when_observation_not_called(self) -> None:
        state = make_state()

        self.assertEqual(state.known_state.discovered_cells, frozenset())
        self.assertEqual(state.known_state.known_cells, {})

    def test_first_blocked_cell_collision_does_not_reveal_when_observation_not_called(self) -> None:
        state = make_state(
            sector_type=SectorType.RIFT,
            sector_seed=4001,
            blocked_edges=frozenset({Direction.N}),
        )

        self.assertEqual(state.known_state.discovered_cells, frozenset())
        self.assertEqual(state.known_state.known_cells, {})

    def test_known_cell_at_returns_none_for_unseen_cell(self) -> None:
        state = make_state()

        self.assertIsNone(known_cell_at(state, Position(4, 4)))

    def test_known_cell_at_returns_known_cell_for_seen_cell(self) -> None:
        state = reveal_observation(
            make_state(),
            center=Position(4, 4),
            contracts=self.contracts,
        )

        self.assertEqual(
            known_cell_at(state, Position(4, 4)),
            state.actual_map.cell_at(Position(4, 4)),
        )

    def test_snapshot_srs_state_uses_known_discovered_cells(self) -> None:
        state = reveal_observation(
            make_state(),
            center=Position(4, 4),
            contracts=self.contracts,
        )

        snapshot = snapshot_srs_state(state)

        self.assertEqual(snapshot.discovered_cells, state.known_state.discovered_cells)

    def test_restore_srs_state_restores_discovered_cells_and_known_cells(self) -> None:
        original = reveal_observation(
            make_state(),
            center=Position(4, 4),
            contracts=self.contracts,
        )
        persistent = snapshot_srs_state(original)

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=Position(0, 4),
            objects=original.objects,
        )

        self.assertEqual(restored.known_state.discovered_cells, original.known_state.discovered_cells)
        self.assertEqual(dict(restored.known_state.known_cells), dict(original.known_state.known_cells))
        self.assertEqual(restored.persistent_state, persistent)

    def test_restore_srs_state_does_not_restore_visited_cells(self) -> None:
        original = reveal_observation(
            make_state(),
            center=Position(4, 4),
            contracts=self.contracts,
        )
        persistent = snapshot_srs_state(original)

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=Position(0, 4),
            objects=original.objects,
        )

        self.assertEqual(restored.known_state.visited_cells, frozenset())
        self.assertEqual(restored.srs_turn, 0)
        self.assertEqual(restored.fuel, 0)
        self.assertEqual(restored.max_fuel, 0)


if __name__ == "__main__":
    unittest.main()
