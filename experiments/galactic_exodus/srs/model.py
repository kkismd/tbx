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


class SrsWeaponType(str, Enum):
    PHOTON_TORPEDO = "PHOTON_TORPEDO"
    PHASER = "PHASER"
    ENEMY_WEAPON = "ENEMY_WEAPON"


class SrsEnemyTier(str, Enum):
    TIER1 = "TIER1"
    TIER2 = "TIER2"
    TIER3 = "TIER3"
    TIER4 = "TIER4"


class SrsCombatPhase(str, Enum):
    PLAYER_MOVEMENT = "PLAYER_MOVEMENT"
    PLAYER_ATTACK = "PLAYER_ATTACK"
    ENEMY_ACTION = "ENEMY_ACTION"


class SrsPlayerAttackAction(str, Enum):
    ATTACK = "ATTACK"
    SKIP = "SKIP"


class SrsEnemyReaction(str, Enum):
    COUNTERATTACK = "COUNTERATTACK"
    DEFEND = "DEFEND"


class ResourceManagementMode(str, Enum):
    NONE = "NONE"


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
class SrsWeaponProfile:
    weapon_type: SrsWeaponType
    damage: int | None
    range: int
    ammo_cost: int = 0
    energy_cost: int = 0
    resource_management: ResourceManagementMode = ResourceManagementMode.NONE

    def __post_init__(self) -> None:
        if self.damage is not None and self.damage <= 0:
            raise SrsModelError("weapon damage must be positive")
        if self.range <= 0:
            raise SrsModelError("weapon range must be positive")
        if self.ammo_cost < 0:
            raise SrsModelError("weapon ammo_cost must be non-negative")
        if self.energy_cost < 0:
            raise SrsModelError("weapon energy_cost must be non-negative")


@dataclass(frozen=True, slots=True)
class SrsPlayerCombatState:
    durability: int = 100
    defense: int = 0
    movement_power: int = 4
    photon_torpedo_ammo: int = 6
    photon_torpedo_ammo_capacity: int = 6
    energy: int = 6
    energy_capacity: int = 6
    energy_recovery: int = 1

    def __post_init__(self) -> None:
        if self.durability < 0:
            raise SrsModelError("player durability must be non-negative")
        if self.defense < 0:
            raise SrsModelError("player defense must be non-negative")
        if self.movement_power <= 0:
            raise SrsModelError("player movement_power must be positive")
        if self.photon_torpedo_ammo < 0:
            raise SrsModelError("player photon_torpedo_ammo must be non-negative")
        if self.photon_torpedo_ammo_capacity <= 0:
            raise SrsModelError("player photon_torpedo_ammo_capacity must be positive")
        if self.photon_torpedo_ammo > self.photon_torpedo_ammo_capacity:
            raise SrsModelError("player photon_torpedo_ammo must not exceed capacity")
        if self.energy < 0:
            raise SrsModelError("player energy must be non-negative")
        if self.energy_capacity <= 0:
            raise SrsModelError("player energy_capacity must be positive")
        if self.energy > self.energy_capacity:
            raise SrsModelError("player energy must not exceed capacity")
        if self.energy_recovery < 0:
            raise SrsModelError("player energy_recovery must be non-negative")


@dataclass(frozen=True, slots=True)
class SrsEnemyCombatState:
    enemy_id: str
    tier: SrsEnemyTier
    position: Position
    durability: int
    attack_damage: int
    movement_power: int

    def __post_init__(self) -> None:
        if self.enemy_id == "":
            raise SrsModelError("enemy_id must not be empty")
        if self.durability <= 0:
            raise SrsModelError("enemy durability must be positive")
        if self.attack_damage <= 0:
            raise SrsModelError("enemy attack_damage must be positive")
        if self.movement_power <= 0:
            raise SrsModelError("enemy movement_power must be positive")


def default_weapon_profiles() -> Mapping[SrsWeaponType, SrsWeaponProfile]:
    return _freeze_mapping(
        {
            SrsWeaponType.PHOTON_TORPEDO: SrsWeaponProfile(
                weapon_type=SrsWeaponType.PHOTON_TORPEDO,
                damage=3,
                range=3,
                ammo_cost=1,
            ),
            SrsWeaponType.PHASER: SrsWeaponProfile(
                weapon_type=SrsWeaponType.PHASER,
                damage=1,
                range=2,
                energy_cost=1,
            ),
            SrsWeaponType.ENEMY_WEAPON: SrsWeaponProfile(
                weapon_type=SrsWeaponType.ENEMY_WEAPON,
                damage=None,
                range=2,
                resource_management=ResourceManagementMode.NONE,
            ),
        }
    )


def enemy_tier_defaults(
    tier: SrsEnemyTier,
) -> tuple[int, int, int]:
    defaults = {
        SrsEnemyTier.TIER1: (3, 6, 3),
        SrsEnemyTier.TIER2: (5, 7, 3),
        SrsEnemyTier.TIER3: (8, 8, 3),
        SrsEnemyTier.TIER4: (12, 10, 3),
    }
    return defaults[tier]


def create_enemy_combat_state(
    *,
    enemy_id: str,
    tier: SrsEnemyTier,
    position: Position,
) -> SrsEnemyCombatState:
    durability, attack_damage, movement_power = enemy_tier_defaults(tier)
    return SrsEnemyCombatState(
        enemy_id=enemy_id,
        tier=tier,
        position=position,
        durability=durability,
        attack_damage=attack_damage,
        movement_power=movement_power,
    )


@dataclass(frozen=True, slots=True)
class SrsCombatState:
    player: SrsPlayerCombatState = field(default_factory=SrsPlayerCombatState)
    enemies: Mapping[str, SrsEnemyCombatState] = field(default_factory=dict)
    weapon_profiles: Mapping[SrsWeaponType, SrsWeaponProfile] = field(default_factory=default_weapon_profiles)
    phase: SrsCombatPhase = SrsCombatPhase.PLAYER_MOVEMENT
    combat_turn: int = 0
    player_attack_target_id: str | None = None

    def __post_init__(self) -> None:
        normalized_enemies = _freeze_mapping(self.enemies)
        normalized_weapons = _freeze_mapping(self.weapon_profiles)

        if any(enemy_id != state.enemy_id for enemy_id, state in normalized_enemies.items()):
            raise SrsModelError("enemies mapping keys must match SrsEnemyCombatState.enemy_id")
        if self.combat_turn < 0:
            raise SrsModelError("combat_turn must be non-negative")
        if self.player_attack_target_id is not None and self.player_attack_target_id not in normalized_enemies:
            raise SrsModelError("player_attack_target_id must reference an existing enemy")

        object.__setattr__(self, "enemies", normalized_enemies)
        object.__setattr__(self, "weapon_profiles", normalized_weapons)

    @property
    def enemy_presence(self) -> bool:
        return bool(self.enemies)

    @property
    def target_available(self) -> bool:
        return self.player_attack_target_id is not None and self.player_attack_target_id in self.enemies


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
    combat_state: SrsCombatState | None = None
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

        if self.combat_state is not None:
            for enemy in self.combat_state.enemies.values():
                if not self.actual_map.contains(enemy.position):
                    raise SrsModelError("combat enemy position must be within actual_map bounds")

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
    target_object_id: str | None = None
    exit_direction: Direction | None = None
    player_attack_action: SrsPlayerAttackAction | None = None
    player_attack_weapon: SrsWeaponType | None = None
    enemy_reactions: Mapping[str, SrsEnemyReaction] = field(default_factory=dict)

    def __post_init__(self) -> None:
        command_type = str(self.command_type)
        try:
            route = tuple(Direction(direction) for direction in self.route)
        except ValueError as exc:
            raise SrsModelError("route must contain only Direction values") from exc
        try:
            exit_direction = None if self.exit_direction is None else Direction(self.exit_direction)
        except ValueError as exc:
            raise SrsModelError("exit_direction must be a Direction value") from exc
        try:
            player_attack_action = (
                None
                if self.player_attack_action is None
                else SrsPlayerAttackAction(self.player_attack_action)
            )
        except ValueError as exc:
            raise SrsModelError("player_attack_action must be ATTACK or SKIP") from exc
        try:
            player_attack_weapon = (
                None
                if self.player_attack_weapon is None
                else SrsWeaponType(self.player_attack_weapon)
            )
        except ValueError as exc:
            raise SrsModelError("player_attack_weapon must be a SrsWeaponType value") from exc
        if player_attack_weapon is SrsWeaponType.ENEMY_WEAPON:
            raise SrsModelError("player_attack_weapon must be PHOTON_TORPEDO or PHASER")
        if not isinstance(self.enemy_reactions, Mapping):
            raise SrsModelError("enemy_reactions must be a mapping")
        try:
            enemy_reactions = _freeze_mapping(
                {
                    str(enemy_id): SrsEnemyReaction(reaction)
                    for enemy_id, reaction in self.enemy_reactions.items()
                }
            )
        except ValueError as exc:
            raise SrsModelError("enemy_reactions values must be COUNTERATTACK or DEFEND") from exc

        if command_type == "MOVE_ROUTE" and not route:
            raise SrsModelError("MOVE_ROUTE requires a non-empty route")
        if command_type == "MOVE_TO" and self.target is None:
            raise SrsModelError("MOVE_TO requires a target")
        if command_type == "INTERACT" and not self.target_object_id:
            raise SrsModelError("INTERACT requires a target_object_id")
        if command_type == "WARP_EXIT" and exit_direction is None:
            raise SrsModelError("WARP_EXIT requires an exit_direction")
        if command_type != "COMBAT_STEP" and (
            player_attack_action is not None
            or player_attack_weapon is not None
            or enemy_reactions
        ):
            raise SrsModelError("combat action fields require COMBAT_STEP")
        if player_attack_action is SrsPlayerAttackAction.ATTACK and player_attack_weapon is None:
            raise SrsModelError("ATTACK requires a player_attack_weapon")
        if player_attack_action is not SrsPlayerAttackAction.ATTACK and player_attack_weapon is not None:
            raise SrsModelError("player_attack_weapon requires ATTACK")

        object.__setattr__(self, "command_type", command_type)
        object.__setattr__(self, "route", route)
        object.__setattr__(self, "exit_direction", exit_direction)
        object.__setattr__(self, "player_attack_action", player_attack_action)
        object.__setattr__(self, "player_attack_weapon", player_attack_weapon)
        object.__setattr__(self, "enemy_reactions", enemy_reactions)

    def __hash__(self) -> int:
        return hash(
            (
                self.command_type,
                self.route,
                self.target,
                self.target_object_id,
                self.exit_direction,
                self.player_attack_action,
                self.player_attack_weapon,
                tuple(sorted(self.enemy_reactions.items())),
            )
        )


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


def derive_lrs_blocked_routes(
    descriptor: SectorDescriptor,
) -> frozenset[tuple[str, Direction]]:
    if descriptor.sector_type is not SectorType.RIFT:
        return frozenset()
    return frozenset(
        (descriptor.sector_id, direction)
        for direction in descriptor.blocked_edges
    )
