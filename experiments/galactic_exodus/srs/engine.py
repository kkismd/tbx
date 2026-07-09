from __future__ import annotations

from collections import deque
from dataclasses import replace
from heapq import heappop, heappush
from math import ceil
from typing import Any, Mapping, Sequence

from experiments.galactic_exodus.srs.contracts import SrsContracts
from experiments.galactic_exodus.srs.log import (
    COMBAT_REJECTED,
    COMBAT_TRANSITIONED,
    INTERACT_ACCEPTED,
    INTERACT_REJECTED,
    MOVE_ACCEPTED,
    MOVE_REJECTED,
    OBJECT_CONSUMED,
    OBSERVATION_UPDATED,
    STATION_ACTIVATED,
    STOPPED_BEFORE_IMPASSABLE,
    WAIT_ACCEPTED,
    WARP_EXIT_ACCEPTED,
    WARP_EXIT_REJECTED,
    make_turn_event,
)
from experiments.galactic_exodus.srs.model import (
    SrsBaseUpgrade,
    Direction,
    CostMode,
    MovementRule,
    Position,
    SectorDescriptor,
    SrsActualMap,
    SrsCell,
    SrsCombatPhase,
    SrsCommand,
    SrsCommandResult,
    SrsEnemyCombatState,
    SrsEnemyReaction,
    SrsEnemyTier,
    SrsGameState,
    SrsKnownState,
    SrsObjectType,
    SrsObjectState,
    SrsPlayerAttackAction,
    SrsPlayerCombatState,
    SrsSalvageChoice,
    SrsPersistentState,
    SrsTerrainType,
    SrsWeaponType,
    default_weapon_profiles,
)


class SrsObservationError(ValueError):
    pass


class SrsMovementError(ValueError):
    pass


class SrsInteractionError(ValueError):
    pass


class SrsCombatError(ValueError):
    pass


_PLAYER_ATTACK_WEAPONS = (
    SrsWeaponType.PHOTON_TORPEDO,
    SrsWeaponType.PHASER,
)
_ENEMY_PATH_TIEBREAK_DIRECTIONS = (
    Direction.N,
    Direction.W,
    Direction.E,
    Direction.S,
)
_RESOURCE_CACHE_FUEL_RESTORE = 3
_SALVAGE_BASE_VALUE = 1
_SUPPORTED_SALVAGE_RECOVERY_CHOICES = frozenset(
    {
        SrsSalvageChoice.RECOVER_ENERGY,
        SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO,
        SrsSalvageChoice.STORE_ONLY,
    }
)
_SALVAGE_BASE_RECOVERY = {
    SrsSalvageChoice.RECOVER_ENERGY: 2,
    SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO: 1,
    SrsSalvageChoice.STORE_ONLY: 0,
}
_SALVAGE_DROP_VALUES = {
    SrsEnemyTier.TIER1: 1,
    SrsEnemyTier.TIER2: 1,
    SrsEnemyTier.TIER3: 2,
    SrsEnemyTier.TIER4: 3,
}
_SALVAGE_DROP_RECOVERY = {
    SrsEnemyTier.TIER1: {
        SrsSalvageChoice.RECOVER_ENERGY: 2,
        SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO: 1,
        SrsSalvageChoice.STORE_ONLY: 0,
    },
    SrsEnemyTier.TIER2: {
        SrsSalvageChoice.RECOVER_ENERGY: 2,
        SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO: 1,
        SrsSalvageChoice.STORE_ONLY: 0,
    },
    SrsEnemyTier.TIER3: {
        SrsSalvageChoice.RECOVER_ENERGY: 3,
        SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO: 1,
        SrsSalvageChoice.STORE_ONLY: 0,
    },
    SrsEnemyTier.TIER4: {
        SrsSalvageChoice.RECOVER_ENERGY: 4,
        SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO: 2,
        SrsSalvageChoice.STORE_ONLY: 0,
    },
}
_BASE_UPGRADE_COSTS = {
    SrsBaseUpgrade.PHASER_POWER: 4,
    SrsBaseUpgrade.PHOTON_TORPEDO_POWER: 5,
    SrsBaseUpgrade.ENERGY_CAPACITY: 3,
    SrsBaseUpgrade.PHOTON_TORPEDO_AMMO_CAPACITY: 3,
    SrsBaseUpgrade.DEFENSE: 4,
    SrsBaseUpgrade.EVASION: 4,
}
_ENEMY_DROP_REWARD_SOURCE = "ENEMY_DROP"
_MAP_PICKUP_REWARD_SOURCE = "MAP_PICKUP"
_ENEMY_DROP_SKIP_OCCUPIED_CELL = "OCCUPIED_CELL"
_ENEMY_DROP_SKIP_OBJECT_ID_COLLISION = "OBJECT_ID_COLLISION"


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


def _current_player_state(state: SrsGameState) -> SrsPlayerCombatState:
    if state.combat_state is not None:
        return state.combat_state.player
    return state.player_state


def _replace_player_state(
    state: SrsGameState,
    player_state: SrsPlayerCombatState,
) -> SrsGameState:
    updated_state = replace(state, player_state=player_state)
    if updated_state.combat_state is None:
        return updated_state
    return replace(
        updated_state,
        combat_state=replace(updated_state.combat_state, player=player_state),
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
    normalized_objects = _apply_persistent_object_flags(objects, persistent=persistent)
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
        objects=normalized_objects,
        srs_turn=0,
        fuel=0,
        max_fuel=0,
    )


def movement_raw_cost_for_step(
    destination: SrsCell,
    *,
    direction: Direction,
    contracts: SrsContracts,
) -> int:
    if direction not in contracts.movement.directions:
        raise SrsMovementError(f"unsupported movement direction: {direction}")
    return _orthogonal_step_raw_cost(destination, contracts=contracts)


def _orthogonal_step_raw_cost(
    destination: SrsCell,
    *,
    contracts: SrsContracts,
) -> int:
    terrain_multipliers = {
        SrsTerrainType.FLOOR: 1,
        SrsTerrainType.DEBRIS: 2,
        SrsTerrainType.NEBULA: 2,
        SrsTerrainType.ASTEROID_FIELD: 3,
        SrsTerrainType.GRAVITY_FIELD_VERTICAL: 1,
        SrsTerrainType.GRAVITY_FIELD_HORIZONTAL: 1,
        SrsTerrainType.RIFT_DISTORTION: 1,
    }
    multiplier = terrain_multipliers.get(destination.terrain)
    if multiplier is None:
        raise SrsMovementError(f"impassable terrain has no movement cost: {destination.terrain.value}")
    return _movement_raw_cost_denominator(contracts) * multiplier


def is_impassable_cell(
    state: SrsGameState,
    position: Position,
) -> bool:
    if not state.actual_map.contains(position):
        return True

    cell = state.actual_map.cell_at(position)
    if cell.terrain in {SrsTerrainType.ASTEROID, SrsTerrainType.RIFT_BARRIER}:
        return True
    if cell.object_id is None:
        return False

    object_type = state.objects[cell.object_id].object_type
    return object_type in {
        SrsObjectType.STAR,
        SrsObjectType.PLANET,
        SrsObjectType.STATION,
    }


def bresenham_line(
    start: Position,
    end: Position,
) -> tuple[Position, ...]:
    x0 = start.x
    y0 = start.y
    x1 = end.x
    y1 = end.y
    dx = abs(x1 - x0)
    dy = abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx - dy

    line: list[Position] = []
    while True:
        line.append(Position(x0, y0))
        if x0 == x1 and y0 == y1:
            return tuple(line)

        err_twice = err * 2
        if err_twice > -dy:
            err -= dy
            x0 += sx
        if err_twice < dx:
            err += dx
            y0 += sy


def combat_range_distance(
    attacker: Position,
    target: Position,
) -> int:
    return max(abs(attacker.x - target.x), abs(attacker.y - target.y))


def has_clear_line_of_sight(
    state: SrsGameState,
    *,
    attacker: Position,
    target: Position,
) -> bool:
    if not state.actual_map.contains(attacker):
        raise SrsCombatError(f"attacker position out of bounds: {attacker}")
    if not state.actual_map.contains(target):
        raise SrsCombatError(f"target position out of bounds: {target}")

    for position in bresenham_line(attacker, target)[1:-1]:
        if is_impassable_cell(state, position):
            return False
    return True


def is_attackable_position(
    state: SrsGameState,
    *,
    attacker: Position,
    target: Position,
    weapon_type: SrsWeaponType,
) -> bool:
    weapon_profiles = default_weapon_profiles() if state.combat_state is None else state.combat_state.weapon_profiles
    weapon_profile = weapon_profiles.get(weapon_type)
    if weapon_profile is None:
        raise SrsCombatError(f"missing weapon profile: {weapon_type.value}")

    if combat_range_distance(attacker, target) > weapon_profile.range:
        return False
    return has_clear_line_of_sight(state, attacker=attacker, target=target)


def enemy_attackable_positions(
    state: SrsGameState,
) -> tuple[Position, ...]:
    player_position = state.player_position
    attackable_positions = []
    for position in _iter_positions(state.actual_map):
        if position == player_position:
            continue
        if is_impassable_cell(state, position):
            continue
        if is_attackable_position(
            state,
            attacker=position,
            target=player_position,
            weapon_type=SrsWeaponType.ENEMY_WEAPON,
        ):
            attackable_positions.append(position)
    return tuple(sorted(attackable_positions, key=_position_sort_key))


def resolve_move_route(
    state: SrsGameState,
    route: Sequence[Direction],
    *,
    contracts: SrsContracts,
) -> tuple[tuple[Position, ...], Position | None, int]:
    entered_cells: list[Position] = []
    blocked_position: Position | None = None
    raw_cost = 0
    current = state.player_position

    for direction in route:
        next_position = _step_position(current, direction)
        if is_impassable_cell(state, next_position):
            blocked_position = next_position
            break

        destination = state.actual_map.cell_at(next_position)
        step_cost = movement_raw_cost_for_step(destination, direction=direction, contracts=contracts)
        if raw_cost + step_cost > contracts.movement.movement_cost_budget_raw:
            break

        entered_cells.append(next_position)
        raw_cost += step_cost
        current = next_position

    return tuple(entered_cells), blocked_position, raw_cost


def route_to_known_target(
    state: SrsGameState,
    target: Position,
) -> tuple[Direction, ...]:
    start = state.player_position
    if start == target:
        return ()

    came_from: dict[Position, tuple[Position, Direction] | None] = {start: None}
    frontier: deque[Position] = deque([start])
    directions = (Direction.N, Direction.E, Direction.S, Direction.W)

    while frontier:
        current = frontier.popleft()
        for direction in directions:
            next_position = _step_position(current, direction)
            if next_position in came_from:
                continue
            if next_position not in state.known_state.discovered_cells:
                continue
            if not state.actual_map.contains(next_position):
                continue
            if is_impassable_cell(state, next_position):
                continue

            came_from[next_position] = (current, direction)
            if next_position == target:
                return _reconstruct_route(came_from, start=start, target=target)
            frontier.append(next_position)

    raise SrsMovementError(f"no route to known target: {target}")


def apply_srs_command(
    state: SrsGameState,
    command: SrsCommand,
    *,
    contracts: SrsContracts,
    cost_mode: CostMode | None = None,
) -> SrsCommandResult:
    resolved_cost_mode = _normalize_cost_mode(cost_mode, contracts=contracts)
    if command.command_type == "MOVE_ROUTE":
        return _apply_move_route(state, command, contracts=contracts, cost_mode=resolved_cost_mode)
    if command.command_type == "MOVE_TO":
        return _apply_move_to(state, command, contracts=contracts, cost_mode=resolved_cost_mode)
    if command.command_type == "INTERACT":
        return _apply_interact(state, command, contracts=contracts)
    if command.command_type == "COMBAT_STEP":
        return _apply_combat_step(state, command, contracts=contracts)
    if command.command_type == "WAIT":
        return _apply_wait(state)
    if command.command_type == "WARP_EXIT":
        return _apply_warp_exit(state, command)
    return _rejected_movement_result(
        state,
        command_type=command.command_type,
        outcome="REJECTED_UNKNOWN_COMMAND",
        cost_mode=resolved_cost_mode,
    )


def run_srs_commands(
    state: SrsGameState,
    commands: Sequence[SrsCommand],
    *,
    contracts: SrsContracts,
    cost_mode: CostMode | None = None,
) -> SrsCommandResult:
    resolved_cost_mode = _normalize_cost_mode(cost_mode, contracts=contracts)
    current_state = state
    all_events: list[Any] = []
    for command in commands:
        result = apply_srs_command(current_state, command, contracts=contracts, cost_mode=resolved_cost_mode)
        current_state = result.state
        all_events.extend(result.events)
    return SrsCommandResult(state=current_state, events=tuple(all_events))


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


def _apply_move_route(
    state: SrsGameState,
    command: SrsCommand,
    *,
    contracts: SrsContracts,
    cost_mode: CostMode,
) -> SrsCommandResult:
    if not all(direction in contracts.movement.directions for direction in command.route):
        return _rejected_movement_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_UNKNOWN_COMMAND",
            cost_mode=cost_mode,
        )

    entered_cells, blocked_position, movement_raw_cost = resolve_move_route(
        state,
        command.route,
        contracts=contracts,
    )
    if not entered_cells and blocked_position is None:
        return _rejected_movement_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_ZERO_STEP",
            cost_mode=cost_mode,
        )

    next_turn = state.srs_turn + _movement_turn_cost(contracts)
    fuel_before = state.fuel
    fuel_delta = fuel_delta_for_movement_raw_cost(
        movement_raw_cost,
        cost_mode=cost_mode,
        contracts=contracts,
    )
    fuel_after = max(0, fuel_before + fuel_delta)

    if not entered_cells:
        result_state = replace(state, srs_turn=next_turn, fuel=fuel_after)
        event = _movement_event(
            srs_turn=next_turn,
            event_type=STOPPED_BEFORE_IMPASSABLE,
            command_type=command.command_type,
            cost_mode=cost_mode,
            start_position=state.player_position,
            end_position=state.player_position,
            entered_cells=(),
            blocked_position=blocked_position,
            movement_raw_cost=0,
            fuel_delta=fuel_delta,
            fuel_before=fuel_before,
            fuel_after=fuel_after,
            observation_updates=(),
            outcome="STOPPED_BEFORE_IMPASSABLE",
        )
        return SrsCommandResult(state=result_state, events=(event,))

    current_state = replace(
        state,
        player_position=entered_cells[-1],
        srs_turn=next_turn,
        fuel=fuel_after,
    )
    observation_updates: list[Position] = []
    observation_events = []
    for center in entered_cells:
        previous_count = len(current_state.known_state.discovered_cells)
        current_state = reveal_observation(current_state, center=center, contracts=contracts)
        total_count = len(current_state.known_state.discovered_cells)
        observation_updates.append(center)
        observation_events.append(
            make_turn_event(
                srs_turn=next_turn,
                event_type=OBSERVATION_UPDATED,
                payload={
                    "center": _position_to_list(center),
                    "newly_discovered_count": total_count - previous_count,
                    "total_discovered_count": total_count,
                },
            )
        )

    movement_event_type = MOVE_ACCEPTED if blocked_position is None else STOPPED_BEFORE_IMPASSABLE
    outcome = _movement_outcome(
        route=command.route,
        entered_cells=entered_cells,
        blocked_position=blocked_position,
    )
    movement_event = _movement_event(
        srs_turn=next_turn,
        event_type=movement_event_type,
        command_type=command.command_type,
        cost_mode=cost_mode,
        start_position=state.player_position,
        end_position=entered_cells[-1],
        entered_cells=entered_cells,
        blocked_position=blocked_position,
        movement_raw_cost=movement_raw_cost,
        fuel_delta=fuel_delta,
        fuel_before=fuel_before,
        fuel_after=fuel_after,
        observation_updates=tuple(observation_updates),
        outcome=outcome,
    )
    return SrsCommandResult(
        state=current_state,
        events=(movement_event, *observation_events),
    )


def _apply_move_to(
    state: SrsGameState,
    command: SrsCommand,
    *,
    contracts: SrsContracts,
    cost_mode: CostMode,
) -> SrsCommandResult:
    target = command.target
    if target is None:
        raise SrsMovementError("MOVE_TO requires a target")
    if not state.actual_map.contains(target):
        return _rejected_movement_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_OUT_OF_BOUNDS",
            cost_mode=cost_mode,
            target_position=target,
            resolved_route=(),
        )
    if target == state.player_position:
        return _rejected_movement_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_SAME_POSITION",
            cost_mode=cost_mode,
            target_position=target,
            resolved_route=(),
        )
    if target not in state.known_state.discovered_cells:
        return _rejected_movement_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_UNKNOWN_TARGET",
            cost_mode=cost_mode,
            target_position=target,
            resolved_route=(),
        )

    try:
        route = route_to_known_target(state, target)
    except SrsMovementError:
        return _rejected_movement_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_NO_PATH",
            cost_mode=cost_mode,
            target_position=target,
            resolved_route=(),
        )

    result = apply_srs_command(
        state,
        SrsCommand(command_type="MOVE_ROUTE", route=route),
        contracts=contracts,
        cost_mode=cost_mode,
    )
    movement_event = _override_move_to_event(
        result.events[0],
        target_position=target,
        resolved_route=route,
    )
    return SrsCommandResult(
        state=result.state,
        events=(movement_event, *result.events[1:]),
    )


def _rejected_movement_result(
    state: SrsGameState,
    *,
    command_type: str,
    outcome: str,
    cost_mode: CostMode,
    target_position: Position | None = None,
    resolved_route: Sequence[Direction] | None = None,
) -> SrsCommandResult:
    event = _movement_event(
        srs_turn=state.srs_turn,
        event_type=MOVE_REJECTED,
        command_type=command_type,
        cost_mode=cost_mode,
        start_position=state.player_position,
        end_position=state.player_position,
        entered_cells=(),
        blocked_position=None,
        movement_raw_cost=0,
        fuel_delta=0,
        fuel_before=state.fuel,
        fuel_after=state.fuel,
        observation_updates=(),
        outcome=outcome,
        target_position=target_position,
        resolved_route=resolved_route,
    )
    return SrsCommandResult(state=state, events=(event,))


def _apply_interact(
    state: SrsGameState,
    command: SrsCommand,
    *,
    contracts: SrsContracts,
) -> SrsCommandResult:
    target_object_id = command.target_object_id
    if target_object_id is None:
        raise SrsInteractionError("INTERACT requires a target_object_id")

    object_state = state.objects.get(target_object_id)
    if object_state is None:
        return _rejected_interaction_result(
            state,
            object_id=target_object_id,
            outcome="REJECTED_UNKNOWN_OBJECT",
        )

    interaction_contract = _interaction_contract_for_object_type(object_state.object_type, contracts=contracts)
    if interaction_contract is None:
        return _rejected_interaction_result(
            state,
            object_id=target_object_id,
            object_state=object_state,
            outcome="REJECTED_UNSUPPORTED_OBJECT",
        )
    if not _is_valid_interaction_range(state.player_position, object_state.position, interaction_contract["range"]):
        return _rejected_interaction_result(
            state,
            object_id=target_object_id,
            object_state=object_state,
            outcome="REJECTED_WRONG_RANGE",
        )

    if object_state.object_type in {SrsObjectType.RESOURCE_CACHE, SrsObjectType.SALVAGE} and _is_consumed_object(state, object_state):
        return _rejected_interaction_result(
            state,
            object_id=target_object_id,
            object_state=object_state,
            outcome="REJECTED_ALREADY_CONSUMED",
        )

    if object_state.object_type is SrsObjectType.RESOURCE_CACHE:
        return _apply_resource_cache_interaction(state, object_state, interaction_contract)
    if object_state.object_type is SrsObjectType.STATION:
        return _apply_station_interaction(
            state,
            object_state,
            interaction_contract,
            base_upgrade_choice=command.base_upgrade_choice,
        )
    if object_state.object_type is SrsObjectType.SALVAGE:
        return _apply_salvage_interaction(
            state,
            object_state,
            interaction_contract,
            salvage_choice=command.salvage_choice,
        )

    return _rejected_interaction_result(
        state,
        object_id=target_object_id,
        object_state=object_state,
        outcome="REJECTED_UNSUPPORTED_OBJECT",
    )


def _apply_wait(
    state: SrsGameState,
) -> SrsCommandResult:
    next_turn = state.srs_turn + 1
    return SrsCommandResult(
        state=replace(state, srs_turn=next_turn),
        events=(
            make_turn_event(
                srs_turn=next_turn,
                event_type=WAIT_ACCEPTED,
                payload={
                    "command_type": "WAIT",
                    "start_position": _position_to_list(state.player_position),
                    "end_position": _position_to_list(state.player_position),
                    "fuel_delta": 0,
                    "fuel_before": state.fuel,
                    "fuel_after": state.fuel,
                    "outcome": "ACCEPTED",
                },
            ),
        ),
    )


def _apply_warp_exit(
    state: SrsGameState,
    command: SrsCommand,
) -> SrsCommandResult:
    exit_direction = command.exit_direction
    if exit_direction is None:
        raise SrsMovementError("WARP_EXIT requires an exit_direction")

    start_position = state.player_position
    if state.combat_state is not None and state.combat_state.enemy_presence:
        return SrsCommandResult(
            state=state,
            events=(
                _warp_exit_event(
                    srs_turn=state.srs_turn,
                    event_type=WARP_EXIT_REJECTED,
                    state=state,
                    exit_direction=exit_direction,
                    start_position=start_position,
                    outcome="REJECTED_ENEMY_PRESENCE",
                ),
            ),
        )

    if not state.actual_map.contains(start_position):
        return SrsCommandResult(
            state=state,
            events=(
                _warp_exit_event(
                    srs_turn=state.srs_turn,
                    event_type=WARP_EXIT_REJECTED,
                    state=state,
                    exit_direction=exit_direction,
                    start_position=start_position,
                    outcome="REJECTED_OUT_OF_BOUNDS",
                ),
            ),
        )

    if exit_direction in state.descriptor.blocked_edges:
        return SrsCommandResult(
            state=state,
            events=(
                _warp_exit_event(
                    srs_turn=state.srs_turn,
                    event_type=WARP_EXIT_REJECTED,
                    state=state,
                    exit_direction=exit_direction,
                    start_position=start_position,
                    outcome="REJECTED_BLOCKED_EDGE",
                ),
            ),
        )

    current_cell = state.actual_map.cell_at(start_position)
    if exit_direction not in current_cell.warp_flags:
        return SrsCommandResult(
            state=state,
            events=(
                _warp_exit_event(
                    srs_turn=state.srs_turn,
                    event_type=WARP_EXIT_REJECTED,
                    state=state,
                    exit_direction=exit_direction,
                    start_position=start_position,
                    outcome="REJECTED_NO_WARP_FLAG",
                ),
            ),
        )

    next_turn = state.srs_turn + 1
    return SrsCommandResult(
        state=replace(state, srs_turn=next_turn),
        events=(
            _warp_exit_event(
                srs_turn=next_turn,
                event_type=WARP_EXIT_ACCEPTED,
                state=state,
                exit_direction=exit_direction,
                start_position=start_position,
                outcome="ACCEPTED",
            ),
        ),
    )


def _apply_combat_step(
    state: SrsGameState,
    command: SrsCommand,
    *,
    contracts: SrsContracts,
) -> SrsCommandResult:
    combat_state = state.combat_state
    if combat_state is None:
        return SrsCommandResult(
            state=state,
            events=(
                make_turn_event(
                    srs_turn=state.srs_turn,
                    event_type=COMBAT_REJECTED,
                    payload={
                        "command_type": command.command_type,
                        "outcome": "REJECTED_NO_COMBAT_STATE",
                    },
                ),
            ),
        )

    player_before = combat_state.player
    phase_from = combat_state.phase
    target_attackable = _player_target_is_attackable(state)
    phase_to = _next_combat_phase(state, target_attackable=target_attackable)
    player_after = player_before
    player_action: Mapping[str, Any] | None = None
    combat_turn_after = combat_state.combat_turn
    enemy_actions: tuple[Mapping[str, Any], ...] = ()
    enemy_states = combat_state.enemies
    player_attack_target_id = combat_state.player_attack_target_id

    try:
        if phase_from is SrsCombatPhase.PLAYER_ATTACK:
            state, enemy_states, player_after, player_action = _resolve_player_attack_phase(state, command)
            if player_attack_target_id not in enemy_states:
                player_attack_target_id = None
        elif phase_from is SrsCombatPhase.ENEMY_ACTION:
            state, enemy_states, player_after, enemy_actions = _resolve_enemy_action_phase(
                state,
                command,
                contracts=contracts,
            )
            combat_turn_after += 1
            player_after = _recover_player_energy(player_after)
    except SrsCombatError as exc:
        return SrsCommandResult(
            state=state,
            events=(
                make_turn_event(
                    srs_turn=state.srs_turn,
                    event_type=COMBAT_REJECTED,
                    payload={
                        "command_type": command.command_type,
                        "phase": phase_from.value,
                        "outcome": str(exc),
                    },
                ),
            ),
        )

    updated_state = replace(
        state,
        player_state=player_after,
        combat_state=replace(
            combat_state,
            enemies=enemy_states,
            player=player_after,
            phase=phase_to,
            combat_turn=combat_turn_after,
            player_attack_target_id=player_attack_target_id,
        ),
    )
    return SrsCommandResult(
        state=updated_state,
        events=(
            make_turn_event(
                srs_turn=state.srs_turn,
                event_type=COMBAT_TRANSITIONED,
                payload={
                    "command_type": command.command_type,
                    "phase_from": phase_from.value,
                    "phase_to": phase_to.value,
                    "combat_turn_before": combat_state.combat_turn,
                    "combat_turn_after": combat_turn_after,
                    "enemy_presence": combat_state.enemy_presence,
                    "target_available": combat_state.target_available,
                    "target_attackable": target_attackable,
                    "player_action": player_action,
                    "enemy_actions": enemy_actions,
                    "player_durability_before": player_before.durability,
                    "player_durability_after": player_after.durability,
                    "player_energy_before": player_before.energy,
                    "player_energy_after": player_after.energy,
                    "player_torpedo_ammo_before": player_before.photon_torpedo_ammo,
                    "player_torpedo_ammo_after": player_after.photon_torpedo_ammo,
                    "outcome": "ACCEPTED",
                },
            ),
        ),
    )


def _resolve_player_attack_phase(
    state: SrsGameState,
    command: SrsCommand,
) -> tuple[SrsGameState, Mapping[str, SrsEnemyCombatState], SrsPlayerCombatState, Mapping[str, Any]]:
    combat_state = state.combat_state
    if combat_state is None:
        raise SrsCombatError("combat_state is required for player attack resolution")

    action = command.player_attack_action or SrsPlayerAttackAction.SKIP
    payload: dict[str, Any] = {
        "selected_action": action.value,
        "selected_weapon": None if command.player_attack_weapon is None else command.player_attack_weapon.value,
        "target_enemy_id": combat_state.player_attack_target_id,
        "attack_executed": False,
        "damage_applied": 0,
        "resource_cost": 0,
        "resource_type": None,
        "target_destroyed": False,
    }
    if action is SrsPlayerAttackAction.SKIP:
        return state, combat_state.enemies, combat_state.player, payload
    if not combat_state.target_available:
        raise SrsCombatError("REJECTED_TARGET_UNAVAILABLE")
    if command.player_attack_weapon is None:
        raise SrsCombatError("REJECTED_ATTACK_WEAPON_REQUIRED")

    weapon_profile = combat_state.weapon_profiles.get(command.player_attack_weapon)
    if weapon_profile is None or weapon_profile.damage is None:
        raise SrsCombatError("REJECTED_INVALID_ATTACK_WEAPON")

    target_enemy_id = combat_state.player_attack_target_id
    target_enemy = combat_state.enemies[target_enemy_id]
    if not is_attackable_position(
        state,
        attacker=state.player_position,
        target=target_enemy.position,
        weapon_type=command.player_attack_weapon,
    ):
        raise SrsCombatError("REJECTED_TARGET_NOT_ATTACKABLE")

    player = combat_state.player
    if weapon_profile.ammo_cost > 0:
        if player.photon_torpedo_ammo < weapon_profile.ammo_cost:
            raise SrsCombatError("REJECTED_INSUFFICIENT_TORPEDO_AMMO")
        player = replace(
            player,
            photon_torpedo_ammo=player.photon_torpedo_ammo - weapon_profile.ammo_cost,
        )
        payload["resource_cost"] = weapon_profile.ammo_cost
        payload["resource_type"] = "PHOTON_TORPEDO_AMMO"
    if weapon_profile.energy_cost > 0:
        if player.energy < weapon_profile.energy_cost:
            raise SrsCombatError("REJECTED_INSUFFICIENT_PHASER_ENERGY")
        player = replace(
            player,
            energy=player.energy - weapon_profile.energy_cost,
        )
        payload["resource_cost"] = weapon_profile.energy_cost
        payload["resource_type"] = "ENERGY"

    updated_enemies = dict(combat_state.enemies)
    remaining_durability = target_enemy.durability - weapon_profile.damage
    payload["attack_executed"] = True
    payload["damage_applied"] = weapon_profile.damage
    if remaining_durability <= 0:
        del updated_enemies[target_enemy_id]
        payload["target_destroyed"] = True
        if target_enemy.drop_salvage:
            state, salvage_drop = _spawn_enemy_drop_salvage_object(
                state,
                enemy=target_enemy,
            )
            payload["salvage_drop"] = salvage_drop
    else:
        updated_enemies[target_enemy_id] = replace(target_enemy, durability=remaining_durability)
        payload["target_remaining_durability"] = remaining_durability
    return state, updated_enemies, player, payload


def _player_target_is_attackable(state: SrsGameState) -> bool:
    combat_state = state.combat_state
    if combat_state is None or not combat_state.target_available:
        return False

    enemy = combat_state.enemies[combat_state.player_attack_target_id]
    return any(
        is_attackable_position(
            state,
            attacker=state.player_position,
            target=enemy.position,
            weapon_type=weapon_type,
        )
        for weapon_type in _PLAYER_ATTACK_WEAPONS
    )


def _next_combat_phase(
    state: SrsGameState,
    *,
    target_attackable: bool,
) -> SrsCombatPhase:
    combat_state = state.combat_state
    if combat_state is None:
        raise SrsCombatError("combat_state is required for combat phase transitions")
    if combat_state.phase is SrsCombatPhase.PLAYER_MOVEMENT:
        if combat_state.enemy_presence and target_attackable:
            return SrsCombatPhase.PLAYER_ATTACK
        return SrsCombatPhase.ENEMY_ACTION
    if combat_state.phase is SrsCombatPhase.PLAYER_ATTACK:
        return SrsCombatPhase.ENEMY_ACTION
    return SrsCombatPhase.PLAYER_MOVEMENT


def _recover_player_energy(player: SrsPlayerCombatState) -> SrsPlayerCombatState:
    return replace(
        player,
        energy=min(player.energy_capacity, player.energy + player.energy_recovery),
    )


def _resolve_enemy_attack_reaction(
    state: SrsGameState,
    *,
    command: SrsCommand,
    enemy: SrsEnemyCombatState,
) -> tuple[SrsPlayerCombatState, SrsEnemyCombatState | None, Mapping[str, Any]]:
    combat_state = state.combat_state
    if combat_state is None:
        raise SrsCombatError("combat_state is required for enemy reaction resolution")

    selected_reaction = command.enemy_reactions.get(enemy.enemy_id, SrsEnemyReaction.DEFEND)
    counterattack_available = _counterattack_available(state, enemy=enemy)
    resolved_reaction = selected_reaction
    if selected_reaction is SrsEnemyReaction.COUNTERATTACK and not counterattack_available:
        resolved_reaction = SrsEnemyReaction.DEFEND

    damage_to_player = enemy.attack_damage
    player = combat_state.player
    updated_enemy: SrsEnemyCombatState | None = enemy
    counterattack_damage = 0
    if resolved_reaction is SrsEnemyReaction.DEFEND:
        damage_to_player = ceil(enemy.attack_damage * 0.5)
    else:
        phaser_profile = combat_state.weapon_profiles[SrsWeaponType.PHASER]
        player = replace(
            player,
            energy=player.energy - phaser_profile.energy_cost,
        )
        counterattack_damage = phaser_profile.damage or 0
        remaining_enemy_durability = enemy.durability - counterattack_damage
        if remaining_enemy_durability <= 0:
            updated_enemy = None
        else:
            updated_enemy = replace(enemy, durability=remaining_enemy_durability)

    player = replace(
        player,
        durability=max(0, player.durability - damage_to_player),
    )
    reaction_payload: dict[str, Any] = {
        "selected_reaction": selected_reaction.value,
        "resolved_reaction": resolved_reaction.value,
        "counterattack_available": counterattack_available,
        "fallback_to_defend": (
            selected_reaction is SrsEnemyReaction.COUNTERATTACK
            and resolved_reaction is SrsEnemyReaction.DEFEND
        ),
        "damage_to_player": damage_to_player,
        "counterattack_damage": counterattack_damage,
        "enemy_destroyed": updated_enemy is None,
    }
    return player, updated_enemy, reaction_payload


def _counterattack_available(
    state: SrsGameState,
    *,
    enemy: SrsEnemyCombatState,
) -> bool:
    combat_state = state.combat_state
    if combat_state is None:
        raise SrsCombatError("combat_state is required for counterattack checks")
    phaser_profile = combat_state.weapon_profiles.get(SrsWeaponType.PHASER)
    if phaser_profile is None:
        raise SrsCombatError("missing weapon profile: PHASER")
    if combat_state.player.energy < phaser_profile.energy_cost:
        return False
    return is_attackable_position(
        state,
        attacker=state.player_position,
        target=enemy.position,
        weapon_type=SrsWeaponType.PHASER,
    )


def _resolve_enemy_action_phase(
    state: SrsGameState,
    command: SrsCommand,
    *,
    contracts: SrsContracts,
) -> tuple[SrsGameState, Mapping[str, Any], SrsPlayerCombatState, tuple[Mapping[str, Any], ...]]:
    combat_state = state.combat_state
    if combat_state is None:
        raise SrsCombatError("combat_state is required for enemy action resolution")

    updated_enemies = dict(combat_state.enemies)
    player = combat_state.player
    actions = []
    attackable_positions = enemy_attackable_positions(state)
    updated_state = state
    for enemy_id in tuple(updated_enemies):
        enemy = updated_enemies.get(enemy_id)
        if enemy is None:
            continue
        working_state = replace(
            updated_state,
            combat_state=replace(
                combat_state,
                enemies=updated_enemies,
                player=player,
            ),
        )
        updated_state, updated_enemy, player, action = _resolve_enemy_action(
            working_state,
            command=command,
            enemy=enemy,
            attackable_positions=attackable_positions,
            contracts=contracts,
        )
        if updated_enemy is None:
            del updated_enemies[enemy_id]
        else:
            updated_enemies[enemy_id] = updated_enemy
        actions.append(action)

    return updated_state, updated_enemies, player, tuple(actions)


def _resolve_enemy_action(
    state: SrsGameState,
    *,
    command: SrsCommand,
    enemy: SrsEnemyCombatState,
    attackable_positions: Sequence[Position],
    contracts: SrsContracts,
) -> tuple[SrsGameState, SrsEnemyCombatState | None, SrsPlayerCombatState, Mapping[str, Any]]:
    combat_state = state.combat_state
    if combat_state is None:
        raise SrsCombatError("combat_state is required for enemy action resolution")

    can_attack_before_move = is_attackable_position(
        state,
        attacker=enemy.position,
        target=state.player_position,
        weapon_type=SrsWeaponType.ENEMY_WEAPON,
    )
    if can_attack_before_move:
        player_after, enemy_after, reaction = _resolve_enemy_attack_reaction(
            state,
            command=command,
            enemy=enemy,
        )
        if enemy_after is None and enemy.drop_salvage:
            state, salvage_drop = _spawn_enemy_drop_salvage_object(state, enemy=enemy)
            reaction = dict(reaction)
            reaction["salvage_drop"] = salvage_drop
        return state, enemy_after, player_after, {
            "enemy_id": enemy.enemy_id,
            "start_position": _position_to_list(enemy.position),
            "target_attackable_position": _position_to_list(enemy.position),
            "planned_path": [],
            "moved_path": [],
            "final_position": _position_to_list(enemy.position),
            "movement_power": enemy.movement_power,
            "movement_cost": 0,
            "attacked_player": True,
            "can_attack_before_move": True,
            "can_attack_after_move": True,
            "reaction": reaction,
        }

    planned_target, planned_path, movement_cost = _select_enemy_path_to_attack_position(
        state,
        start=enemy.position,
        attackable_positions=attackable_positions,
        contracts=contracts,
    )
    moved_path = planned_path[: enemy.movement_power]
    final_position = enemy.position if not moved_path else moved_path[-1]
    updated_enemy = replace(enemy, position=final_position)
    can_attack_after_move = is_attackable_position(
        state,
        attacker=final_position,
        target=state.player_position,
        weapon_type=SrsWeaponType.ENEMY_WEAPON,
    )
    return state, updated_enemy, combat_state.player, {
        "enemy_id": enemy.enemy_id,
        "start_position": _position_to_list(enemy.position),
        "target_attackable_position": None if planned_target is None else _position_to_list(planned_target),
        "planned_path": [_position_to_list(position) for position in planned_path],
        "moved_path": [_position_to_list(position) for position in moved_path],
        "final_position": _position_to_list(final_position),
        "movement_power": enemy.movement_power,
        "movement_cost": movement_cost,
        "attacked_player": False,
        "can_attack_before_move": False,
        "can_attack_after_move": can_attack_after_move,
        "reaction": None,
    }


def _select_enemy_path_to_attack_position(
    state: SrsGameState,
    *,
    start: Position,
    attackable_positions: Sequence[Position],
    contracts: SrsContracts,
) -> tuple[Position | None, tuple[Position, ...], int]:
    if not state.actual_map.contains(start):
        raise SrsCombatError(f"enemy position out of bounds: {start}")

    best_paths = _dijkstra_enemy_paths(state, start=start, contracts=contracts)
    best_target: Position | None = None
    best_path: tuple[Position, ...] = ()
    best_cost: int | None = None
    for target in attackable_positions:
        if target == start:
            continue
        candidate = best_paths.get(target)
        if candidate is None:
            continue
        candidate_cost, candidate_path = candidate
        target_key = _position_sort_key(target)
        best_target_key = None if best_target is None else _position_sort_key(best_target)
        if best_cost is None or (candidate_cost, target_key) < (best_cost, best_target_key):
            best_target = target
            best_path = candidate_path
            best_cost = candidate_cost

    if best_target is None or best_cost is None:
        return None, (), 0
    return best_target, best_path, best_cost


def _dijkstra_enemy_paths(
    state: SrsGameState,
    *,
    start: Position,
    contracts: SrsContracts,
) -> Mapping[Position, tuple[int, tuple[Position, ...]]]:
    best_paths: dict[Position, tuple[int, tuple[tuple[int, int], ...], tuple[Position, ...]]] = {
        start: (0, (), ())
    }
    frontier: list[tuple[int, tuple[tuple[int, int], ...], Position]] = [(0, (), start)]

    while frontier:
        current_cost, current_path_key, current_position = heappop(frontier)
        recorded_cost, recorded_path_key, recorded_path = best_paths[current_position]
        if (current_cost, current_path_key) != (recorded_cost, recorded_path_key):
            continue

        for direction in _ENEMY_PATH_TIEBREAK_DIRECTIONS:
            next_position = _step_position(current_position, direction)
            if next_position == state.player_position:
                continue
            if not state.actual_map.contains(next_position):
                continue
            if is_impassable_cell(state, next_position):
                continue

            step_cost = _orthogonal_step_raw_cost(
                state.actual_map.cell_at(next_position),
                contracts=contracts,
            )
            next_path = recorded_path + (next_position,)
            next_path_key = current_path_key + (_position_sort_key(next_position),)
            candidate = (current_cost + step_cost, next_path_key, next_path)
            existing = best_paths.get(next_position)
            if existing is not None and candidate[:2] >= existing[:2]:
                continue
            best_paths[next_position] = candidate
            heappush(frontier, (candidate[0], candidate[1], next_position))

    return {
        position: (cost, path)
        for position, (cost, _path_key, path) in best_paths.items()
    }


def _apply_resource_cache_interaction(
    state: SrsGameState,
    object_state: SrsObjectState,
    interaction_contract: Mapping[str, Any],
) -> SrsCommandResult:
    fuel_before = state.fuel
    fuel_after = min(state.max_fuel, fuel_before + _RESOURCE_CACHE_FUEL_RESTORE)
    fuel_delta = fuel_after - fuel_before
    if fuel_delta == 0:
        return _rejected_interaction_result(
            state,
            object_id=object_state.object_id,
            object_state=object_state,
            outcome="REJECTED_NO_EFFECT",
        )

    next_turn = state.srs_turn + 1
    updated_state = _accepted_interaction_state(
        state,
        fuel_after=fuel_after,
        next_turn=next_turn,
        object_state=replace(object_state, consumed=True),
        consumed_object_id=object_state.object_id,
    )
    events = (
        _interaction_event(
            srs_turn=next_turn,
            event_type=INTERACT_ACCEPTED,
            object_state=object_state,
            interaction_contract=interaction_contract,
            fuel_before=fuel_before,
            fuel_after=fuel_after,
            fuel_delta=fuel_delta,
            outcome="ACCEPTED",
        ),
        make_turn_event(
            srs_turn=next_turn,
            event_type=OBJECT_CONSUMED,
            payload={
                "object_id": object_state.object_id,
                "object_type": object_state.object_type.value,
                "fuel_restore": _RESOURCE_CACHE_FUEL_RESTORE,
                "fuel_before": fuel_before,
                "fuel_after": fuel_after,
                "fuel_delta": fuel_delta,
                "consumed": True,
            },
        ),
    )
    return SrsCommandResult(state=updated_state, events=events)


def _apply_station_interaction(
    state: SrsGameState,
    object_state: SrsObjectState,
    interaction_contract: Mapping[str, Any],
    *,
    base_upgrade_choice: SrsBaseUpgrade | None,
) -> SrsCommandResult:
    player_before = _current_player_state(state)
    fuel_before = state.fuel
    fuel_after = state.max_fuel
    fuel_delta = fuel_after - fuel_before
    available_upgrades = [
        upgrade.value
        for upgrade, cost in _BASE_UPGRADE_COSTS.items()
        if player_before.salvage >= cost
    ]
    player_after_recovery = replace(
        player_before,
        durability=player_before.durability_capacity,
        energy=player_before.energy_capacity,
        photon_torpedo_ammo=player_before.photon_torpedo_ammo_capacity,
    )
    player_after_upgrade = player_after_recovery
    applied_upgrade = None
    salvage_spent = 0
    if base_upgrade_choice is not None:
        try:
            player_after_upgrade, salvage_spent, applied_upgrade = _apply_base_upgrade_choice(
                player_after_recovery,
                upgrade=base_upgrade_choice,
            )
        except SrsInteractionError:
            return _rejected_interaction_result(
                state,
                object_id=object_state.object_id,
                object_state=object_state,
                outcome="REJECTED_UPGRADE_UNAVAILABLE",
            )

    next_turn = state.srs_turn + 1
    updated_state = _accepted_interaction_state(
        state,
        fuel_after=fuel_after,
        next_turn=next_turn,
        object_state=replace(object_state, activated=True),
        activated_object_id=object_state.object_id,
    )
    updated_state = _replace_player_state(updated_state, player_after_upgrade)
    events = (
        _interaction_event(
            srs_turn=next_turn,
            event_type=INTERACT_ACCEPTED,
            object_state=object_state,
            interaction_contract=interaction_contract,
            fuel_before=fuel_before,
            fuel_after=fuel_after,
            fuel_delta=fuel_delta,
            outcome="ACCEPTED",
            player_before=player_before,
            player_after=player_after_upgrade,
            extra_payload={
                "available_upgrades": available_upgrades,
                "selected_upgrade": None if base_upgrade_choice is None else base_upgrade_choice.value,
                "applied_upgrade": applied_upgrade,
                "salvage_spent": salvage_spent,
            },
        ),
        make_turn_event(
            srs_turn=next_turn,
            event_type=STATION_ACTIVATED,
            payload={
                "object_id": object_state.object_id,
                "object_type": object_state.object_type.value,
                "fuel_before": fuel_before,
                "fuel_after": fuel_after,
                "fuel_delta": fuel_delta,
                "activated": True,
                "reusable": True,
                "player_durability_before": player_before.durability,
                "player_durability_after": player_after_upgrade.durability,
                "player_energy_before": player_before.energy,
                "player_energy_after": player_after_upgrade.energy,
                "player_torpedo_ammo_before": player_before.photon_torpedo_ammo,
                "player_torpedo_ammo_after": player_after_upgrade.photon_torpedo_ammo,
                "salvage_before": player_before.salvage,
                "salvage_after": player_after_upgrade.salvage,
                "available_upgrades": available_upgrades,
                "selected_upgrade": None if base_upgrade_choice is None else base_upgrade_choice.value,
                "applied_upgrade": applied_upgrade,
                "salvage_spent": salvage_spent,
            },
        ),
    )
    return SrsCommandResult(state=updated_state, events=events)


def _spawn_enemy_drop_salvage_object(
    state: SrsGameState,
    *,
    enemy: SrsEnemyCombatState,
) -> tuple[SrsGameState, Mapping[str, Any]]:
    cell = state.actual_map.cell_at(enemy.position)
    payload = {
        "reward_source": _ENEMY_DROP_REWARD_SOURCE,
        "object_id": _enemy_drop_object_id(enemy.enemy_id),
        "position": _position_to_list(enemy.position),
        "enemy_id": enemy.enemy_id,
        "enemy_tier": enemy.tier.value,
        "salvage_value": _SALVAGE_DROP_VALUES[enemy.tier],
    }
    if cell.object_id is not None:
        return state, payload | {
            "spawned": False,
            "skip_reason": _ENEMY_DROP_SKIP_OCCUPIED_CELL,
        }

    object_id = _enemy_drop_object_id(enemy.enemy_id)
    if object_id in state.objects:
        return state, payload | {
            "spawned": False,
            "skip_reason": _ENEMY_DROP_SKIP_OBJECT_ID_COLLISION,
        }
    object_state = SrsObjectState(
        object_id=object_id,
        object_type=SrsObjectType.SALVAGE,
        position=enemy.position,
        metadata={
            "reward_source": _ENEMY_DROP_REWARD_SOURCE,
            "dropped_by_enemy_id": enemy.enemy_id,
            "dropped_by_enemy_tier": enemy.tier.value,
            "salvage_value": _SALVAGE_DROP_VALUES[enemy.tier],
        },
    )
    return _state_with_added_object(state, object_state=object_state), payload | {"spawned": True}


def _enemy_drop_object_id(enemy_id: str) -> str:
    return f"enemy-drop-salvage-{enemy_id}"


def _state_with_added_object(
    state: SrsGameState,
    *,
    object_state: SrsObjectState,
) -> SrsGameState:
    rows = [list(row) for row in state.actual_map.cells]
    current_cell = state.actual_map.cell_at(object_state.position)
    rows[object_state.position.y][object_state.position.x] = SrsCell(
        terrain=current_cell.terrain,
        object_id=object_state.object_id,
        actor_id=current_cell.actor_id,
        warp_flags=current_cell.warp_flags,
    )
    actual_map = SrsActualMap(
        width=state.actual_map.width,
        height=state.actual_map.height,
        cells=tuple(tuple(row) for row in rows),
    )
    known_cells = dict(state.known_state.known_cells)
    if object_state.position in known_cells:
        known_cells[object_state.position] = actual_map.cell_at(object_state.position)
    return replace(
        state,
        actual_map=actual_map,
        known_state=replace(state.known_state, known_cells=known_cells),
        objects={
            **state.objects,
            object_state.object_id: object_state,
        },
    )


def _salvage_reward_profile(
    object_state: SrsObjectState,
) -> tuple[str, int, Mapping[SrsSalvageChoice, int]]:
    reward_source = object_state.metadata.get("reward_source")
    if reward_source == _ENEMY_DROP_REWARD_SOURCE:
        enemy_tier = SrsEnemyTier(object_state.metadata["dropped_by_enemy_tier"])
        salvage_value = int(object_state.metadata["salvage_value"])
        return reward_source, salvage_value, _SALVAGE_DROP_RECOVERY[enemy_tier]
    return _MAP_PICKUP_REWARD_SOURCE, _SALVAGE_BASE_VALUE, _SALVAGE_BASE_RECOVERY


def _apply_salvage_interaction(
    state: SrsGameState,
    object_state: SrsObjectState,
    interaction_contract: Mapping[str, Any],
    *,
    salvage_choice: SrsSalvageChoice | None,
) -> SrsCommandResult:
    player_before = _current_player_state(state)
    try:
        reward_source, salvage_value, recovery_amounts = _salvage_reward_profile(object_state)
        resolved_choice = _resolve_salvage_choice(
            salvage_choice,
            error_type=SrsInteractionError,
        )
        player_after, salvage_reward = _apply_salvage_reward_to_player(
            player_before,
            salvage_value=salvage_value,
            resolved_choice=resolved_choice,
            recovery_amounts=recovery_amounts,
            source=reward_source,
        )
    except SrsInteractionError as exc:
        return _rejected_interaction_result(
            state,
            object_id=object_state.object_id,
            object_state=object_state,
            outcome=str(exc),
        )
    next_turn = state.srs_turn + 1
    updated_state = _accepted_interaction_state(
        state,
        fuel_after=state.fuel,
        next_turn=next_turn,
        object_state=replace(object_state, consumed=True),
        consumed_object_id=object_state.object_id,
    )
    updated_state = _replace_player_state(updated_state, player_after)
    events = (
        _interaction_event(
            srs_turn=next_turn,
            event_type=INTERACT_ACCEPTED,
            object_state=object_state,
            interaction_contract=interaction_contract,
            fuel_before=state.fuel,
            fuel_after=state.fuel,
            fuel_delta=0,
            outcome="ACCEPTED",
            player_before=player_before,
            player_after=player_after,
            extra_payload=salvage_reward,
        ),
        make_turn_event(
            srs_turn=next_turn,
            event_type=OBJECT_CONSUMED,
            payload={
                "object_id": object_state.object_id,
                "object_type": object_state.object_type.value,
                "consumed": True,
                "outcome": "ACCEPTED",
                **salvage_reward,
            },
        ),
    )
    return SrsCommandResult(state=updated_state, events=events)


def _apply_salvage_reward_to_player(
    player: SrsPlayerCombatState,
    *,
    salvage_value: int,
    resolved_choice: SrsSalvageChoice,
    recovery_amounts: Mapping[SrsSalvageChoice, int],
    source: str,
) -> tuple[SrsPlayerCombatState, Mapping[str, Any]]:
    recovery_amount = recovery_amounts[resolved_choice]
    durability_before = player.durability
    energy_before = player.energy
    ammo_before = player.photon_torpedo_ammo

    updated_player = player
    if resolved_choice is SrsSalvageChoice.RECOVER_ENERGY:
        updated_player = replace(
            updated_player,
            energy=min(updated_player.energy_capacity, updated_player.energy + recovery_amount),
        )
    elif resolved_choice is SrsSalvageChoice.RECOVER_PHOTON_TORPEDO_AMMO:
        updated_player = replace(
            updated_player,
            photon_torpedo_ammo=min(
                updated_player.photon_torpedo_ammo_capacity,
                updated_player.photon_torpedo_ammo + recovery_amount,
            ),
        )

    updated_player = replace(updated_player, salvage=updated_player.salvage + salvage_value)
    return updated_player, {
        "reward_source": source,
        "salvage_value": salvage_value,
        "salvage_before": player.salvage,
        "salvage_after": updated_player.salvage,
        "selected_salvage_choice": resolved_choice.value,
        "durability_before": durability_before,
        "durability_after": updated_player.durability,
        "durability_delta": updated_player.durability - durability_before,
        "energy_before": energy_before,
        "energy_after": updated_player.energy,
        "energy_delta": updated_player.energy - energy_before,
        "photon_torpedo_ammo_before": ammo_before,
        "photon_torpedo_ammo_after": updated_player.photon_torpedo_ammo,
        "photon_torpedo_ammo_delta": updated_player.photon_torpedo_ammo - ammo_before,
    }


def _resolve_salvage_choice(
    choice: SrsSalvageChoice | None,
    *,
    error_type: type[SrsInteractionError] | type[SrsCombatError],
) -> SrsSalvageChoice:
    if choice is None:
        return SrsSalvageChoice.STORE_ONLY
    if choice in _SUPPORTED_SALVAGE_RECOVERY_CHOICES:
        return choice
    raise error_type("REJECTED_UNSUPPORTED_SALVAGE_CHOICE")


def _apply_base_upgrade_choice(
    player: SrsPlayerCombatState,
    *,
    upgrade: SrsBaseUpgrade,
) -> tuple[SrsPlayerCombatState, int, str]:
    cost = _BASE_UPGRADE_COSTS[upgrade]
    if player.salvage < cost:
        raise SrsInteractionError(f"insufficient salvage for base upgrade: {upgrade.value}")

    updated_player = replace(player, salvage=player.salvage - cost)
    if upgrade is SrsBaseUpgrade.PHASER_POWER:
        updated_player = replace(updated_player, phaser_power=updated_player.phaser_power + 1)
    elif upgrade is SrsBaseUpgrade.PHOTON_TORPEDO_POWER:
        updated_player = replace(updated_player, photon_torpedo_power=updated_player.photon_torpedo_power + 1)
    elif upgrade is SrsBaseUpgrade.ENERGY_CAPACITY:
        updated_player = replace(
            updated_player,
            energy_capacity=updated_player.energy_capacity + 1,
            energy=updated_player.energy + 1,
        )
    elif upgrade is SrsBaseUpgrade.PHOTON_TORPEDO_AMMO_CAPACITY:
        updated_player = replace(
            updated_player,
            photon_torpedo_ammo_capacity=updated_player.photon_torpedo_ammo_capacity + 1,
            photon_torpedo_ammo=updated_player.photon_torpedo_ammo + 1,
        )
    elif upgrade is SrsBaseUpgrade.DEFENSE:
        updated_player = replace(updated_player, defense=updated_player.defense + 1)
    elif upgrade is SrsBaseUpgrade.EVASION:
        updated_player = replace(updated_player, evasion=updated_player.evasion + 1)

    return updated_player, cost, upgrade.value


def _accepted_interaction_state(
    state: SrsGameState,
    *,
    fuel_after: int,
    next_turn: int,
    object_state: SrsObjectState,
    consumed_object_id: str | None = None,
    activated_object_id: str | None = None,
) -> SrsGameState:
    objects = dict(state.objects)
    objects[object_state.object_id] = object_state
    consumed_object_ids = set(state.persistent_state.consumed_object_ids)
    activated_object_ids = set(state.persistent_state.activated_object_ids)
    if consumed_object_id is not None:
        consumed_object_ids.add(consumed_object_id)
    if activated_object_id is not None:
        activated_object_ids.add(activated_object_id)
    persistent_state = replace(
        state.persistent_state,
        consumed_object_ids=frozenset(consumed_object_ids),
        activated_object_ids=frozenset(activated_object_ids),
    )
    return replace(
        state,
        objects=objects,
        persistent_state=persistent_state,
        srs_turn=next_turn,
        fuel=fuel_after,
    )


def _rejected_interaction_result(
    state: SrsGameState,
    *,
    object_id: str,
    outcome: str,
    object_state: SrsObjectState | None = None,
) -> SrsCommandResult:
    event = _interaction_event(
        srs_turn=state.srs_turn,
        event_type=INTERACT_REJECTED,
        object_state=object_state,
        interaction_contract=None,
        fuel_before=state.fuel,
        fuel_after=state.fuel,
        fuel_delta=0,
        outcome=outcome,
        object_id=object_id,
    )
    return SrsCommandResult(state=state, events=(event,))


def _interaction_event(
    *,
    srs_turn: int,
    event_type: str,
    object_state: SrsObjectState | None,
    interaction_contract: Mapping[str, Any] | None,
    fuel_before: int,
    fuel_after: int,
    fuel_delta: int,
    outcome: str,
    object_id: str | None = None,
    player_before: SrsPlayerCombatState | None = None,
    player_after: SrsPlayerCombatState | None = None,
    extra_payload: Mapping[str, Any] | None = None,
) -> Any:
    resolved_object_id = object_id if object_id is not None else object_state.object_id
    payload = {
        "command_type": "INTERACT",
        "object_id": resolved_object_id,
        "object_type": None if object_state is None else object_state.object_type.value,
        "interaction_range": None if interaction_contract is None else interaction_contract["range"],
        "effect": None if interaction_contract is None else interaction_contract["effect"],
        "position": None if object_state is None else _position_to_list(object_state.position),
        "fuel_before": fuel_before,
        "fuel_after": fuel_after,
        "fuel_delta": fuel_delta,
        "outcome": outcome,
    }
    if player_before is not None and player_after is not None:
        payload |= {
            "player_durability_before": player_before.durability,
            "player_durability_after": player_after.durability,
            "player_energy_before": player_before.energy,
            "player_energy_after": player_after.energy,
            "player_torpedo_ammo_before": player_before.photon_torpedo_ammo,
            "player_torpedo_ammo_after": player_after.photon_torpedo_ammo,
            "salvage_before": player_before.salvage,
            "salvage_after": player_after.salvage,
        }
    if extra_payload is not None:
        payload |= dict(extra_payload)
    return make_turn_event(
        srs_turn=srs_turn,
        event_type=event_type,
        payload=payload,
    )


def _interaction_contract_for_object_type(
    object_type: SrsObjectType,
    *,
    contracts: SrsContracts,
) -> Mapping[str, Any] | None:
    contract = contracts.movement.interaction.get(object_type.value)
    if contract is None:
        return None
    if not isinstance(contract, Mapping):
        raise SrsInteractionError(f"interaction contract for {object_type.value} must be a mapping")
    return contract


def _is_valid_interaction_range(player_position: Position, object_position: Position, interaction_range: object) -> bool:
    if interaction_range == "SAME_CELL":
        return player_position == object_position
    if interaction_range == "ADJACENT":
        distance = abs(player_position.x - object_position.x) + abs(player_position.y - object_position.y)
        return distance == 1
    raise SrsInteractionError(f"unsupported interaction range: {interaction_range}")


def _is_consumed_object(state: SrsGameState, object_state: SrsObjectState) -> bool:
    return object_state.consumed or object_state.object_id in state.persistent_state.consumed_object_ids


def _apply_persistent_object_flags(
    objects: Mapping[str, SrsObjectState],
    *,
    persistent: SrsPersistentState,
) -> Mapping[str, SrsObjectState]:
    normalized: dict[str, SrsObjectState] = {}
    for object_id, object_state in objects.items():
        normalized[object_id] = replace(
            object_state,
            consumed=object_state.consumed or object_id in persistent.consumed_object_ids,
            activated=object_state.activated or object_id in persistent.activated_object_ids,
        )
    return normalized


def _warp_exit_event(
    *,
    srs_turn: int,
    event_type: str,
    state: SrsGameState,
    exit_direction: Direction,
    start_position: Position,
    outcome: str,
):
    return make_turn_event(
        srs_turn=srs_turn,
        event_type=event_type,
        payload={
            "command_type": "WARP_EXIT",
            "exit_direction": exit_direction.value,
            "start_position": _position_to_list(start_position),
            "warp_position": _position_to_list(start_position),
            "sector_id": state.descriptor.sector_id,
            "generated_map_id": state.persistent_state.generated_map_id,
            "outcome": outcome,
        },
    )


def _movement_event(
    *,
    srs_turn: int,
    event_type: str,
    command_type: str,
    cost_mode: CostMode,
    start_position: Position,
    end_position: Position,
    entered_cells: Sequence[Position],
    blocked_position: Position | None,
    movement_raw_cost: int,
    fuel_delta: int,
    fuel_before: int,
    fuel_after: int,
    observation_updates: Sequence[Position],
    outcome: str,
    target_position: Position | None = None,
    resolved_route: Sequence[Direction] | None = None,
):
    payload = {
        "command_type": command_type,
        "movement_rule": MovementRule.MOVEMENT_POINTS.value,
        "cost_mode": cost_mode.value,
        "start_position": _position_to_list(start_position),
        "end_position": _position_to_list(end_position),
        "entered_cells": [_position_to_list(position) for position in entered_cells],
        "blocked_position": None if blocked_position is None else _position_to_list(blocked_position),
        "movement_raw_cost": movement_raw_cost,
        "fuel_delta": fuel_delta,
        "fuel_before": fuel_before,
        "fuel_after": fuel_after,
        "observation_updates": [_position_to_list(position) for position in observation_updates],
        "outcome": outcome,
    }
    if target_position is not None or resolved_route is not None:
        payload["target_position"] = None if target_position is None else _position_to_list(target_position)
        payload["resolved_route"] = [] if resolved_route is None else [direction.value for direction in resolved_route]
    return make_turn_event(
        srs_turn=srs_turn,
        event_type=event_type,
        payload=payload,
    )


def _movement_outcome(
    *,
    route: Sequence[Direction],
    entered_cells: Sequence[Position],
    blocked_position: Position | None,
) -> str:
    if blocked_position is not None:
        return "STOPPED_BEFORE_IMPASSABLE"
    if len(entered_cells) < len(route):
        return "BUDGET_EXHAUSTED"
    return "ACCEPTED"


def _movement_turn_cost(contracts: SrsContracts) -> int:
    turn_cost = contracts.movement.command_turn_rules.get("movement_turn_cost")
    if not isinstance(turn_cost, int) or isinstance(turn_cost, bool) or turn_cost <= 0:
        raise SrsMovementError("movement_turn_cost must be a positive integer")
    return turn_cost


def fuel_delta_for_movement_raw_cost(
    movement_raw_cost: int,
    *,
    cost_mode: CostMode,
    contracts: SrsContracts,
) -> int:
    if not isinstance(movement_raw_cost, int) or isinstance(movement_raw_cost, bool):
        raise SrsMovementError("movement_raw_cost must be an integer")
    if movement_raw_cost < 0:
        raise SrsMovementError("movement_raw_cost must be non-negative")
    if cost_mode is CostMode.TURN_ONLY or movement_raw_cost == 0:
        return 0
    if cost_mode is not CostMode.SHARED_FUEL:
        raise SrsMovementError(f"unsupported cost mode: {cost_mode}")

    denominator = _movement_raw_cost_denominator(contracts)
    return -((movement_raw_cost + denominator - 1) // denominator)


def _normalize_cost_mode(
    cost_mode: CostMode | None,
    *,
    contracts: SrsContracts,
) -> CostMode:
    raw_cost_mode = contracts.movement.baseline_cost_mode if cost_mode is None else cost_mode
    try:
        normalized = CostMode(raw_cost_mode)
    except ValueError as exc:
        raise SrsMovementError(f"unsupported cost mode: {raw_cost_mode}") from exc
    if normalized not in {CostMode.TURN_ONLY, CostMode.SHARED_FUEL}:
        raise SrsMovementError(f"unsupported cost mode: {raw_cost_mode}")
    return normalized


def _movement_raw_cost_denominator(contracts: SrsContracts) -> int:
    denominator = contracts.movement.orthogonal_raw_cost
    if not isinstance(denominator, int) or isinstance(denominator, bool) or denominator <= 0:
        raise SrsMovementError("raw_cost_denominator must be a positive integer")
    return denominator


def _reconstruct_route(
    came_from: Mapping[Position, tuple[Position, Direction] | None],
    *,
    start: Position,
    target: Position,
) -> tuple[Direction, ...]:
    route: list[Direction] = []
    current = target
    while current != start:
        previous = came_from.get(current)
        if previous is None:
            raise SrsMovementError(f"route reconstruction failed: {target}")
        current, direction = previous
        route.append(direction)
    route.reverse()
    return tuple(route)


def _step_position(position: Position, direction: Direction) -> Position:
    deltas = {
        Direction.N: (0, 1),
        Direction.E: (1, 0),
        Direction.S: (0, -1),
        Direction.W: (-1, 0),
    }
    dx, dy = deltas[direction]
    return Position(position.x + dx, position.y + dy)


def _position_sort_key(position: Position) -> tuple[int, int]:
    return (position.y, position.x)


def _position_to_list(position: Position) -> list[int]:
    return [position.x, position.y]


def _override_move_to_event(
    event,
    *,
    target_position: Position,
    resolved_route: Sequence[Direction],
):
    payload = dict(event.payload)
    payload["command_type"] = "MOVE_TO"
    payload["target_position"] = _position_to_list(target_position)
    payload["resolved_route"] = [direction.value for direction in resolved_route]
    return make_turn_event(
        srs_turn=event.srs_turn,
        event_type=event.event_type,
        payload=payload,
    )
