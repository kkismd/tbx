from __future__ import annotations

from dataclasses import dataclass, field
import json
from typing import Callable, Iterable

from experiments.galactic_exodus import simulate


SCHEMA_VERSION = 3
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

SUPPLY_RESULT_NONE = "NONE"
SUPPLY_RESULT_BASE_REFUELED = "BASE_REFUELED"
SUPPLY_RESULT_BASE_ALREADY_FULL = "BASE_ALREADY_FULL"
SUPPLY_RESULT_RESOURCE_REFUELED = "RESOURCE_REFUELED"
SUPPLY_RESULT_RESOURCE_ALREADY_FULL = "RESOURCE_ALREADY_FULL"
SUPPLY_RESULT_RESOURCE_ALREADY_USED = "RESOURCE_ALREADY_USED"

COMMAND_DELTAS: dict[str, tuple[int, int]] = {
    "N": (0, 1),
    "E": (1, 0),
    "S": (0, -1),
    "W": (-1, 0),
}

VALID_CELL_SYMBOLS = frozenset({".", "N", "A", "@", BASE_CELL, RESOURCE_CELL, "S", HOME_CELL})

CandidateGenerator = Callable[[int, int, float], simulate.GalacticMap]
ReachabilityPredicate = Callable[[simulate.GalacticMap], bool]


@dataclass(frozen=True)
class GameSettings:
    width: int = simulate.WIDTH
    height: int = simulate.HEIGHT
    start_position: simulate.Position = simulate.SPECIAL_S
    goal_position: simulate.Position = simulate.SPECIAL_H
    rift_density: float = simulate.DEFAULT_RIFT_DENSITY
    initial_fuel: int = simulate.DEFAULT_INITIAL_FUEL
    max_fuel: int = simulate.DEFAULT_INITIAL_FUEL
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
        simulate.validate_non_negative("max-fuel", self.max_fuel)
        simulate.validate_non_negative("resource-supply", self.resource_supply)
        simulate.validate_resource_count(self.resource_count)
        if self.initial_fuel > self.max_fuel:
            raise ValueError("initial-fuel must be less than or equal to max-fuel")


DEFAULT_SETTINGS = GameSettings()


@dataclass(frozen=True)
class ActualMap:
    cells: simulate.Cells
    rift_edges: tuple[simulate.Edge, ...]
    base_position: simulate.Position
    resource_positions: tuple[simulate.Position, ...]


@dataclass(frozen=True)
class SupplySource:
    kind: str
    position: simulate.Position

    def to_dict(self) -> dict[str, object]:
        return {
            "kind": self.kind,
            "position": position_to_dict(self.position),
        }


@dataclass(frozen=True)
class DiscoveredCell:
    position: simulate.Position
    symbol: str

    def to_dict(self) -> dict[str, object]:
        return {
            "position": position_to_dict(self.position),
            "symbol": self.symbol,
        }


@dataclass
class GameState:
    settings: GameSettings
    actual_map: ActualMap
    known_cells: dict[simulate.Position, str]
    visited_cells: set[simulate.Position]
    known_routes: dict[simulate.Edge, str]
    player_position: simulate.Position
    remaining_fuel: int
    used_resource_positions: set[simulate.Position]
    base_visit_count: int
    base_refuel_count: int
    resource_visit_count: int
    resource_refuel_count: int
    last_supply_source: SupplySource | None
    turn_count: int
    game_status: str
    requested_seed: int
    effective_seed: int
    reroll_count: int
    rift_attempt_count: int = 0
    invalid_or_rejected_action_count: int = 0
    path: list[simulate.Position] = field(default_factory=list)


@dataclass(frozen=True)
class GenerationErrorInfo:
    kind: str
    requested_seed: int
    attempts: int
    last_candidate_seed: int | None
    reason: str
    message: str

    def to_dict(self) -> dict[str, object]:
        return {
            "kind": self.kind,
            "requested_seed": self.requested_seed,
            "attempts": self.attempts,
            "last_candidate_seed": self.last_candidate_seed,
            "reason": self.reason,
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
    required_fuel: int | None
    discovered_cells: tuple[DiscoveredCell, ...]
    discovered_rift: bool
    supply_result: str
    supply_source: SupplySource | None
    fuel_before_supply: int | None
    fuel_after_supply: int | None
    supply_amount: int
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
            "required_fuel": self.required_fuel,
            "discovered_cells": [cell.to_dict() for cell in self.discovered_cells],
            "discovered_rift": self.discovered_rift,
            "supply_result": self.supply_result,
            "supply_source": optional_supply_source_to_dict(self.supply_source),
            "fuel_before_supply": self.fuel_before_supply,
            "fuel_after_supply": self.fuel_after_supply,
            "supply_amount": self.supply_amount,
            "status_after": self.status_after,
        }


@dataclass(frozen=True)
class FinalSummary:
    outcome: str
    turn_count: int
    remaining_fuel: int
    max_fuel: int
    used_resource_positions: tuple[simulate.Position, ...]
    base_visit_count: int
    base_refuel_count: int
    resource_visit_count: int
    resource_refuel_count: int
    last_supply_source: SupplySource | None
    rift_attempts: int
    invalid_or_rejected_actions: int
    path: tuple[simulate.Position, ...]

    def to_dict(self) -> dict[str, object]:
        return {
            "outcome": self.outcome,
            "turn_count": self.turn_count,
            "remaining_fuel": self.remaining_fuel,
            "max_fuel": self.max_fuel,
            "used_resource_positions": positions_to_sorted_dicts(set(self.used_resource_positions)),
            "base_visit_count": self.base_visit_count,
            "base_refuel_count": self.base_refuel_count,
            "resource_visit_count": self.resource_visit_count,
            "resource_refuel_count": self.resource_refuel_count,
            "last_supply_source": optional_supply_source_to_dict(self.last_supply_source),
            "rift_attempts": self.rift_attempts,
            "invalid_or_rejected_actions": self.invalid_or_rejected_actions,
            "path": [position_to_dict(position) for position in self.path],
        }


@dataclass(frozen=True)
class SupplyResolution:
    result: str
    source: SupplySource | None
    fuel_before_supply: int | None
    fuel_after_supply: int | None
    supply_amount: int


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
    def __init__(
        self,
        *,
        requested_seed: int,
        attempts: int,
        last_candidate_seed: int | None,
        reason: str,
        message: str,
    ):
        super().__init__(message)
        self.requested_seed = requested_seed
        self.attempts = attempts
        self.last_candidate_seed = last_candidate_seed
        self.reason = reason

    def to_info(self) -> GenerationErrorInfo:
        return GenerationErrorInfo(
            kind="GENERATION_ERROR",
            requested_seed=self.requested_seed,
            attempts=self.attempts,
            last_candidate_seed=self.last_candidate_seed,
            reason=self.reason,
            message=str(self),
        )


def validate_actual_map(actual_map: ActualMap, settings: GameSettings) -> None:
    expected_positions = {
        (x, y)
        for y in range(1, settings.height + 1)
        for x in range(1, settings.width + 1)
    }
    actual_positions = set(actual_map.cells)
    missing_positions = sorted(expected_positions - actual_positions)
    extra_positions = sorted(actual_positions - expected_positions)
    if missing_positions or extra_positions:
        raise ValueError(
            f"actual_map.cells must contain exactly the board positions; missing={missing_positions}, extra={extra_positions}"
        )

    for position, symbol in actual_map.cells.items():
        if symbol not in VALID_CELL_SYMBOLS:
            raise ValueError(f"actual_map.cells[{position}] contains invalid symbol {symbol!r}")

    if actual_map.cells[settings.start_position] != "S":
        raise ValueError("start_position cell must be 'S'")
    if actual_map.cells[settings.goal_position] != HOME_CELL:
        raise ValueError("goal_position cell must be 'H'")
    for position, symbol in actual_map.cells.items():
        if symbol == "S" and position != settings.start_position:
            raise ValueError(f"'S' may only appear at start_position, found at {position}")
        if symbol == HOME_CELL and position != settings.goal_position:
            raise ValueError(f"'H' may only appear at goal_position, found at {position}")

    if actual_map.cells[actual_map.base_position] != BASE_CELL:
        raise ValueError("base_position cell must be 'B'")
    base_cells = {position for position, symbol in actual_map.cells.items() if symbol == BASE_CELL}
    if base_cells != {actual_map.base_position}:
        raise ValueError("actual_map base_position must exactly match the board's 'B' cell")

    resource_positions = tuple(actual_map.resource_positions)
    if len(set(resource_positions)) != len(resource_positions):
        raise ValueError("resource_positions must not contain duplicates")
    for position in resource_positions:
        if actual_map.cells[position] != RESOURCE_CELL:
            raise ValueError(f"resource_positions cell {position} must be 'R'")
    resource_cells = {position for position, symbol in actual_map.cells.items() if symbol == RESOURCE_CELL}
    if resource_cells != set(resource_positions):
        raise ValueError("actual_map resource_positions must exactly match the board's 'R' cells")

    normalized_rift_edges: set[simulate.Edge] = set()
    for edge in actual_map.rift_edges:
        start, goal = edge
        if start not in expected_positions or goal not in expected_positions:
            raise ValueError(f"rift edge {edge} must stay inside the board")
        if abs(start[0] - goal[0]) + abs(start[1] - goal[1]) != 1:
            raise ValueError(f"rift edge {edge} must connect adjacent positions")
        normalized = simulate.normalize_edge(start, goal)
        if edge != normalized:
            raise ValueError(f"rift edge {edge} must be normalized")
        if normalized in normalized_rift_edges:
            raise ValueError(f"rift edge {edge} is duplicated")
        normalized_rift_edges.add(normalized)


def create_game_from_actual_map(
    actual_map: ActualMap,
    *,
    settings: GameSettings = DEFAULT_SETTINGS,
    requested_seed: int,
    effective_seed: int,
    reroll_count: int,
) -> GameState:
    settings.validate()
    validate_actual_map(actual_map, settings)
    state = GameState(
        settings=settings,
        actual_map=actual_map,
        known_cells={},
        visited_cells={settings.start_position},
        known_routes={},
        player_position=settings.start_position,
        remaining_fuel=settings.initial_fuel,
        used_resource_positions=set(),
        base_visit_count=0,
        base_refuel_count=0,
        resource_visit_count=0,
        resource_refuel_count=0,
        last_supply_source=None,
        turn_count=0,
        game_status=GAME_STATUS_IN_PROGRESS,
        requested_seed=requested_seed,
        effective_seed=effective_seed,
        reroll_count=reroll_count,
        path=[settings.start_position],
    )
    reveal_neighborhood(state, settings.start_position)
    reveal_neighborhood(state, settings.goal_position)
    state.game_status = determine_game_status(state)
    return state


def create_game(requested_seed: int, settings: GameSettings = DEFAULT_SETTINGS) -> GameState:
    settings.validate()
    galactic_map, effective_seed, reroll_count = create_playable_map(requested_seed, settings)
    actual_map = ActualMap(
        cells=dict(galactic_map.cells),
        rift_edges=tuple(galactic_map.rift_edges),
        base_position=galactic_map.b_position,
        resource_positions=tuple(galactic_map.r_positions),
    )
    return create_game_from_actual_map(
        actual_map,
        settings=settings,
        requested_seed=requested_seed,
        effective_seed=effective_seed,
        reroll_count=reroll_count,
    )


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

    return run_state_commands(state, commands, max_turns=max_turns)


def run_state_commands(
    state: GameState,
    commands: Iterable[str],
    *,
    max_turns: int = 256,
) -> GameLog:
    simulate.validate_non_negative("max-turns", max_turns)
    initial_state = snapshot_state(state)
    events: list[TurnEvent] = []
    command_iter = iter(commands)

    while state.game_status == GAME_STATUS_IN_PROGRESS:
        if state.turn_count >= max_turns:
            return build_game_log(
                settings=state.settings,
                requested_seed=state.requested_seed,
                state=state,
                initial_state=initial_state,
                events=events,
                final_outcome=FINAL_OUTCOME_ABORTED_TURN_LIMIT,
            )

        try:
            command = next(command_iter)
        except StopIteration:
            return build_game_log(
                settings=state.settings,
                requested_seed=state.requested_seed,
                state=state,
                initial_state=initial_state,
                events=events,
                final_outcome=FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION,
            )

        events.append(apply_command(state, command))

    return build_game_log(
        settings=state.settings,
        requested_seed=state.requested_seed,
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
            required_fuel=None,
            discovered_cells=(),
            discovered_rift=False,
            supply_result=SUPPLY_RESULT_NONE,
            supply_source=None,
            fuel_before_supply=None,
            fuel_after_supply=None,
            supply_amount=0,
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
            required_fuel=None,
            discovered_cells=(),
            discovered_rift=False,
            supply_result=SUPPLY_RESULT_NONE,
            supply_source=None,
            fuel_before_supply=None,
            fuel_after_supply=None,
            supply_amount=0,
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
            required_fuel=None,
            discovered_cells=(),
            discovered_rift=False,
            supply_result=SUPPLY_RESULT_NONE,
            supply_source=None,
            fuel_before_supply=None,
            fuel_after_supply=None,
            supply_amount=0,
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
                required_fuel=1,
                discovered_cells=(),
                discovered_rift=False,
                supply_result=SUPPLY_RESULT_NONE,
                supply_source=None,
                fuel_before_supply=None,
                fuel_after_supply=None,
                supply_amount=0,
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
            required_fuel=None,
            discovered_cells=(),
            discovered_rift=True,
            supply_result=SUPPLY_RESULT_NONE,
            supply_source=None,
            fuel_before_supply=None,
            fuel_after_supply=None,
            supply_amount=0,
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
            required_fuel=fuel_cost,
            discovered_cells=(),
            discovered_rift=False,
            supply_result=SUPPLY_RESULT_NONE,
            supply_source=None,
            fuel_before_supply=None,
            fuel_after_supply=None,
            supply_amount=0,
            status_after=state.game_status,
        )

    state.remaining_fuel -= fuel_cost
    state.turn_count += 1
    state.player_position = attempted_position
    state.known_routes[edge] = ROUTE_OPEN
    state.visited_cells.add(attempted_position)
    state.path.append(attempted_position)
    discovered_cells = reveal_neighborhood(state, attempted_position)

    supply_resolution = maybe_apply_supply(
        state,
        destination_symbol,
        attempted_position,
    )

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
        required_fuel=None,
        discovered_cells=discovered_cells,
        discovered_rift=False,
        supply_result=supply_resolution.result,
        supply_source=supply_resolution.source,
        fuel_before_supply=supply_resolution.fuel_before_supply,
        fuel_after_supply=supply_resolution.fuel_after_supply,
        supply_amount=supply_resolution.supply_amount,
        status_after=state.game_status,
    )


def create_playable_map(
    requested_seed: int,
    settings: GameSettings,
    *,
    generate_candidate: CandidateGenerator | None = None,
    is_reachable: ReachabilityPredicate | None = None,
) -> tuple[simulate.GalacticMap, int, int]:
    if generate_candidate is None:
        generate_candidate = simulate.generate_map
    if is_reachable is None:
        is_reachable = is_goal_reachable
    last_candidate_seed: int | None = None
    for attempt in range(MAX_GENERATION_ATTEMPTS):
        candidate_seed = add_seed_offset(requested_seed, attempt)
        last_candidate_seed = candidate_seed
        galactic_map = generate_candidate(
            candidate_seed,
            settings.resource_count,
            settings.rift_density,
        )
        if is_reachable(galactic_map):
            return galactic_map, candidate_seed, attempt
    raise GenerationError(
        requested_seed=requested_seed,
        attempts=MAX_GENERATION_ATTEMPTS,
        last_candidate_seed=last_candidate_seed,
        reason="NO_REACHABLE_MAP",
        message=f"failed to generate a reachable map after {MAX_GENERATION_ATTEMPTS} attempts",
    )


def add_seed_offset(seed: int, offset: int) -> int:
    candidate = seed + offset
    if not MIN_INT64 <= candidate <= MAX_INT64:
        last_candidate_seed = None if offset == 0 else seed + offset - 1
        raise GenerationError(
            requested_seed=seed,
            attempts=offset + 1,
            last_candidate_seed=last_candidate_seed,
            reason="SEED_OVERFLOW",
            message=f"seed overflow while adding attempt {offset} to requested_seed={seed}",
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


def maybe_apply_supply(
    state: GameState,
    destination_symbol: str,
    position: simulate.Position,
) -> SupplyResolution:
    if destination_symbol == BASE_CELL:
        return apply_base_supply(state, position)
    if destination_symbol == RESOURCE_CELL:
        return apply_resource_supply(state, position)
    return SupplyResolution(
        result=SUPPLY_RESULT_NONE,
        source=None,
        fuel_before_supply=None,
        fuel_after_supply=None,
        supply_amount=0,
    )


def apply_base_supply(
    state: GameState,
    position: simulate.Position,
) -> SupplyResolution:
    state.base_visit_count += 1
    source = SupplySource(kind=BASE_CELL, position=position)
    fuel_before_supply = state.remaining_fuel
    supply_amount = state.settings.max_fuel - fuel_before_supply
    if supply_amount <= 0:
        return SupplyResolution(
            result=SUPPLY_RESULT_BASE_ALREADY_FULL,
            source=source,
            fuel_before_supply=fuel_before_supply,
            fuel_after_supply=fuel_before_supply,
            supply_amount=0,
        )

    state.remaining_fuel = state.settings.max_fuel
    state.base_refuel_count += 1
    state.last_supply_source = source
    return SupplyResolution(
        result=SUPPLY_RESULT_BASE_REFUELED,
        source=source,
        fuel_before_supply=fuel_before_supply,
        fuel_after_supply=state.remaining_fuel,
        supply_amount=supply_amount,
    )


def apply_resource_supply(
    state: GameState,
    position: simulate.Position,
) -> SupplyResolution:
    state.resource_visit_count += 1
    source = SupplySource(kind=RESOURCE_CELL, position=position)
    fuel_before_supply = state.remaining_fuel
    if position in state.used_resource_positions:
        return SupplyResolution(
            result=SUPPLY_RESULT_RESOURCE_ALREADY_USED,
            source=source,
            fuel_before_supply=fuel_before_supply,
            fuel_after_supply=fuel_before_supply,
            supply_amount=0,
        )

    supply_amount = min(
        state.settings.resource_supply,
        state.settings.max_fuel - state.remaining_fuel,
    )
    if supply_amount <= 0:
        return SupplyResolution(
            result=SUPPLY_RESULT_RESOURCE_ALREADY_FULL,
            source=source,
            fuel_before_supply=fuel_before_supply,
            fuel_after_supply=fuel_before_supply,
            supply_amount=0,
        )

    state.remaining_fuel += supply_amount
    state.used_resource_positions.add(position)
    state.resource_refuel_count += 1
    state.last_supply_source = source
    return SupplyResolution(
        result=SUPPLY_RESULT_RESOURCE_REFUELED,
        source=source,
        fuel_before_supply=fuel_before_supply,
        fuel_after_supply=state.remaining_fuel,
        supply_amount=supply_amount,
    )


def reveal_neighborhood(
    state: GameState,
    center: simulate.Position,
    radius: int = 1,
) -> tuple[DiscoveredCell, ...]:
    discovered: list[DiscoveredCell] = []
    for y in range(center[1] - radius, center[1] + radius + 1):
        for x in range(center[0] - radius, center[0] + radius + 1):
            position = (x, y)
            if not is_inside_board(position):
                continue
            if position in state.known_cells:
                continue
            symbol = state.actual_map.cells[position]
            state.known_cells[position] = symbol
            discovered.append(DiscoveredCell(position=position, symbol=symbol))
    return tuple(sorted(discovered, key=lambda cell: cell.position))


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
            max_fuel=state.settings.max_fuel,
            used_resource_positions=tuple(sorted(state.used_resource_positions)),
            base_visit_count=state.base_visit_count,
            base_refuel_count=state.base_refuel_count,
            resource_visit_count=state.resource_visit_count,
            resource_refuel_count=state.resource_refuel_count,
            last_supply_source=state.last_supply_source,
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
        "max_fuel": state.settings.max_fuel,
        "used_resource_positions": positions_to_sorted_dicts(state.used_resource_positions),
        "base_visit_count": state.base_visit_count,
        "base_refuel_count": state.base_refuel_count,
        "resource_visit_count": state.resource_visit_count,
        "resource_refuel_count": state.resource_refuel_count,
        "last_supply_source": optional_supply_source_to_dict(state.last_supply_source),
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
        "max_fuel": settings.max_fuel,
        "resource_count": settings.resource_count,
        "resource_supply": settings.resource_supply,
    }


def actual_map_to_dict(actual_map: ActualMap) -> dict[str, object]:
    return {
        "cells": cells_to_sorted_rows(actual_map.cells),
        "rift_edges": [
            [
                position_to_dict(edge[0]),
                position_to_dict(edge[1]),
            ]
            for edge in sorted(actual_map.rift_edges)
        ],
        "base_position": position_to_dict(actual_map.base_position),
        "resource_positions": [position_to_dict(position) for position in sorted(actual_map.resource_positions)],
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


def optional_supply_source_to_dict(source: SupplySource | None) -> dict[str, object] | None:
    if source is None:
        return None
    return source.to_dict()


def edge_to_dict(edge: simulate.Edge) -> dict[str, object]:
    return {
        "from": position_to_dict(edge[0]),
        "to": position_to_dict(edge[1]),
    }
