from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import (
    apply_srs_command,
    known_cell_at,
    observation_area,
    observation_size_for_terrain,
    restore_srs_state,
    reveal_full_observation,
    reveal_observation,
    snapshot_srs_state,
)
from experiments.galactic_exodus.srs.generate import create_sector
from experiments.galactic_exodus.srs.log import INTERACT_ACCEPTED
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsActualMap,
    SrsCell,
    SrsCommand,
    SrsGameState,
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
    row_idx, col_idx = state.actual_map.indices_for(position)
    rows[row_idx][col_idx] = SrsCell(
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

        area = observation_area(state.actual_map, center=Position(1, 1), size=5)

        self.assertEqual(len(area), 9)
        self.assertIn(Position(1, 1), area)
        self.assertIn(Position(3, 3), area)
        self.assertNotIn(Position(4, 4), area)

    def test_observation_area_rejects_out_of_bounds_center(self) -> None:
        state = make_state()

        with self.assertRaisesRegex(ValueError, "out of bounds"):
            observation_area(state.actual_map, center=Position(-1, 1), size=5)

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
            center=Position(5, 5),
            contracts=self.contracts,
        )

        self.assertEqual(len(revealed.known_state.discovered_cells), 25)
        self.assertEqual(len(revealed.known_state.known_cells), 25)
        self.assertEqual(revealed.persistent_state.discovered_cells, revealed.known_state.discovered_cells)

    def test_local_movement_nebula_reveals_3x3(self) -> None:
        state = replace_cell_terrain(make_state(), Position(5, 5), SrsTerrainType.NEBULA)

        revealed = reveal_observation(
            state,
            center=Position(5, 5),
            contracts=self.contracts,
        )

        self.assertEqual(len(revealed.known_state.discovered_cells), 9)
        self.assertEqual(len(revealed.known_state.known_cells), 9)

    def test_known_map_is_cumulative(self) -> None:
        state = make_state()
        first = reveal_observation(
            state,
            center=Position(5, 5),
            contracts=self.contracts,
        )

        second = reveal_observation(
            first,
            center=Position(7, 5),
            contracts=self.contracts,
        )

        self.assertEqual(len(first.known_state.discovered_cells), 25)
        self.assertEqual(len(second.known_state.discovered_cells), 35)
        self.assertTrue(first.known_state.discovered_cells.issubset(second.known_state.discovered_cells))

    def test_reveal_observation_marks_center_visited(self) -> None:
        state = make_state()

        revealed = reveal_observation(
            state,
            center=Position(5, 5),
            contracts=self.contracts,
        )

        self.assertEqual(revealed.known_state.visited_cells, frozenset({Position(5, 5)}))

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

        self.assertIsNone(known_cell_at(state, Position(5, 5)))

    def test_known_cell_at_returns_known_cell_for_seen_cell(self) -> None:
        state = reveal_observation(
            make_state(),
            center=Position(5, 5),
            contracts=self.contracts,
        )

        self.assertEqual(
            known_cell_at(state, Position(5, 5)),
            state.actual_map.cell_at(Position(5, 5)),
        )

    def test_snapshot_srs_state_uses_known_discovered_cells(self) -> None:
        state = reveal_observation(
            make_state(),
            center=Position(5, 5),
            contracts=self.contracts,
        )

        snapshot = snapshot_srs_state(state)

        self.assertEqual(snapshot.discovered_cells, state.known_state.discovered_cells)

    def test_restore_srs_state_restores_discovered_cells_and_known_cells(self) -> None:
        original = reveal_observation(
            make_state(),
            center=Position(5, 5),
            contracts=self.contracts,
        )
        persistent = snapshot_srs_state(original)

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=Position(1, 5),
            objects=original.objects,
        )

        self.assertEqual(restored.known_state.discovered_cells, original.known_state.discovered_cells)
        self.assertEqual(dict(restored.known_state.known_cells), dict(original.known_state.known_cells))
        self.assertEqual(restored.persistent_state, persistent)

    def test_restore_srs_state_does_not_restore_visited_cells(self) -> None:
        original = reveal_observation(
            make_state(),
            center=Position(5, 5),
            contracts=self.contracts,
        )
        persistent = snapshot_srs_state(original)

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=Position(1, 5),
            objects=original.objects,
        )

        self.assertEqual(restored.known_state.visited_cells, frozenset())
        self.assertEqual(restored.srs_turn, 0)
        self.assertEqual(restored.fuel, 0)
        self.assertEqual(restored.max_fuel, 0)

    def test_restore_srs_state_applies_persistent_consumed_and_activated_flags(self) -> None:
        resource_state = make_state(sector_type=SectorType.RESOURCE)
        resource_cache_id = next(
            object_id
            for object_id, object_state in resource_state.objects.items()
            if object_state.object_type is SrsObjectType.RESOURCE_CACHE
        )
        resource_persistent = replace(
            resource_state.persistent_state,
            consumed_object_ids=frozenset({resource_cache_id}),
        )
        restored_resource = restore_srs_state(
            descriptor=resource_state.descriptor,
            actual_map=resource_state.actual_map,
            persistent=resource_persistent,
            player_position=resource_state.player_position,
            objects=resource_state.objects,
        )
        self.assertTrue(restored_resource.objects[resource_cache_id].consumed)

        base_state = make_state(sector_type=SectorType.BASE)
        station_id = next(
            object_id
            for object_id, object_state in base_state.objects.items()
            if object_state.object_type is SrsObjectType.STATION
        )
        base_persistent = replace(
            base_state.persistent_state,
            activated_object_ids=frozenset({station_id}),
        )
        restored_base = restore_srs_state(
            descriptor=base_state.descriptor,
            actual_map=base_state.actual_map,
            persistent=base_persistent,
            player_position=base_state.player_position,
            objects=base_state.objects,
        )
        self.assertTrue(restored_base.objects[station_id].activated)

    def test_snapshot_after_warp_exit_keeps_existing_persistent_fields(self) -> None:
        state = reveal_observation(
            make_state(sector_type=SectorType.RIFT, sector_seed=4001, blocked_edges=frozenset({Direction.N})),
            center=Position(5, 9),
            contracts=self.contracts,
        )
        state = replace(
            state,
            persistent_state=replace(
                state.persistent_state,
                consumed_object_ids=frozenset({"salvage-1"}),
                activated_object_ids=frozenset({"station-1"}),
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
            contracts=self.contracts,
        )
        snapshot = snapshot_srs_state(result.state)

        self.assertEqual(snapshot.generated_map_id, state.persistent_state.generated_map_id)
        self.assertEqual(snapshot.blocked_edges, state.persistent_state.blocked_edges)
        self.assertEqual(dict(snapshot.warp_flags), dict(state.persistent_state.warp_flags))
        self.assertEqual(snapshot.celestial_body_positions, state.persistent_state.celestial_body_positions)
        self.assertEqual(snapshot.consumed_object_ids, state.persistent_state.consumed_object_ids)
        self.assertEqual(snapshot.activated_object_ids, state.persistent_state.activated_object_ids)
        self.assertEqual(snapshot.discovered_cells, result.state.known_state.discovered_cells)

    def test_revisit_restores_consumed_resource_cache(self) -> None:
        original = make_state(sector_type=SectorType.RESOURCE)
        resource_cache_id = next(
            object_id
            for object_id, object_state in original.objects.items()
            if object_state.object_type is SrsObjectType.RESOURCE_CACHE
        )
        persistent = replace(
            original.persistent_state,
            consumed_object_ids=frozenset({resource_cache_id}),
        )

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=original.player_position,
            objects=original.objects,
        )

        self.assertTrue(restored.objects[resource_cache_id].consumed)

    def test_revisit_restores_consumed_salvage(self) -> None:
        original = make_state(sector_type=SectorType.NORMAL)
        salvage_id = next(
            object_id
            for object_id, object_state in original.objects.items()
            if object_state.object_type is SrsObjectType.SALVAGE
        )
        persistent = replace(
            original.persistent_state,
            consumed_object_ids=frozenset({salvage_id}),
        )

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=original.player_position,
            objects=original.objects,
        )

        self.assertTrue(restored.objects[salvage_id].consumed)

    def test_revisit_restores_activated_station_but_keeps_reusable_behavior(self) -> None:
        original = make_state(sector_type=SectorType.BASE)
        station_id, station_state = next(
            (object_id, object_state)
            for object_id, object_state in original.objects.items()
            if object_state.object_type is SrsObjectType.STATION
        )
        persistent = replace(
            original.persistent_state,
            activated_object_ids=frozenset({station_id}),
        )
        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=Position(station_state.position.x, station_state.position.y + 1),
            objects=original.objects,
        )
        restored = replace(restored, fuel=1, max_fuel=9)

        result = apply_srs_command(
            restored,
            SrsCommand(command_type="INTERACT", target_object_id=station_id),
            contracts=self.contracts,
        )

        self.assertTrue(restored.objects[station_id].activated)
        self.assertEqual(result.events[0].event_type, INTERACT_ACCEPTED)
        self.assertEqual(result.state.fuel, 9)

    def test_revisit_resets_turn_and_fuel(self) -> None:
        original = replace(
            make_state(sector_type=SectorType.BASE),
            srs_turn=3,
            fuel=5,
            max_fuel=9,
        )
        persistent = snapshot_srs_state(original)

        restored = restore_srs_state(
            descriptor=original.descriptor,
            actual_map=original.actual_map,
            persistent=persistent,
            player_position=original.player_position,
            objects=original.objects,
        )

        self.assertEqual(restored.srs_turn, 0)
        self.assertEqual(restored.fuel, 0)
        self.assertEqual(restored.max_fuel, 0)


if __name__ == "__main__":
    unittest.main()
