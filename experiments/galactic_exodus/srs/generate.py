from __future__ import annotations

import random
from typing import Iterable

from experiments.galactic_exodus.srs.contracts import SrsContracts
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsActualMap,
    SrsCell,
    SrsGameState,
    SrsKnownState,
    SrsObjectState,
    SrsObjectType,
    SrsPersistentState,
    SrsTerrainType,
    validate_sector_descriptor as validate_sector_descriptor,
)


class SrsGenerationError(ValueError):
    pass


MAP_WIDTH = 9
MAP_HEIGHT = 9
MAP_CENTER = Position(4, 4)

# Internal SRS coordinates are 0-origin lower-left:
# x increases eastward and y increases northward.
EDGE_POSITIONS = {
    Direction.N: Position(4, 8),
    Direction.E: Position(8, 4),
    Direction.S: Position(4, 0),
    Direction.W: Position(0, 4),
}

SECTOR_EXTRA_OBJECTS = {
    SectorType.NORMAL: ((SrsObjectType.SALVAGE, "salvage-1"),),
    SectorType.BASE: ((SrsObjectType.STATION, "station-1"),),
    SectorType.RESOURCE: ((SrsObjectType.RESOURCE_CACHE, "resource-cache-1"),),
    SectorType.RIFT: ((SrsObjectType.SALVAGE, "salvage-1"),),
}


def resource_cache_restore_values(cache_count: int) -> tuple[int, ...]:
    if cache_count < 0:
        raise SrsGenerationError("resource cache count must be non-negative")
    return tuple(3 for _ in range(cache_count))


def create_sector(
    descriptor: SectorDescriptor,
    *,
    width: int = MAP_WIDTH,
    height: int = MAP_HEIGHT,
    contracts: SrsContracts,
) -> SrsGameState:
    validate_sector_descriptor(descriptor)
    if width != MAP_WIDTH or height != MAP_HEIGHT:
        raise SrsGenerationError("only 9x9 sector generation is supported")

    cells = _make_floor_cells(width=width, height=height)
    _apply_rift_barriers(cells, descriptor.blocked_edges)
    _apply_warp_flags(cells, descriptor)

    player_position = EDGE_POSITIONS[descriptor.entry_edge]
    objects = _place_objects(cells, descriptor, player_position)

    actual_map = SrsActualMap(
        width=width,
        height=height,
        cells=tuple(tuple(row) for row in cells),
    )
    known_state = SrsKnownState(discovered_cells=frozenset())
    persistent_state = SrsPersistentState(
        generated_map_id=f"{descriptor.sector_id}:{descriptor.sector_seed}",
        generation_schema_version=contracts.generation.generation_schema_version,
        generation_seed=descriptor.sector_seed,
        sector_type=descriptor.sector_type,
        blocked_edges=descriptor.blocked_edges,
        warp_flags={
            position: cell.warp_flags
            for position, cell in _iter_cells(actual_map)
            if cell.warp_flags
        },
        celestial_body_positions={
            object_id: state.position
            for object_id, state in objects.items()
            if state.object_type in {SrsObjectType.STAR, SrsObjectType.PLANET}
        },
        consumed_object_ids=frozenset(),
        activated_object_ids=frozenset(),
        discovered_cells=frozenset(),
    )
    return SrsGameState(
        descriptor=descriptor,
        actual_map=actual_map,
        known_state=known_state,
        persistent_state=persistent_state,
        player_position=player_position,
        objects=objects,
        srs_turn=0,
        fuel=0,
        max_fuel=0,
    )


def _make_floor_cells(*, width: int, height: int) -> list[list[SrsCell]]:
    return [
        [SrsCell(terrain=SrsTerrainType.FLOOR) for _ in range(width)]
        for _ in range(height)
    ]


def _apply_rift_barriers(cells: list[list[SrsCell]], blocked_edges: frozenset[Direction]) -> None:
    for edge in blocked_edges:
        if edge is Direction.N:
            for x in range(len(cells[-1])):
                cells[-1][x] = SrsCell(terrain=SrsTerrainType.RIFT_BARRIER)
        elif edge is Direction.E:
            for row in cells:
                row[-1] = SrsCell(terrain=SrsTerrainType.RIFT_BARRIER)
        elif edge is Direction.S:
            for x in range(len(cells[0])):
                cells[0][x] = SrsCell(terrain=SrsTerrainType.RIFT_BARRIER)
        elif edge is Direction.W:
            for row in cells:
                row[0] = SrsCell(terrain=SrsTerrainType.RIFT_BARRIER)


def _apply_warp_flags(cells: list[list[SrsCell]], descriptor: SectorDescriptor) -> None:
    for direction in Direction:
        if direction in descriptor.blocked_edges:
            continue
        warp_positions = [
            position
            for position in _edge_cells(width=len(cells[0]), height=len(cells), direction=direction)
            if _has_floor_square(cells, position)
        ]
        if not warp_positions:
            raise SrsGenerationError(f"open edge {direction.value} has no warp candidate")
        for position in warp_positions:
            _add_warp_flag(cells, position, direction)


def _edge_cells(*, width: int, height: int, direction: Direction) -> Iterable[Position]:
    if direction is Direction.N:
        return (Position(x, height - 1) for x in range(width))
    if direction is Direction.E:
        return (Position(width - 1, y) for y in range(height))
    if direction is Direction.S:
        return (Position(x, 0) for x in range(width))
    return (Position(0, y) for y in range(height))


def _has_floor_square(cells: list[list[SrsCell]], position: Position) -> bool:
    width = len(cells[0])
    height = len(cells)
    for min_x in range(position.x - 1, position.x + 1):
        for min_y in range(position.y - 1, position.y + 1):
            if min_x < 0 or min_y < 0:
                continue
            if min_x + 1 >= width or min_y + 1 >= height:
                continue
            square = (
                cells[min_y][min_x],
                cells[min_y][min_x + 1],
                cells[min_y + 1][min_x],
                cells[min_y + 1][min_x + 1],
            )
            if all(cell.terrain is SrsTerrainType.FLOOR for cell in square):
                return True
    return False


def _add_warp_flag(cells: list[list[SrsCell]], position: Position, direction: Direction) -> None:
    base = cells[position.y][position.x]
    cells[position.y][position.x] = SrsCell(
        terrain=base.terrain,
        object_id=base.object_id,
        actor_id=base.actor_id,
        warp_flags=base.warp_flags | frozenset({direction}),
    )


def _place_objects(
    cells: list[list[SrsCell]],
    descriptor: SectorDescriptor,
    player_position: Position,
) -> dict[str, SrsObjectState]:
    object_specs = [
        (SrsObjectType.STAR, "star-1"),
        (SrsObjectType.PLANET, "planet-1"),
        (SrsObjectType.PLANET, "planet-2"),
        *SECTOR_EXTRA_OBJECTS.get(descriptor.sector_type, ()),
    ]
    candidates = _collect_object_candidates(cells, player_position=player_position)
    if len(candidates) < len(object_specs):
        raise SrsGenerationError("not enough placement candidates for requested objects")

    rng = random.Random(descriptor.sector_seed)
    shuffled = list(candidates)
    rng.shuffle(shuffled)

    objects: dict[str, SrsObjectState] = {}
    for (object_type, object_id), position in zip(object_specs, shuffled[: len(object_specs)], strict=True):
        cells[position.y][position.x] = SrsCell(
            terrain=cells[position.y][position.x].terrain,
            object_id=object_id,
            actor_id=cells[position.y][position.x].actor_id,
            warp_flags=cells[position.y][position.x].warp_flags,
        )
        objects[object_id] = SrsObjectState(
            object_id=object_id,
            object_type=object_type,
            position=position,
        )
    _assign_resource_cache_metadata(objects)
    return objects


def _assign_resource_cache_metadata(objects: dict[str, SrsObjectState]) -> None:
    resource_cache_ids = sorted(
        object_id
        for object_id, state in objects.items()
        if state.object_type is SrsObjectType.RESOURCE_CACHE
    )
    restore_values = resource_cache_restore_values(len(resource_cache_ids))
    for object_id, fuel_restore in zip(resource_cache_ids, restore_values, strict=True):
        resource_cache = objects[object_id]
        objects[object_id] = SrsObjectState(
            object_id=resource_cache.object_id,
            object_type=resource_cache.object_type,
            position=resource_cache.position,
            consumed=resource_cache.consumed,
            activated=resource_cache.activated,
            metadata={"fuel_restore": fuel_restore},
        )


def _collect_object_candidates(
    cells: list[list[SrsCell]],
    *,
    player_position: Position,
) -> list[Position]:
    candidates: list[Position] = []
    for y, row in enumerate(cells):
        for x, cell in enumerate(row):
            position = Position(x, y)
            if position == player_position:
                continue
            if cell.terrain is not SrsTerrainType.FLOOR:
                continue
            if cell.warp_flags:
                continue
            if cell.object_id is not None:
                continue
            candidates.append(position)
    return candidates


def _iter_cells(actual_map: SrsActualMap) -> Iterable[tuple[Position, SrsCell]]:
    for y, row in enumerate(actual_map.cells):
        for x, cell in enumerate(row):
            yield Position(x, y), cell
