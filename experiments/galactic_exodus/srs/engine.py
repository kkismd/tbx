from __future__ import annotations

from dataclasses import replace
from typing import Mapping

from experiments.galactic_exodus.srs.contracts import SrsContracts
from experiments.galactic_exodus.srs.model import (
    Position,
    SectorDescriptor,
    SrsActualMap,
    SrsCell,
    SrsGameState,
    SrsKnownState,
    SrsObjectState,
    SrsPersistentState,
    SrsTerrainType,
)


class SrsObservationError(ValueError):
    pass


def observation_size_for_terrain(
    terrain: SrsTerrainType,
    contracts: SrsContracts,
) -> int:
    observation = contracts.movement.observation.get("LOCAL_MOVEMENT")
    if not isinstance(observation, Mapping):
        raise SrsObservationError("LOCAL_MOVEMENT observation contract is required")

    default_size = _validated_observation_size(observation.get("default_size"), field_name="default_size")
    nebula_size = _validated_observation_size(observation.get("nebula_size"), field_name="nebula_size")

    if terrain is SrsTerrainType.NEBULA:
        return nebula_size
    return default_size


def observation_area(
    actual_map: SrsActualMap,
    *,
    center: Position,
    size: int,
) -> frozenset[Position]:
    if not actual_map.contains(center):
        raise SrsObservationError(f"observation center out of bounds: {center}")
    radius = _validated_observation_size(size, field_name="size") // 2
    positions = {
        Position(x, y)
        for y in range(center.y - radius, center.y + radius + 1)
        for x in range(center.x - radius, center.x + radius + 1)
        if 0 <= x < actual_map.width and 0 <= y < actual_map.height
    }
    return frozenset(positions)


def reveal_observation(
    state: SrsGameState,
    *,
    center: Position,
    contracts: SrsContracts,
    mark_visited: bool = True,
) -> SrsGameState:
    if not state.actual_map.contains(center):
        raise SrsObservationError(f"observation center out of bounds: {center}")

    terrain = state.actual_map.cell_at(center).terrain
    size = observation_size_for_terrain(terrain, contracts)
    revealed_positions = observation_area(state.actual_map, center=center, size=size)
    known_cells = dict(state.known_state.known_cells)
    for position in revealed_positions:
        known_cells[position] = state.actual_map.cell_at(position)

    discovered_cells = state.known_state.discovered_cells | revealed_positions
    visited_cells = state.known_state.visited_cells
    if mark_visited:
        visited_cells = visited_cells | frozenset({center})

    known_state = SrsKnownState(
        discovered_cells=discovered_cells,
        known_cells=known_cells,
        visited_cells=visited_cells,
    )
    persistent_state = replace(
        state.persistent_state,
        discovered_cells=known_state.discovered_cells,
    )
    return replace(
        state,
        known_state=known_state,
        persistent_state=persistent_state,
    )


def reveal_full_observation(
    state: SrsGameState,
) -> SrsGameState:
    discovered_cells = frozenset(_iter_positions(state.actual_map))
    known_cells = {
        position: state.actual_map.cell_at(position)
        for position in discovered_cells
    }
    known_state = SrsKnownState(
        discovered_cells=discovered_cells,
        known_cells=known_cells,
        visited_cells=state.known_state.visited_cells,
    )
    persistent_state = replace(
        state.persistent_state,
        discovered_cells=discovered_cells,
    )
    return replace(
        state,
        known_state=known_state,
        persistent_state=persistent_state,
    )


def known_cell_at(
    state: SrsGameState,
    position: Position,
) -> SrsCell | None:
    return state.known_state.known_cells.get(position)


def snapshot_srs_state(
    state: SrsGameState,
) -> SrsPersistentState:
    return replace(
        state.persistent_state,
        discovered_cells=state.known_state.discovered_cells,
    )


def restore_srs_state(
    *,
    descriptor: SectorDescriptor,
    actual_map: SrsActualMap,
    persistent: SrsPersistentState,
    player_position: Position,
    objects: Mapping[str, SrsObjectState],
) -> SrsGameState:
    discovered_cells = frozenset(persistent.discovered_cells)
    _validate_positions_within_map(discovered_cells, actual_map=actual_map, context="persistent discovered cell")
    known_cells = {
        position: actual_map.cell_at(position)
        for position in discovered_cells
    }
    return SrsGameState(
        descriptor=descriptor,
        actual_map=actual_map,
        known_state=SrsKnownState(
            discovered_cells=discovered_cells,
            known_cells=known_cells,
            visited_cells=frozenset(),
        ),
        persistent_state=persistent,
        player_position=player_position,
        objects=objects,
        srs_turn=0,
        fuel=0,
        max_fuel=0,
    )


def _validated_observation_size(value: object, *, field_name: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0 or value % 2 == 0:
        raise SrsObservationError(f"{field_name} must be an odd positive integer")
    return value


def _iter_positions(actual_map: SrsActualMap):
    for y, row in enumerate(actual_map.cells):
        for x, _ in enumerate(row):
            yield Position(x, y)


def _validate_positions_within_map(
    positions: frozenset[Position],
    *,
    actual_map: SrsActualMap,
    context: str,
) -> None:
    for position in positions:
        if not actual_map.contains(position):
            raise SrsObservationError(f"{context} out of bounds: {position}")
