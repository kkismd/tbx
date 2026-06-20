from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from types import MappingProxyType
from typing import Any, Mapping


class SrsModelError(ValueError):
    pass


class Direction(str, Enum):
    N = "N"
    E = "E"
    S = "S"
    W = "W"


class SectorType(str, Enum):
    NORMAL = "NORMAL"
    BASE = "BASE"
    RESOURCE = "RESOURCE"
    NEBULA = "NEBULA"
    ASTEROID = "ASTEROID"
    GRAVITY = "GRAVITY"
    RIFT = "RIFT"


class SrsTerrainType(str, Enum):
    FLOOR = "FLOOR"
    DEBRIS = "DEBRIS"
    NEBULA = "NEBULA"
    ASTEROID_FIELD = "ASTEROID_FIELD"
    ASTEROID = "ASTEROID"
    GRAVITY_FIELD_VERTICAL = "GRAVITY_FIELD_VERTICAL"
    GRAVITY_FIELD_HORIZONTAL = "GRAVITY_FIELD_HORIZONTAL"
    RIFT_DISTORTION = "RIFT_DISTORTION"
    RIFT_BARRIER = "RIFT_BARRIER"


class SrsObjectType(str, Enum):
    STAR = "STAR"
    PLANET = "PLANET"
    STATION = "STATION"
    RESOURCE_CACHE = "RESOURCE_CACHE"
    SALVAGE = "SALVAGE"


class SrsActorType(str, Enum):
    PLAYER = "PLAYER"


class CostMode(str, Enum):
    TURN_ONLY = "TURN_ONLY"
    SHARED_FUEL = "SHARED_FUEL"


class MovementRule(str, Enum):
    VECTOR_COMMAND = "VECTOR_COMMAND"
    MOVEMENT_POINTS = "MOVEMENT_POINTS"
    DIRECTIONAL_THRUST = "DIRECTIONAL_THRUST"


class ObservationMode(str, Enum):
    FULL = "FULL"
    LOCAL_MOVEMENT = "LOCAL_MOVEMENT"


class InteractionMode(str, Enum):
    AUTO_INTERACT = "AUTO_INTERACT"
    EXPLICIT_INTERACT = "EXPLICIT_INTERACT"


class CollisionBehavior(str, Enum):
    STOP_BEFORE = "STOP_BEFORE"


def _freeze_mapping(mapping: Mapping[Any, Any]) -> Mapping[Any, Any]:
    return MappingProxyType(dict(mapping))


@dataclass(frozen=True, slots=True)
class Position:
    x: int
    y: int


@dataclass(frozen=True, slots=True)
class SectorDescriptor:
    sector_id: str
    sector_type: SectorType
    sector_seed: int
    entry_edge: Direction
    blocked_edges: frozenset[Direction] = frozenset()

    def __post_init__(self) -> None:
        object.__setattr__(self, "blocked_edges", frozenset(self.blocked_edges))


@dataclass(frozen=True, slots=True)
class SrsObjectState:
    object_id: str
    object_type: SrsObjectType
    position: Position
    consumed: bool = False
    activated: bool = False
    metadata: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "metadata", _freeze_mapping(self.metadata))


@dataclass(frozen=True, slots=True)
class SrsCell:
    terrain: SrsTerrainType
    object_id: str | None = None
    actor_id: str | None = None
    warp_flags: frozenset[Direction] = frozenset()

    def __post_init__(self) -> None:
        object.__setattr__(self, "warp_flags", frozenset(self.warp_flags))


@dataclass(frozen=True, slots=True)
class SrsActualMap:
    width: int
    height: int
    cells: tuple[tuple[SrsCell, ...], ...]

    def __post_init__(self) -> None:
        normalized = tuple(tuple(row) for row in self.cells)
        if self.height != len(normalized):
            raise SrsModelError("height must match number of rows in cells")
        if any(len(row) != self.width for row in normalized):
            raise SrsModelError("each cells row must match map width")
        object.__setattr__(self, "cells", normalized)

    def contains(self, position: Position) -> bool:
        return 0 <= position.x < self.width and 0 <= position.y < self.height

    def cell_at(self, position: Position) -> SrsCell:
        if not self.contains(position):
            raise IndexError(f"position out of bounds: {position}")
        return self.cells[position.y][position.x]


@dataclass(frozen=True, slots=True)
class SrsKnownState:
    discovered_cells: frozenset[Position] = frozenset()
    known_cells: Mapping[Position, SrsCell] = field(default_factory=dict)
    visited_cells: frozenset[Position] = frozenset()

    def __post_init__(self) -> None:
        discovered_cells = frozenset(self.discovered_cells)
        known_cells = _freeze_mapping(self.known_cells)
        visited_cells = frozenset(self.visited_cells)

        if not set(known_cells).issubset(discovered_cells):
            raise SrsModelError("known_cells keys must be a subset of discovered_cells")

        object.__setattr__(self, "discovered_cells", discovered_cells)
        object.__setattr__(self, "known_cells", known_cells)
        object.__setattr__(self, "visited_cells", visited_cells)


@dataclass(frozen=True, slots=True)
class SrsPersistentState:
    generated_map_id: str
    generation_schema_version: int
    generation_seed: int
    sector_type: SectorType
    blocked_edges: frozenset[Direction]
    warp_flags: Mapping[Position, frozenset[Direction]] = field(default_factory=dict)
    celestial_body_positions: Mapping[str, Position] = field(default_factory=dict)
    consumed_object_ids: frozenset[str] = frozenset()
    activated_object_ids: frozenset[str] = frozenset()
    discovered_cells: frozenset[Position] = frozenset()

    def __post_init__(self) -> None:
        object.__setattr__(self, "blocked_edges", frozenset(self.blocked_edges))
        object.__setattr__(
            self,
            "warp_flags",
            _freeze_mapping({position: frozenset(flags) for position, flags in self.warp_flags.items()}),
        )
        object.__setattr__(self, "celestial_body_positions", _freeze_mapping(self.celestial_body_positions))
        object.__setattr__(self, "consumed_object_ids", frozenset(self.consumed_object_ids))
        object.__setattr__(self, "activated_object_ids", frozenset(self.activated_object_ids))
        object.__setattr__(self, "discovered_cells", frozenset(self.discovered_cells))


@dataclass(frozen=True, slots=True)
class SrsGameState:
    descriptor: SectorDescriptor
    actual_map: SrsActualMap
    known_state: SrsKnownState
    persistent_state: SrsPersistentState
    player_position: Position
    objects: Mapping[str, SrsObjectState] = field(default_factory=dict)
    srs_turn: int = 0
    fuel: int = 0
    max_fuel: int = 0

    def __post_init__(self) -> None:
        normalized_objects = _freeze_mapping(self.objects)
        if any(object_id != state.object_id for object_id, state in normalized_objects.items()):
            raise SrsModelError("objects mapping keys must match SrsObjectState.object_id")

        map_object_ids = {
            cell.object_id
            for row in self.actual_map.cells
            for cell in row
            if cell.object_id is not None
        }
        if map_object_ids != set(normalized_objects):
            raise SrsModelError("actual_map object_id values must match objects mapping keys")

        object.__setattr__(self, "objects", normalized_objects)


@dataclass(frozen=True, slots=True)
class SrsTurnEvent:
    srs_turn: int
    event_type: str
    payload: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "payload", _freeze_mapping(self.payload))


@dataclass(frozen=True, slots=True)
class SrsGameLog:
    events: tuple[SrsTurnEvent, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(self, "events", tuple(self.events))


@dataclass(frozen=True, slots=True)
class SrsCommand:
    command_type: str
    route: tuple[Direction, ...] = ()
    target: Position | None = None

    def __post_init__(self) -> None:
        command_type = str(self.command_type)
        try:
            route = tuple(Direction(direction) for direction in self.route)
        except ValueError as exc:
            raise SrsModelError("route must contain only Direction values") from exc

        if command_type == "MOVE_ROUTE" and not route:
            raise SrsModelError("MOVE_ROUTE requires a non-empty route")
        if command_type == "MOVE_TO" and self.target is None:
            raise SrsModelError("MOVE_TO requires a target")

        object.__setattr__(self, "command_type", command_type)
        object.__setattr__(self, "route", route)


@dataclass(frozen=True, slots=True)
class SrsCommandResult:
    state: SrsGameState
    events: tuple[SrsTurnEvent, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "events", tuple(self.events))


def validate_sector_descriptor(descriptor: SectorDescriptor) -> None:
    if descriptor.sector_type is SectorType.RIFT:
        if not descriptor.blocked_edges:
            raise SrsModelError("RIFT sector requires at least one blocked edge")
    elif descriptor.blocked_edges:
        raise SrsModelError("only RIFT sector may have blocked edges")

    if descriptor.entry_edge in descriptor.blocked_edges:
        raise SrsModelError("entry_edge must not be blocked")
