from __future__ import annotations

from dataclasses import dataclass, field
import json
from typing import Iterable

from experiments.galactic_exodus import simulate


SCHEMA_VERSION = 1
MAX_GENERATION_ATTEMPTS = 100
MIN_INT64 = -(2**63)
MAX_INT64 = 2**63 - 1

GAME_STATUS_IN_PROGRESS = "IN_PROGRESS"
GAME_STATUS_WON = "WON"
GAME_STATUS_LOST_FUEL = "LOST_FUEL"

OUTCOME_MOVED = "MOVED"
OUTCOME_BLOCKED_UNKNOWN_RIFT = "BLOCKED_UNKNOWN_RIFT"
OUTCOME_REJECTED_KNOWN_RIFT = "REJECTED_KNOWN_RIFT"
OUTCOME_REJECTED_INSUFFICIENT_FUEL = "REJECTED_INSUFFICIENT_FUEL"
OUTCOME_INVALID_COMMAND = "INVALID_COMMAND"
OUTCOME_OUT_OF_BOUNDS = "OUT_OF_BOUNDS"

FINAL_OUTCOME_WON = "WON"
FINAL_OUTCOME_LOST_FUEL = "LOST_FUEL"
FINAL_OUTCOME_ABORTED_TURN_LIMIT = "ABORTED_TURN_LIMIT"
FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION = "ABORTED_NO_POLICY_ACTION"

ROUTE_OPEN = "OPEN"
ROUTE_RIFT = "RIFT"

RESOURCE_CELL = "R"
BASE_CELL = "B"
HOME_CELL = "H"

COMMAND_DELTAS: dict[str, tuple[int, int]] = {
    "N": (0, 1),
    "E": (1, 0),
    "S": (0, -1),
    "W": (-1, 0),
}


@dataclass(frozen=True)
class GameSettings:
    width: int = simulate.WIDTH
    height: int = simulate.HEIGHT
    start_position: simulate.Position = simulate.SPECIAL_S
    goal_position: simulate.Position = simulate.SPECIAL_H
    rift_density: float = simulate.DEFAULT_RIFT_DENSITY
    initial_fuel: int = simulate.DEFAULT_INITIAL_FUEL
    base_supply: int = simulate.DEFAULT_BASE_SUPPLY
    resource_count: int = simulate.DEFAULT_RESOURCE_COUNT
    resource_supply: int = simulate.DEFAULT_RESOURCE_SUPPLY

    def validate(self) -> None:
        if self.width != simulate.WIDTH or self.height != simulate.HEIGHT:
            raise ValueError("only the fixed 8x8 board is supported")
        if self.start_position != simulate.SPECIAL_S:
            raise ValueError("start_position must be (1, 1)")
        if self.goal_position != simulate.SPECIAL_H:
            raise ValueError("goal_position must be (8, 8)")
        simulate.validate_rift_density(self.rift_density)
        simulate.validate_non_negative("initial-fuel", self.initial_fuel)
        simulate.validate_non_negative("base-supply", self.base_supply)
        simulate.validate_non_negative("resource-supply", self.resource_supply)
        simulate.validate_resource_count(self.resource_count)


DEFAULT_SETTINGS = GameSettings()


@dataclass(frozen=True)
class ActualMap:
    cells: simulate.Cells
    rift_edges: tuple[simulate.Edge, ...]
    base_position: simulate.Position
    resource_positions: tuple[simulate.Position, ...]


@dataclass
class GameState:
    settings: GameSettings
    actual_map: ActualMap
    known_cells: dict[simulate.Position, str]
    visited_cells: set[simulate.Position]
    known_routes: dict[simulate.Edge, str]
    player_position: simulate.Position
    remaining_fuel: int
    supply_used: bool
    supply_source: str | None
    turn_count: int
    game_status: str
    requested_seed: int
    effective_seed: int
    reroll_count: int
    resource_visit_count: int = 0
    rift_attempt_count: int = 0
    invalid_or_rejected_action_count: int = 0
    path: list[simulate.Position] = field(default_factory=list)
    base_visited: bool = False


@dataclass(frozen=True)
class GenerationErrorInfo:
    kind: str
    message: str

    def to_dict(self) -> dict[str, object]:
        return {
            "kind": self.kind,
            "message": self.message,
        }


@dataclass(frozen=True)
class TurnEvent:
    turn: int
    command: str
    outcome: str
    from_position: simulate.Position
    attempted_position: simulate.Position | None
    to_position: simulate.Position
    fuel_before: int
    fuel_spent: int
    fuel_after: int
    discovered_cell: str | None
    discovered_rift: bool
    supply_applied: bool
    supply_source: str | None
    status_after: str

    def to_dict(self) -> dict[str, object]:
        return {
            "turn": self.turn,
            "command": self.command,
            "outcome": self.outcome,
            "from_position": position_to_dict(self.from_position),
            "attempted_position": optional_position_to_dict(self.attempted_position),
            "to_position": position_to_dict(self.to_position),
            "fuel_before": self.fuel_before,
            "fuel_spent": self.fuel_spent,
            "fuel_after": self.fuel_after,
            "discovered_cell": self.discovered_cell,
            "discovered_rift": self.discovered_rift,
            "supply_applied": self.supply_applied,
            "supply_source": self.supply_source,
            "status_after": self.status_after,
        }


@dataclass(frozen=True)
class FinalSummary:
    outcome: str
    turn_count: int
    remaining_fuel: int
    supply_source: str | None
    base_visited: bool
    resource_visits: int
    rift_attempts: int
    invalid_or_rejected_actions: int
    path: tuple[simulate.Position, ...]

    def to_dict(self) -> dict[str, object]:
        return {
            "outcome": self.outcome,
            "turn_count": self.turn_count,
            "remaining_fuel": self.remaining_fuel,
            "supply_source": self.supply_source,
            "base_visited": self.base_visited,
            "resource_visits": self.resource_visits,
            "rift_attempts": self.rift_attempts,
            "invalid_or_rejected_actions": self.invalid_or_rejected_actions,
            "path": [position_to_dict(position) for position in self.path],
        }


@dataclass(frozen=True)
class GameLog:
    schema_version: int
    settings: GameSettings
    requested_seed: int
    effective_seed: int | None
    reroll_count: int | None
    initial_state: dict[str, object] | None
    events: tuple[TurnEvent, ...]
    final_summary: FinalSummary | None
    generation_error: GenerationErrorInfo | None

    def to_dict(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "settings": settings_to_dict(self.settings),
            "requested_seed": self.requested_seed,
            "effective_seed": self.effective_seed,
            "reroll_count": self.reroll_count,
            "initial_state": self.initial_state,
            "events": [event.to_dict() for event in self.events],
            "final_summary": None if self.final_summary is None else self.final_summary.to_dict(),
            "generation_error": None if self.generation_error is None else self.generation_error.to_dict(),
        }

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=True, separators=(",", ":"))


class GenerationError(ValueError):
    def __init__(self, kind: str, message: str):
        super().__init__(message)
        self.kind = kind

    def to_info(self) -> GenerationErrorInfo:
        return GenerationErrorInfo(kind=self.kind, message=str(self))


def create_game(requested_seed: int, settings: GameSettings = DEFAULT_SETTINGS) -> GameState:
    settings.validate()
    galactic_map, effective_seed, reroll_count = create_playable_map(requested_seed, settings)
    actual_map = ActualMap(
        cells=dict(galactic_map.cells),
        rift_edges=tuple(galactic_map.rift_edges),
        base_position=galactic_map.b_position,
        resource_positions=tuple(galactic_map.r_positions),
    )
    known_cells = {
        settings.start_position: actual_map.cells[settings.start_position],
        settings.goal_position: actual_map.cells[settings.goal_position],
    }
    state = GameState(
        settings=settings,
        actual_map=actual_map,
        known_cells=known_cells,
        visited_cells={settings.start_position},
        known_routes={},
        player_position=settings.start_position,
        remaining_fuel=settings.initial_fuel,
        supply_used=False,
        supply_source=None,
        turn_count=0,
        game_status=GAME_STATUS_IN_PROGRESS,
        requested_seed=requested_seed,
        effective_seed=effective_seed,
        reroll_count=reroll_count,
        path=[settings.start_position],
    )
    state.game_status = determine_game_status(state)
    return state


def run_commands(
    requested_seed: int,
    commands: Iterable[str],
    settings: GameSettings = DEFAULT_SETTINGS,
    max_turns: int = 256,
) -> GameLog:
    simulate.validate_non_negative("max-turns", max_turns)
    try:
        state = create_game(requested_seed, settings)
    except GenerationError as exc:
        return GameLog(
            schema_version=SCHEMA_VERSION,
            settings=settings,
            requested_seed=requested_seed,
            effective_seed=None,
            reroll_count=None,
            initial_state=None,
            events=(),
            final_summary=None,
            generation_error=exc.to_info(),
        )

    initial_state = snapshot_state(state)
    events: list[TurnEvent] = []
    command_iter = iter(commands)

    while state.game_status == GAME_STATUS_IN_PROGRESS:
        if state.turn_count >= max_turns:
            return build_game_log(
                settings=settings,
                requested_seed=requested_seed,
                state=state,
                initial_state=initial_state,
                events=events,
                final_outcome=FINAL_OUTCOME_ABORTED_TURN_LIMIT,
            )

        try:
            command = next(command_iter)
        except StopIteration:
            return build_game_log(
                settings=settings,
                requested_seed=requested_seed,
                state=state,
                initial_state=initial_state,
                events=events,
                final_outcome=FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION,
            )

        events.append(apply_command(state, command))

    return build_game_log(
        settings=settings,
        requested_seed=requested_seed,
        state=state,
        initial_state=initial_state,
        events=events,
        final_outcome=final_outcome_for_status(state.game_status),
    )


def apply_command(state: GameState, command: str) -> TurnEvent:
    if state.game_status != GAME_STATUS_IN_PROGRESS:
        raise ValueError("game is already finished")

    normalized_command = command.strip().upper()
    if normalized_command not in COMMAND_DELTAS:
        state.invalid_or_rejected_action_count += 1
        return TurnEvent(
            turn=state.turn_count,
            command=normalized_command,
            outcome=OUTCOME_INVALID_COMMAND,
            from_position=state.player_position,
            attempted_position=None,
            to_position=state.player_position,
            fuel_before=state.remaining_fuel,
            fuel_spent=0,
            fuel_after=state.remaining_fuel,
            discovered_cell=None,
            discovered_rift=False,
            supply_applied=False,
            supply_source=None,
            status_after=state.game_status,
        )

    from_position = state.player_position
    attempted_position = move_position(from_position, COMMAND_DELTAS[normalized_command])
    fuel_before = state.remaining_fuel

    if not is_inside_board(attempted_position):
        state.invalid_or_rejected_action_count += 1
        return TurnEvent(
            turn=state.turn_count,
            command=normalized_command,
            outcome=OUTCOME_OUT_OF_BOUNDS,
            from_position=from_position,
            attempted_position=attempted_position,
            to_position=from_position,
            fuel_before=fuel_before,
            fuel_spent=0,
            fuel_after=state.remaining_fuel,
            discovered_cell=None,
            discovered_rift=False,
            supply_applied=False,
            supply_source=None,
            status_after=state.game_status,
        )

    edge = simulate.normalize_edge(from_position, attempted_position)
    route_state = state.known_routes.get(edge)
    if route_state == ROUTE_RIFT:
        state.invalid_or_rejected_action_count += 1
        return TurnEvent(
            turn=state.turn_count,
            command=normalized_command,
            outcome=OUTCOME_REJECTED_KNOWN_RIFT,
            from_position=from_position,
            attempted_position=attempted_position,
            to_position=from_position,
            fuel_before=fuel_before,
            fuel_spent=0,
            fuel_after=state.remaining_fuel,
            discovered_cell=None,
            discovered_rift=False,
            supply_applied=False,
            supply_source=None,
            status_after=state.game_status,
        )

    if is_rift_edge(state, edge):
        if fuel_before < 1:
            state.invalid_or_rejected_action_count += 1
            return TurnEvent(
                turn=state.turn_count,
                command=normalized_command,
                outcome=OUTCOME_REJECTED_INSUFFICIENT_FUEL,
                from_position=from_position,
                attempted_position=attempted_position,
                to_position=from_position,
                fuel_before=fuel_before,
                fuel_spent=0,
                fuel_after=state.remaining_fuel,
                discovered_cell=None,
                discovered_rift=False,
                supply_applied=False,
                supply_source=None,
                status_after=state.game_status,
            )

        state.remaining_fuel -= 1
        state.turn_count += 1
        state.known_routes[edge] = ROUTE_RIFT
        state.rift_attempt_count += 1
        state.game_status = determine_game_status(state)
        return TurnEvent(
            turn=state.turn_count,
            command=normalized_command,
            outcome=OUTCOME_BLOCKED_UNKNOWN_RIFT,
            from_position=from_position,
            attempted_position=attempted_position,
            to_position=from_position,
            fuel_before=fuel_before,
            fuel_spent=1,
            fuel_after=state.remaining_fuel,
            discovered_cell=None,
            discovered_rift=True,
            supply_applied=False,
            supply_source=None,
            status_after=state.game_status,
        )

    destination_symbol = state.actual_map.cells[attempted_position]
    fuel_cost = simulate.terrain_cost(destination_symbol)
    if fuel_before < fuel_cost:
        state.invalid_or_rejected_action_count += 1
        return TurnEvent(
            turn=state.turn_count,
            command=normalized_command,
            outcome=OUTCOME_REJECTED_INSUFFICIENT_FUEL,
            from_position=from_position,
            attempted_position=attempted_position,
            to_position=from_position,
            fuel_before=fuel_before,
            fuel_spent=0,
            fuel_after=state.remaining_fuel,
            discovered_cell=None,
            discovered_rift=False,
            supply_applied=False,
            supply_source=None,
            status_after=state.game_status,
        )

    state.remaining_fuel -= fuel_cost
    state.turn_count += 1
    state.player_position = attempted_position
    state.known_routes[edge] = ROUTE_OPEN
    state.visited_cells.add(attempted_position)
    state.path.append(attempted_position)
    discovered_cell = None
    if attempted_position not in state.known_cells:
        state.known_cells[attempted_position] = destination_symbol
        discovered_cell = destination_symbol

    supply_applied, supply_source = maybe_apply_supply(state, destination_symbol)
    if destination_symbol == BASE_CELL:
        state.base_visited = True
    if destination_symbol == RESOURCE_CELL:
        state.resource_visit_count += 1

    state.game_status = determine_game_status(state)
    return TurnEvent(
        turn=state.turn_count,
        command=normalized_command,
        outcome=OUTCOME_MOVED,
        from_position=from_position,
        attempted_position=attempted_position,
        to_position=attempted_position,
        fuel_before=fuel_before,
        fuel_spent=fuel_cost,
        fuel_after=state.remaining_fuel,
        discovered_cell=discovered_cell,
        discovered_rift=False,
        supply_applied=supply_applied,
        supply_source=supply_source,
        status_after=state.game_status,
    )


def create_playable_map(
    requested_seed: int,
    settings: GameSettings,
) -> tuple[simulate.GalacticMap, int, int]:
    for attempt in range(MAX_GENERATION_ATTEMPTS):
        candidate_seed = add_seed_offset(requested_seed, attempt)
        galactic_map = simulate.generate_map(
            candidate_seed,
            settings.resource_count,
            settings.rift_density,
        )
        if is_goal_reachable(galactic_map):
            return galactic_map, candidate_seed, attempt
    raise GenerationError(
        "NO_REACHABLE_MAP",
        f"failed to generate a reachable map after {MAX_GENERATION_ATTEMPTS} attempts",
    )


def add_seed_offset(seed: int, offset: int) -> int:
    candidate = seed + offset
    if not MIN_INT64 <= candidate <= MAX_INT64:
        raise GenerationError(
            "SEED_OVERFLOW",
            f"seed overflow while adding attempt {offset} to requested_seed={seed}",
        )
    return candidate


def is_goal_reachable(galactic_map: simulate.GalacticMap) -> bool:
    return (
        simulate.shortest_path(
            galactic_map.cells,
            simulate.SPECIAL_S,
            simulate.SPECIAL_H,
            set(galactic_map.rift_edges),
        )
        is not None
    )


def maybe_apply_supply(state: GameState, destination_symbol: str) -> tuple[bool, str | None]:
    if state.supply_used:
        return False, None
    if destination_symbol == BASE_CELL:
        supply_amount = state.settings.base_supply
    elif destination_symbol == RESOURCE_CELL:
        supply_amount = state.settings.resource_supply
    else:
        return False, None
    state.supply_used = True
    state.supply_source = destination_symbol
    state.remaining_fuel += supply_amount
    return True, destination_symbol


def determine_game_status(state: GameState) -> str:
    if state.player_position == simulate.SPECIAL_H:
        return GAME_STATUS_WON
    if can_continue(state):
        return GAME_STATUS_IN_PROGRESS
    return GAME_STATUS_LOST_FUEL


def can_continue(state: GameState) -> bool:
    for neighbor in simulate.neighbors(state.player_position):
        edge = simulate.normalize_edge(state.player_position, neighbor)
        if is_rift_edge(state, edge):
            continue
        terrain_symbol = state.actual_map.cells[neighbor]
        if simulate.terrain_cost(terrain_symbol) <= state.remaining_fuel:
            return True
    return False


def is_rift_edge(state: GameState, edge: simulate.Edge) -> bool:
    return edge in state.actual_map.rift_edges


def move_position(position: simulate.Position, delta: tuple[int, int]) -> simulate.Position:
    return (position[0] + delta[0], position[1] + delta[1])


def is_inside_board(position: simulate.Position) -> bool:
    x, y = position
    return 1 <= x <= simulate.WIDTH and 1 <= y <= simulate.HEIGHT


def final_outcome_for_status(status: str) -> str:
    if status == GAME_STATUS_WON:
        return FINAL_OUTCOME_WON
    if status == GAME_STATUS_LOST_FUEL:
        return FINAL_OUTCOME_LOST_FUEL
    raise ValueError(f"unexpected terminal status: {status}")


def build_game_log(
    *,
    settings: GameSettings,
    requested_seed: int,
    state: GameState,
    initial_state: dict[str, object],
    events: list[TurnEvent],
    final_outcome: str,
) -> GameLog:
    return GameLog(
        schema_version=SCHEMA_VERSION,
        settings=settings,
        requested_seed=requested_seed,
        effective_seed=state.effective_seed,
        reroll_count=state.reroll_count,
        initial_state=initial_state,
        events=tuple(events),
        final_summary=FinalSummary(
            outcome=final_outcome,
            turn_count=state.turn_count,
            remaining_fuel=state.remaining_fuel,
            supply_source=state.supply_source,
            base_visited=state.base_visited,
            resource_visits=state.resource_visit_count,
            rift_attempts=state.rift_attempt_count,
            invalid_or_rejected_actions=state.invalid_or_rejected_action_count,
            path=tuple(state.path),
        ),
        generation_error=None,
    )


def snapshot_state(state: GameState) -> dict[str, object]:
    return {
        "actual_map": actual_map_to_dict(state.actual_map),
        "known_cells": known_cells_to_dict(state.known_cells),
        "visited_cells": positions_to_sorted_dicts(state.visited_cells),
        "known_routes": known_routes_to_dict(state.known_routes),
        "player_position": position_to_dict(state.player_position),
        "remaining_fuel": state.remaining_fuel,
        "supply_used": state.supply_used,
        "supply_source": state.supply_source,
        "turn_count": state.turn_count,
        "game_status": state.game_status,
        "requested_seed": state.requested_seed,
        "effective_seed": state.effective_seed,
        "reroll_count": state.reroll_count,
    }


def settings_to_dict(settings: GameSettings) -> dict[str, object]:
    return {
        "width": settings.width,
        "height": settings.height,
        "start_position": position_to_dict(settings.start_position),
        "goal_position": position_to_dict(settings.goal_position),
        "rift_density": settings.rift_density,
        "initial_fuel": settings.initial_fuel,
        "base_supply": settings.base_supply,
        "resource_count": settings.resource_count,
        "resource_supply": settings.resource_supply,
    }


def actual_map_to_dict(actual_map: ActualMap) -> dict[str, object]:
    return {
        "cells": cells_to_sorted_rows(actual_map.cells),
        "rift_edges": [
            edge_to_dict(edge)
            for edge in sorted(actual_map.rift_edges)
        ],
        "base_position": position_to_dict(actual_map.base_position),
        "resource_positions": [position_to_dict(position) for position in actual_map.resource_positions],
    }


def known_cells_to_dict(known_cells: dict[simulate.Position, str]) -> list[dict[str, object]]:
    return [
        {
            "position": position_to_dict(position),
            "symbol": known_cells[position],
        }
        for position in sorted(known_cells)
    ]


def known_routes_to_dict(known_routes: dict[simulate.Edge, str]) -> list[dict[str, object]]:
    return [
        {
            "edge": edge_to_dict(edge),
            "state": known_routes[edge],
        }
        for edge in sorted(known_routes)
    ]


def cells_to_sorted_rows(cells: simulate.Cells) -> list[dict[str, object]]:
    return [
        {
            "position": position_to_dict(position),
            "symbol": cells[position],
        }
        for position in sorted(cells)
    ]


def positions_to_sorted_dicts(positions: set[simulate.Position]) -> list[dict[str, int]]:
    return [position_to_dict(position) for position in sorted(positions)]


def position_to_dict(position: simulate.Position) -> dict[str, int]:
    return {
        "x": position[0],
        "y": position[1],
    }


def optional_position_to_dict(position: simulate.Position | None) -> dict[str, int] | None:
    if position is None:
        return None
    return position_to_dict(position)


def edge_to_dict(edge: simulate.Edge) -> dict[str, object]:
    return {
        "from": position_to_dict(edge[0]),
        "to": position_to_dict(edge[1]),
    }
