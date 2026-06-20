from __future__ import annotations

from collections import deque
from dataclasses import replace
from typing import Any, Mapping, Sequence

from experiments.galactic_exodus.srs.contracts import SrsContracts
from experiments.galactic_exodus.srs.log import (
    MOVE_ACCEPTED,
    MOVE_REJECTED,
    OBSERVATION_UPDATED,
    STOPPED_BEFORE_IMPASSABLE,
    make_turn_event,
)
from experiments.galactic_exodus.srs.model import (
    Direction,
    CostMode,
    MovementRule,
    Position,
    SectorDescriptor,
    SrsActualMap,
    SrsCell,
    SrsCommand,
    SrsCommandResult,
    SrsGameState,
    SrsKnownState,
    SrsObjectType,
    SrsObjectState,
    SrsPersistentState,
    SrsTerrainType,
)


class SrsObservationError(ValueError):
    pass


class SrsMovementError(ValueError):
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


def movement_raw_cost_for_step(
    destination: SrsCell,
    *,
    direction: Direction,
    contracts: SrsContracts,
) -> int:
    if direction not in contracts.movement.directions:
        raise SrsMovementError(f"unsupported movement direction: {direction}")

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
    return contracts.movement.orthogonal_raw_cost * multiplier


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
    return _rejected_command_result(
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
        return _rejected_command_result(
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
        return _rejected_command_result(
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
        return _rejected_command_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_OUT_OF_BOUNDS",
            cost_mode=cost_mode,
            target_position=target,
            resolved_route=(),
        )
    if target == state.player_position:
        return _rejected_command_result(
            state,
            command_type=command.command_type,
            outcome="REJECTED_SAME_POSITION",
            cost_mode=cost_mode,
            target_position=target,
            resolved_route=(),
        )
    if target not in state.known_state.discovered_cells:
        return _rejected_command_result(
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
        return _rejected_command_result(
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


def _rejected_command_result(
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
        Direction.N: (0, -1),
        Direction.E: (1, 0),
        Direction.S: (0, 1),
        Direction.W: (-1, 0),
    }
    dx, dy = deltas[direction]
    return Position(position.x + dx, position.y + dy)


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
