from __future__ import annotations

import argparse
import csv
import json
from collections import deque
from dataclasses import dataclass, replace
from enum import Enum
import statistics
from pathlib import Path
import sys
from types import MappingProxyType
from typing import Any, Callable, Iterable, Mapping

from experiments.galactic_exodus import metrics
from experiments.galactic_exodus.srs.contracts import SrsContracts, load_default_contracts
from experiments.galactic_exodus.srs.engine import (
    apply_srs_command,
    restore_srs_state,
    reveal_full_observation,
    reveal_observation,
)
from experiments.galactic_exodus.srs.generate import create_sector
from experiments.galactic_exodus.srs.log import (
    INTERACT_REJECTED,
    MOVE_REJECTED,
    OBSERVATION_UPDATED,
    WARP_EXIT_ACCEPTED,
    WARP_EXIT_REJECTED,
)
from experiments.galactic_exodus.srs.model import (
    CostMode,
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsCell,
    SrsCommand,
    SrsGameState,
    SrsModelError,
    SrsObjectState,
    SrsObjectType,
    SrsTerrainType,
    SrsTurnEvent,
    validate_sector_descriptor,
)


class EvaluationCaseError(ValueError):
    pass


class InitialRevealMode(str, Enum):
    NONE = "NONE"
    FULL = "FULL"
    LOCAL_MOVEMENT = "LOCAL_MOVEMENT"


class RevisitMode(str, Enum):
    FIRST_VISIT = "FIRST_VISIT"
    REVISIT_PRESERVE_DISCOVERY = "REVISIT_PRESERVE_DISCOVERY"
    REVISIT_AFTER_PRIMARY_INTERACTION = "REVISIT_AFTER_PRIMARY_INTERACTION"


_DIRECTION_ORDER = (Direction.N, Direction.E, Direction.S, Direction.W)
_REVISIT_CONSUMED_OBJECT_TYPES = frozenset({SrsObjectType.RESOURCE_CACHE, SrsObjectType.SALVAGE})
_KNOWN_IMPASSABLE_TERRAINS = frozenset({SrsTerrainType.ASTEROID, SrsTerrainType.RIFT_BARRIER})
_KNOWN_IMPASSABLE_OBJECT_TYPES = frozenset({SrsObjectType.STAR, SrsObjectType.PLANET, SrsObjectType.STATION})
EXIT_GREEDY_POLICY_NAME = "EXIT_GREEDY"
EXPLORE_THEN_EXIT_POLICY_NAME = "EXPLORE_THEN_EXIT"
OBJECT_GREEDY_POLICY_NAME = "OBJECT_GREEDY"
_OBJECT_GREEDY_PRIORITY = {
    SrsObjectType.RESOURCE_CACHE: 0,
    SrsObjectType.STATION: 1,
    SrsObjectType.SALVAGE: 2,
}
_EXPLORE_THEN_EXIT_MIN_UNKNOWN_FRONTIER_COUNT = 1
_EXPLORE_THEN_EXIT_MAX_EXPLORE_STEPS = 12
DEFAULT_MAX_SRS_TURN = 50
DEFAULT_MAX_COMMANDS = 50
_VALUE_OBJECT_TYPES = frozenset(
    {
        SrsObjectType.RESOURCE_CACHE,
        SrsObjectType.STATION,
        SrsObjectType.SALVAGE,
    }
)
POLICY_RUNS_CSV_FIELDNAMES = [
    "case_id",
    "policy",
    "sector_type",
    "sector_seed",
    "entry_edge",
    "selected_exit_edge",
    "cost_mode",
    "outcome",
    "srs_turn_count",
    "command_count",
    "final_fuel",
    "max_fuel",
    "objects_discovered",
    "objects_acquired",
    "station_used",
    "resource_used",
    "salvage_acquired",
    "blocked_edge_attempt_count",
    "observation_5x5_count",
    "observation_3x3_count",
]
_POLICY_SUMMARY_METRIC_KEYS = (
    "run_count_by_policy",
    "run_count_by_cost_mode",
    "run_count_by_sector_type",
    "run_count_by_outcome",
    "exit_rate",
    "median_srs_turn_count",
    "p90_srs_turn_count",
    "object_discovery_rate",
    "object_acquisition_rate",
    "station_use_rate",
    "resource_use_rate",
    "salvage_acquisition_rate",
    "blocked_edge_attempt_rate",
    "turn_limit_rate",
    "no_policy_action_rate",
    "turn_only_exit_rate",
    "shared_fuel_exit_rate",
    "turn_only_vs_shared_fuel_failure_delta",
)
POLICY_NAME_ORDER = (
    EXIT_GREEDY_POLICY_NAME,
    EXPLORE_THEN_EXIT_POLICY_NAME,
    OBJECT_GREEDY_POLICY_NAME,
)
REPO_ROOT = Path(__file__).resolve().parents[3]


def _freeze_mapping(mapping: Mapping[str, Any]) -> Mapping[str, Any]:
    return MappingProxyType(dict(mapping))


def iter_known_cardinal_neighbors(position: Position) -> tuple[tuple[Direction, Position], ...]:
    return tuple(
        (direction, _step_position(position, direction))
        for direction in _DIRECTION_ORDER
    )


def is_known_passable_cell(
    position: Position,
    *,
    known_cells: Mapping[Position, SrsCell],
    objects: Mapping[str, Any],
) -> bool:
    cell = known_cells.get(position)
    if cell is None:
        return False
    if cell.terrain in _KNOWN_IMPASSABLE_TERRAINS:
        return False
    if cell.object_id is None:
        return True

    object_state = objects.get(cell.object_id)
    if object_state is None:
        return False
    return object_state.object_type not in _KNOWN_IMPASSABLE_OBJECT_TYPES


def route_on_known_cells(
    start: Position,
    target: Position,
    *,
    known_cells: Mapping[Position, SrsCell],
    objects: Mapping[str, Any],
) -> tuple[Direction, ...] | None:
    if not is_known_passable_cell(start, known_cells=known_cells, objects=objects):
        return None
    if not is_known_passable_cell(target, known_cells=known_cells, objects=objects):
        return None
    if start == target:
        return ()

    came_from: dict[Position, tuple[Position, Direction] | None] = {start: None}
    frontier: deque[Position] = deque([start])

    while frontier:
        current = frontier.popleft()
        for direction, next_position in iter_known_cardinal_neighbors(current):
            if next_position in came_from:
                continue
            if not is_known_passable_cell(next_position, known_cells=known_cells, objects=objects):
                continue
            came_from[next_position] = (current, direction)
            if next_position == target:
                return _reconstruct_route(came_from, start=start, target=target)
            frontier.append(next_position)

    return None


def first_known_route_step(
    start: Position,
    target: Position,
    *,
    known_cells: Mapping[Position, SrsCell],
    objects: Mapping[str, Any],
) -> Direction | None:
    route = route_on_known_cells(start, target, known_cells=known_cells, objects=objects)
    if route is None or not route:
        return None
    return route[0]


def choose_known_target_step(
    start: Position,
    targets: Iterable[Position],
    *,
    known_cells: Mapping[Position, SrsCell],
    objects: Mapping[str, Any],
) -> tuple[Position, Direction] | None:
    choice = _choose_known_target_route(
        start,
        targets,
        known_cells=known_cells,
        objects=objects,
    )
    if choice is None:
        return None
    return choice[0], choice[2][0]


def _choose_known_target_route(
    start: Position,
    targets: Iterable[Position],
    *,
    known_cells: Mapping[Position, SrsCell],
    objects: Mapping[str, Any],
) -> tuple[Position, int, tuple[Direction, ...]] | None:
    best_choice: tuple[int, int, int, int, Position, tuple[Direction, ...]] | None = None
    for target in targets:
        route = route_on_known_cells(start, target, known_cells=known_cells, objects=objects)
        if route is None or not route:
            continue
        choice = (
            len(route),
            target.y,
            target.x,
            _DIRECTION_ORDER.index(route[0]),
            target,
            route,
        )
        if best_choice is None or choice < best_choice:
            best_choice = choice

    if best_choice is None:
        return None
    return best_choice[4], best_choice[0], best_choice[5]


def choose_exit_greedy_command(
    state: SrsGameState,
    *,
    selected_exit_edge: Direction,
) -> SrsCommand | None:
    selected_exit_edge = Direction(selected_exit_edge)
    known_cells = state.known_state.known_cells
    current_cell = known_cells.get(state.player_position)
    if current_cell is not None and selected_exit_edge in current_cell.warp_flags:
        return SrsCommand(
            command_type="WARP_EXIT",
            exit_direction=selected_exit_edge,
        )

    targets = tuple(
        position
        for position, cell in known_cells.items()
        if selected_exit_edge in cell.warp_flags
    )
    if not targets:
        return None

    choice = choose_known_target_step(
        state.player_position,
        targets,
        known_cells=known_cells,
        objects=state.objects,
    )
    if choice is None:
        return None

    _, first_step = choice
    return SrsCommand(
        command_type="MOVE_ROUTE",
        route=(first_step,),
    )


def choose_explore_then_exit_command(
    state: SrsGameState,
    *,
    selected_exit_edge: Direction,
) -> SrsCommand | None:
    selected_exit_edge = Direction(selected_exit_edge)
    frontier_candidates = _build_unknown_frontier_candidates(state)
    if (
        state.srs_turn >= _EXPLORE_THEN_EXIT_MAX_EXPLORE_STEPS
        or len(frontier_candidates) < _EXPLORE_THEN_EXIT_MIN_UNKNOWN_FRONTIER_COUNT
    ):
        return choose_exit_greedy_command(state, selected_exit_edge=selected_exit_edge)

    if state.player_position in {candidate[0] for candidate in frontier_candidates}:
        direction = _choose_unknown_frontier_step(
            state.player_position,
            known_cells=state.known_state.known_cells,
            map_width=state.actual_map.width,
            map_height=state.actual_map.height,
            selected_exit_edge=selected_exit_edge,
        )
        if direction is not None:
            return SrsCommand(command_type="MOVE_ROUTE", route=(direction,))
        return choose_exit_greedy_command(state, selected_exit_edge=selected_exit_edge)

    best_choice: tuple[int, int, int, int, tuple[Direction, ...]] | None = None
    for position, unknown_neighbor_count in frontier_candidates:
        route = route_on_known_cells(
            state.player_position,
            position,
            known_cells=state.known_state.known_cells,
            objects=state.objects,
        )
        if route is None or not route:
            continue
        choice = (
            -unknown_neighbor_count,
            len(route),
            position.y,
            position.x,
            route,
        )
        if best_choice is None or choice < best_choice:
            best_choice = choice

    if best_choice is None:
        return choose_exit_greedy_command(state, selected_exit_edge=selected_exit_edge)

    return SrsCommand(command_type="MOVE_ROUTE", route=(best_choice[4][0],))


def _build_unknown_frontier_candidates(
    state: SrsGameState,
) -> tuple[tuple[Position, int], ...]:
    candidates: list[tuple[Position, int]] = []
    known_cells = state.known_state.known_cells
    for position in known_cells:
        if not is_known_passable_cell(position, known_cells=known_cells, objects=state.objects):
            continue
        unknown_neighbor_count = _count_unknown_cardinal_neighbors(
            position,
            known_cells=known_cells,
            map_width=state.actual_map.width,
            map_height=state.actual_map.height,
        )
        if unknown_neighbor_count > 0:
            candidates.append((position, unknown_neighbor_count))
    candidates.sort(key=lambda candidate: (-candidate[1], candidate[0].y, candidate[0].x))
    return tuple(candidates)


def _count_unknown_cardinal_neighbors(
    position: Position,
    *,
    known_cells: Mapping[Position, SrsCell],
    map_width: int,
    map_height: int,
) -> int:
    return sum(
        1
        for _, neighbor in iter_known_cardinal_neighbors(position)
        if _is_within_map_bounds(neighbor, map_width=map_width, map_height=map_height) and neighbor not in known_cells
    )


def _choose_unknown_frontier_step(
    position: Position,
    *,
    known_cells: Mapping[Position, SrsCell],
    map_width: int,
    map_height: int,
    selected_exit_edge: Direction,
) -> Direction | None:
    preferred_direction = _selected_exit_preferred_direction(selected_exit_edge)
    candidates = [
        direction
        for direction, neighbor in iter_known_cardinal_neighbors(position)
        if _is_within_map_bounds(neighbor, map_width=map_width, map_height=map_height) and neighbor not in known_cells
    ]
    if not candidates:
        return None
    return min(
        candidates,
        key=lambda direction: (
            0 if direction is preferred_direction else 1,
            _DIRECTION_ORDER.index(direction),
        ),
    )


def _selected_exit_preferred_direction(selected_exit_edge: Direction) -> Direction:
    return {
        Direction.N: Direction.N,
        Direction.E: Direction.E,
        Direction.S: Direction.S,
        Direction.W: Direction.W,
    }[selected_exit_edge]


def _is_within_map_bounds(
    position: Position,
    *,
    map_width: int,
    map_height: int,
) -> bool:
    return 0 <= position.x < map_width and 0 <= position.y < map_height


@dataclass(frozen=True, slots=True)
class ObjectGreedyCandidate:
    object_id: str
    object_type: SrsObjectType
    object_position: Position
    interaction_positions: tuple[Position, ...]


def _known_interaction_positions_for_object(
    object_state: SrsObjectState,
    *,
    known_cells: Mapping[Position, SrsCell],
    objects: Mapping[str, Any],
    contracts: SrsContracts,
) -> tuple[Position, ...]:
    contract = contracts.movement.interaction.get(object_state.object_type.value)
    if not isinstance(contract, Mapping):
        raise EvaluationCaseError(f"interaction contract missing for {object_state.object_type.value}")
    interaction_range = contract.get("range")
    if interaction_range == "SAME_CELL":
        if is_known_passable_cell(object_state.position, known_cells=known_cells, objects=objects):
            return (object_state.position,)
        return ()
    if interaction_range == "ADJACENT":
        return tuple(
            position
            for _, position in iter_known_cardinal_neighbors(object_state.position)
            if is_known_passable_cell(position, known_cells=known_cells, objects=objects)
        )
    raise EvaluationCaseError(f"unsupported interaction range: {interaction_range}")


def _is_object_greedy_candidate(
    object_state: SrsObjectState,
    *,
    fuel: int,
    max_fuel: int,
    rejected_object_ids: frozenset[str],
) -> bool:
    if object_state.object_id in rejected_object_ids:
        return False
    if object_state.object_type is SrsObjectType.RESOURCE_CACHE:
        return not object_state.consumed and fuel != max_fuel
    if object_state.object_type is SrsObjectType.STATION:
        return not object_state.activated and fuel != max_fuel
    if object_state.object_type is SrsObjectType.SALVAGE:
        return not object_state.consumed
    return False


def build_object_greedy_candidates(
    state: SrsGameState,
    *,
    contracts: SrsContracts,
    rejected_object_ids: Iterable[str] = (),
) -> tuple[ObjectGreedyCandidate, ...]:
    rejected = frozenset(rejected_object_ids)
    candidates: list[ObjectGreedyCandidate] = []
    for position, cell in state.known_state.known_cells.items():
        object_id = cell.object_id
        if object_id is None:
            continue
        object_state = state.objects.get(object_id)
        if object_state is None:
            continue
        if position != object_state.position:
            continue
        if object_state.object_type not in _OBJECT_GREEDY_PRIORITY:
            continue
        if not _is_object_greedy_candidate(
            object_state,
            fuel=state.fuel,
            max_fuel=state.max_fuel,
            rejected_object_ids=rejected,
        ):
            continue
        interaction_positions = _known_interaction_positions_for_object(
            object_state,
            known_cells=state.known_state.known_cells,
            objects=state.objects,
            contracts=contracts,
        )
        if not interaction_positions:
            continue
        candidates.append(
            ObjectGreedyCandidate(
                object_id=object_id,
                object_type=object_state.object_type,
                object_position=object_state.position,
                interaction_positions=interaction_positions,
            )
        )
    return tuple(candidates)


def choose_object_greedy_command(
    state: SrsGameState,
    *,
    contracts: SrsContracts,
    selected_exit_edge: Direction,
    rejected_object_ids: Iterable[str] = (),
) -> SrsCommand | None:
    best_choice: tuple[int, int, int, int, str, ObjectGreedyCandidate, tuple[Direction, ...]] | None = None
    for candidate in build_object_greedy_candidates(
        state,
        contracts=contracts,
        rejected_object_ids=rejected_object_ids,
    ):
        if state.player_position in candidate.interaction_positions:
            route: tuple[Direction, ...] = ()
        else:
            route_choice = _choose_known_target_route(
                state.player_position,
                candidate.interaction_positions,
                known_cells=state.known_state.known_cells,
                objects=state.objects,
            )
            if route_choice is None:
                continue
            _, _, route = route_choice
        choice = (
            _OBJECT_GREEDY_PRIORITY[candidate.object_type],
            len(route),
            candidate.object_position.y,
            candidate.object_position.x,
            candidate.object_id,
            candidate,
            route,
        )
        if best_choice is None or choice < best_choice:
            best_choice = choice

    if best_choice is None:
        return choose_exit_greedy_command(state, selected_exit_edge=selected_exit_edge)

    candidate = best_choice[5]
    route = best_choice[6]
    if not route:
        return SrsCommand(command_type="INTERACT", target_object_id=candidate.object_id)
    return SrsCommand(command_type="MOVE_ROUTE", route=(route[0],))


@dataclass(frozen=True, slots=True)
class EvaluationCase:
    case_id: str
    sector_id: str
    sector_type: SectorType
    sector_seed: int
    entry_edge: Direction
    blocked_edges: frozenset[Direction]
    selected_exit_edge: Direction
    cost_mode: CostMode
    initial_fuel: int
    max_fuel: int
    initial_reveal_mode: InitialRevealMode
    revisit_mode: RevisitMode
    initial_player_position: Position | None = None

    def __post_init__(self) -> None:
        try:
            sector_type = SectorType(self.sector_type)
            entry_edge = Direction(self.entry_edge)
            blocked_edges = frozenset(Direction(edge) for edge in self.blocked_edges)
            selected_exit_edge = Direction(self.selected_exit_edge)
            cost_mode = CostMode(self.cost_mode)
            initial_reveal_mode = InitialRevealMode(self.initial_reveal_mode)
            revisit_mode = RevisitMode(self.revisit_mode)
        except ValueError as exc:
            raise EvaluationCaseError(str(exc)) from exc

        object.__setattr__(self, "sector_type", sector_type)
        object.__setattr__(self, "entry_edge", entry_edge)
        object.__setattr__(self, "blocked_edges", blocked_edges)
        object.__setattr__(self, "selected_exit_edge", selected_exit_edge)
        object.__setattr__(self, "cost_mode", cost_mode)
        object.__setattr__(self, "initial_reveal_mode", initial_reveal_mode)
        object.__setattr__(self, "revisit_mode", revisit_mode)
        self._validate()

    def build_sector_descriptor(self) -> SectorDescriptor:
        return SectorDescriptor(
            sector_id=self.sector_id,
            sector_type=self.sector_type,
            sector_seed=self.sector_seed,
            entry_edge=self.entry_edge,
            blocked_edges=self.blocked_edges,
        )

    def build_initial_state(self, *, contracts: SrsContracts) -> SrsGameState:
        state = create_sector(self.build_sector_descriptor(), contracts=contracts)
        if self.initial_player_position is not None:
            state = replace(state, player_position=self.initial_player_position)
        state = _apply_initial_reveal(state, case=self, contracts=contracts)
        state = _apply_revisit_mode(state, case=self)
        return replace(state, fuel=self.initial_fuel, max_fuel=self.max_fuel)

    def metadata(self) -> Mapping[str, Any]:
        return _freeze_mapping(
            {
                "case_id": self.case_id,
                "sector_id": self.sector_id,
                "sector_type": self.sector_type.value,
                "sector_seed": self.sector_seed,
                "entry_edge": self.entry_edge.value,
                "blocked_edges": [edge.value for edge in _sorted_directions(self.blocked_edges)],
                "selected_exit_edge": self.selected_exit_edge.value,
                "cost_mode": self.cost_mode.value,
                "initial_fuel": self.initial_fuel,
                "max_fuel": self.max_fuel,
                "initial_reveal_mode": self.initial_reveal_mode.value,
                "revisit_mode": self.revisit_mode.value,
                **(
                    {}
                    if self.initial_player_position is None
                    else {
                        "initial_player_position": [
                            self.initial_player_position.x,
                            self.initial_player_position.y,
                        ]
                    }
                ),
            }
        )

    def open_exit_edges(self) -> tuple[Direction, ...]:
        return tuple(
            direction
            for direction in _DIRECTION_ORDER
            if direction not in self.blocked_edges
        )

    def _validate(self) -> None:
        if not self.case_id or self.case_id.strip() != self.case_id:
            raise EvaluationCaseError("case_id must be a non-empty stable string")
        if not self.sector_id or self.sector_id.strip() != self.sector_id:
            raise EvaluationCaseError("sector_id must be a non-empty stable string")
        if "/" in self.case_id or "\\" in self.case_id:
            raise EvaluationCaseError("case_id must not contain path separators")
        if "/" in self.sector_id or "\\" in self.sector_id:
            raise EvaluationCaseError("sector_id must not contain path separators")
        if self.selected_exit_edge in self.blocked_edges:
            raise EvaluationCaseError("selected_exit_edge must not be included in blocked_edges")
        if self.initial_fuel < 0:
            raise EvaluationCaseError("initial_fuel must be non-negative")
        if self.max_fuel < 0:
            raise EvaluationCaseError("max_fuel must be non-negative")
        if self.initial_fuel > self.max_fuel:
            raise EvaluationCaseError("initial_fuel must be less than or equal to max_fuel")
        if self.selected_exit_edge not in self.open_exit_edges():
            raise EvaluationCaseError("selected_exit_edge must be an open exit edge")
        if self.initial_player_position is not None:
            if self.initial_player_position.x < 0 or self.initial_player_position.y < 0:
                raise EvaluationCaseError("initial_player_position must be within the generated map")
            if self.initial_player_position.x >= 9 or self.initial_player_position.y >= 9:
                raise EvaluationCaseError("initial_player_position must be within the generated map")
        try:
            validate_sector_descriptor(self.build_sector_descriptor())
        except SrsModelError as exc:
            raise EvaluationCaseError(str(exc)) from exc


def build_default_evaluation_cases() -> tuple[EvaluationCase, ...]:
    return (
        EvaluationCase(
            case_id="normal-turn-only-first-visit",
            sector_id="normal-1001",
            sector_type=SectorType.NORMAL,
            sector_seed=1001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.N,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=0,
            max_fuel=0,
            initial_reveal_mode=InitialRevealMode.LOCAL_MOVEMENT,
            revisit_mode=RevisitMode.FIRST_VISIT,
        ),
        EvaluationCase(
            case_id="normal-shared-fuel-first-visit",
            sector_id="normal-1002",
            sector_type=SectorType.NORMAL,
            sector_seed=1002,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.E,
            cost_mode=CostMode.SHARED_FUEL,
            initial_fuel=6,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.LOCAL_MOVEMENT,
            revisit_mode=RevisitMode.FIRST_VISIT,
        ),
        EvaluationCase(
            case_id="resource-cache-first-visit",
            sector_id="resource-3001",
            sector_type=SectorType.RESOURCE,
            sector_seed=3001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.N,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=2,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.FULL,
            revisit_mode=RevisitMode.FIRST_VISIT,
        ),
        EvaluationCase(
            case_id="base-station-first-visit",
            sector_id="base-2001",
            sector_type=SectorType.BASE,
            sector_seed=2001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.E,
            cost_mode=CostMode.SHARED_FUEL,
            initial_fuel=3,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.FULL,
            revisit_mode=RevisitMode.FIRST_VISIT,
        ),
        EvaluationCase(
            case_id="salvage-placeholder-first-visit",
            sector_id="rift-4001",
            sector_type=SectorType.RIFT,
            sector_seed=4001,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N}),
            selected_exit_edge=Direction.E,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=1,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.FULL,
            revisit_mode=RevisitMode.FIRST_VISIT,
        ),
        EvaluationCase(
            case_id="normal-turn-only-revisit",
            sector_id="normal-1003",
            sector_type=SectorType.NORMAL,
            sector_seed=1003,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.W,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=0,
            max_fuel=0,
            initial_reveal_mode=InitialRevealMode.LOCAL_MOVEMENT,
            revisit_mode=RevisitMode.REVISIT_PRESERVE_DISCOVERY,
        ),
        EvaluationCase(
            case_id="resource-cache-revisit",
            sector_id="resource-3002",
            sector_type=SectorType.RESOURCE,
            sector_seed=3002,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.W,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=2,
            max_fuel=9,
            initial_reveal_mode=InitialRevealMode.FULL,
            revisit_mode=RevisitMode.REVISIT_AFTER_PRIMARY_INTERACTION,
        ),
        EvaluationCase(
            case_id="nebula-local-3x3-first-visit",
            sector_id="nebula-5001",
            sector_type=SectorType.NEBULA,
            sector_seed=5001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.N,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=0,
            max_fuel=0,
            initial_reveal_mode=InitialRevealMode.LOCAL_MOVEMENT,
            revisit_mode=RevisitMode.FIRST_VISIT,
            initial_player_position=Position(4, 4),
        ),
    )


class PolicyRunOutcome(str, Enum):
    EXITED = "EXITED"
    ABORTED_TURN_LIMIT = "ABORTED_TURN_LIMIT"
    ABORTED_NO_POLICY_ACTION = "ABORTED_NO_POLICY_ACTION"
    RESOURCE_DEPLETED = "RESOURCE_DEPLETED"
    GENERATION_ERROR = "GENERATION_ERROR"


@dataclass(frozen=True, slots=True)
class PolicyRunResult:
    evaluation_case: EvaluationCase
    policy_name: str
    outcome: PolicyRunOutcome
    final_state: SrsGameState | None
    command_count: int
    event_log: tuple[SrsTurnEvent, ...]
    action_sequence: tuple[SrsCommand, ...]


def summarize_policy_runs(policy_runs: Iterable[PolicyRunResult]) -> dict[str, Any]:
    runs = tuple(policy_runs)
    if not runs:
        raise ValueError("policy_runs must not be empty")

    turn_only_exit_rate = _exit_rate_for_cost_mode(runs, CostMode.TURN_ONLY)
    shared_fuel_exit_rate = _exit_rate_for_cost_mode(runs, CostMode.SHARED_FUEL)

    summary = _build_summary_metrics(runs)
    summary["run_count_by_policy"] = _count_by_group(
        runs,
        key_fn=lambda run: run.policy_name,
        preferred_keys=tuple(_POLICY_COMMAND_GENERATORS),
    )
    summary["run_count_by_cost_mode"] = _count_by_group(
        runs,
        key_fn=lambda run: run.evaluation_case.cost_mode.value,
        preferred_keys=tuple(cost_mode.value for cost_mode in CostMode),
    )
    summary["run_count_by_sector_type"] = _count_by_group(
        runs,
        key_fn=lambda run: run.evaluation_case.sector_type.value,
        preferred_keys=tuple(sector_type.value for sector_type in SectorType),
    )
    summary["run_count_by_outcome"] = _count_by_group(
        runs,
        key_fn=lambda run: run.outcome.value,
        preferred_keys=tuple(outcome.value for outcome in PolicyRunOutcome),
    )
    summary["turn_only_exit_rate"] = turn_only_exit_rate
    summary["shared_fuel_exit_rate"] = shared_fuel_exit_rate
    summary["turn_only_vs_shared_fuel_failure_delta"] = _failure_rate_delta(
        turn_only_exit_rate=turn_only_exit_rate,
        shared_fuel_exit_rate=shared_fuel_exit_rate,
    )
    summary["by_policy"] = _summaries_by_group(
        runs,
        key_fn=lambda run: run.policy_name,
        preferred_keys=tuple(_POLICY_COMMAND_GENERATORS),
    )
    summary["by_cost_mode"] = _summaries_by_group(
        runs,
        key_fn=lambda run: run.evaluation_case.cost_mode.value,
        preferred_keys=tuple(cost_mode.value for cost_mode in CostMode),
    )
    summary["by_sector_type"] = _summaries_by_group(
        runs,
        key_fn=lambda run: run.evaluation_case.sector_type.value,
        preferred_keys=tuple(sector_type.value for sector_type in SectorType),
    )
    summary["by_outcome"] = _summaries_by_group(
        runs,
        key_fn=lambda run: run.outcome.value,
        preferred_keys=tuple(outcome.value for outcome in PolicyRunOutcome),
    )
    return summary


def build_policy_summary_document(policy_runs: Iterable[PolicyRunResult]) -> dict[str, Any]:
    summary = summarize_policy_runs(policy_runs)
    return {
        "run_count": summary["run_count"],
        "policies": summary["by_policy"],
        "conditions": {
            "cost_modes": summary["by_cost_mode"],
            "sector_types": summary["by_sector_type"],
            "outcomes": summary["by_outcome"],
        },
        "metrics": {
            key: summary[key]
            for key in _POLICY_SUMMARY_METRIC_KEYS
        },
    }


def write_policy_runs_csv(output_path: Path, policy_runs: Iterable[PolicyRunResult]) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    ordered_runs = sorted(policy_runs, key=_policy_run_csv_sort_key)
    with output_path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=POLICY_RUNS_CSV_FIELDNAMES)
        writer.writeheader()
        writer.writerows(_policy_run_csv_row(run) for run in ordered_runs)


def write_policy_summary_json(summary_path: Path, policy_runs: Iterable[PolicyRunResult]) -> None:
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    payload = build_policy_summary_document(policy_runs)
    summary_path.write_text(json.dumps(payload, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-runs",
        type=Path,
        required=True,
        help="CSV file path for per-run policy evaluation results.",
    )
    parser.add_argument(
        "--output-summary",
        type=Path,
        required=True,
        help="JSON file path for aggregated policy evaluation results.",
    )
    return parser.parse_args(argv)


def evaluate_default_policies(*, contracts: SrsContracts) -> tuple[PolicyRunResult, ...]:
    runs: list[PolicyRunResult] = []
    for evaluation_case in build_default_evaluation_cases():
        for policy_name in POLICY_NAME_ORDER:
            runs.append(
                run_policy_evaluation_case(
                    evaluation_case,
                    policy_name,
                    contracts=contracts,
                )
            )
    return tuple(runs)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        policy_runs = evaluate_default_policies(contracts=load_default_contracts(REPO_ROOT))
        write_policy_runs_csv(args.output_runs, policy_runs)
        write_policy_summary_json(args.output_summary, policy_runs)
    except (OSError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


def _policy_run_csv_sort_key(run: PolicyRunResult) -> tuple[Any, ...]:
    return (
        run.evaluation_case.case_id,
        run.policy_name,
        run.evaluation_case.cost_mode.value,
        run.evaluation_case.sector_seed,
        run.outcome.value,
        run.command_count,
    )


def _policy_run_csv_row(run: PolicyRunResult) -> dict[str, Any]:
    final_state = run.final_state
    objects_discovered = _discovered_value_object_ids(run)
    objects_acquired = _acquired_value_object_ids(run)
    observation_counts = _observation_counts(run)
    return {
        "case_id": run.evaluation_case.case_id,
        "policy": run.policy_name,
        "sector_type": run.evaluation_case.sector_type.value,
        "sector_seed": run.evaluation_case.sector_seed,
        "entry_edge": run.evaluation_case.entry_edge.value,
        "selected_exit_edge": run.evaluation_case.selected_exit_edge.value,
        "cost_mode": run.evaluation_case.cost_mode.value,
        "outcome": run.outcome.value,
        "srs_turn_count": None if final_state is None else final_state.srs_turn,
        "command_count": run.command_count,
        "final_fuel": None if final_state is None else final_state.fuel,
        "max_fuel": None if final_state is None else final_state.max_fuel,
        "objects_discovered": len(objects_discovered),
        "objects_acquired": len(objects_acquired),
        "station_used": int(_has_used_station(run)),
        "resource_used": int(_has_used_resource_cache(run)),
        "salvage_acquired": int(_has_acquired_salvage(run)),
        "blocked_edge_attempt_count": _blocked_edge_attempt_count(run),
        "observation_5x5_count": observation_counts[5],
        "observation_3x3_count": observation_counts[3],
    }


def _build_summary_metrics(runs: tuple[PolicyRunResult, ...]) -> dict[str, Any]:
    srs_turn_counts = sorted(
        run.final_state.srs_turn
        for run in runs
        if run.final_state is not None
    )
    run_count = len(runs)
    return {
        "run_count": run_count,
        "exit_rate": _rounded_ratio(_count_runs(runs, lambda run: run.outcome is PolicyRunOutcome.EXITED), run_count),
        "median_srs_turn_count": _median_or_none(srs_turn_counts),
        "p90_srs_turn_count": _p90_or_none(srs_turn_counts),
        "object_discovery_rate": _rounded_ratio(_count_runs(runs, _has_discovered_value_object), run_count),
        "object_acquisition_rate": _rounded_ratio(_count_runs(runs, _has_acquired_value_object), run_count),
        "station_use_rate": _rounded_ratio(_count_runs(runs, _has_used_station), run_count),
        "resource_use_rate": _rounded_ratio(_count_runs(runs, _has_used_resource_cache), run_count),
        "salvage_acquisition_rate": _rounded_ratio(_count_runs(runs, _has_acquired_salvage), run_count),
        "blocked_edge_attempt_rate": _rounded_ratio(_count_runs(runs, _attempted_blocked_edge), run_count),
        "turn_limit_rate": _rounded_ratio(
            _count_runs(runs, lambda run: run.outcome is PolicyRunOutcome.ABORTED_TURN_LIMIT),
            run_count,
        ),
        "no_policy_action_rate": _rounded_ratio(
            _count_runs(runs, lambda run: run.outcome is PolicyRunOutcome.ABORTED_NO_POLICY_ACTION),
            run_count,
        ),
    }


def _count_by_group(
    runs: tuple[PolicyRunResult, ...],
    *,
    key_fn: Callable[[PolicyRunResult], str],
    preferred_keys: tuple[str, ...],
) -> dict[str, int]:
    counts = {key: 0 for key in preferred_keys}
    for run in runs:
        key = key_fn(run)
        counts[key] = counts.get(key, 0) + 1
    ordered_keys = list(preferred_keys) + sorted(key for key in counts if key not in preferred_keys)
    return {key: counts[key] for key in ordered_keys if counts.get(key, 0) > 0 or key in preferred_keys}


def _summaries_by_group(
    runs: tuple[PolicyRunResult, ...],
    *,
    key_fn: Callable[[PolicyRunResult], str],
    preferred_keys: tuple[str, ...],
) -> dict[str, dict[str, Any]]:
    grouped: dict[str, list[PolicyRunResult]] = {}
    for run in runs:
        key = key_fn(run)
        grouped.setdefault(key, []).append(run)

    ordered_keys = [key for key in preferred_keys if key in grouped]
    ordered_keys.extend(sorted(key for key in grouped if key not in preferred_keys))
    return {
        key: _build_summary_metrics(tuple(grouped[key]))
        for key in ordered_keys
    }


def _count_runs(runs: Iterable[PolicyRunResult], predicate: Callable[[PolicyRunResult], bool]) -> int:
    return sum(1 for run in runs if predicate(run))


def _rounded_ratio(count: int, total: int) -> float:
    if total == 0:
        return 0.0
    return round(count / total, 6)


def _median_or_none(series: list[int]) -> float | int | None:
    if not series:
        return None
    return statistics.median(series)


def _p90_or_none(series: list[int]) -> int | None:
    if not series:
        return None
    return metrics.percentile_nearest_rank(series, 0.90)


def _exit_rate_for_cost_mode(
    runs: tuple[PolicyRunResult, ...],
    cost_mode: CostMode,
) -> float | None:
    matched_runs = tuple(run for run in runs if run.evaluation_case.cost_mode is cost_mode)
    if not matched_runs:
        return None
    return _rounded_ratio(
        _count_runs(matched_runs, lambda run: run.outcome is PolicyRunOutcome.EXITED),
        len(matched_runs),
    )


def _failure_rate_delta(
    *,
    turn_only_exit_rate: float | None,
    shared_fuel_exit_rate: float | None,
) -> float | None:
    if turn_only_exit_rate is None or shared_fuel_exit_rate is None:
        return None
    turn_only_failure_rate = 1.0 - turn_only_exit_rate
    shared_fuel_failure_rate = 1.0 - shared_fuel_exit_rate
    return round(turn_only_failure_rate - shared_fuel_failure_rate, 6)


def _has_discovered_value_object(run: PolicyRunResult) -> bool:
    return bool(_discovered_value_object_ids(run))


def _discovered_value_object_ids(run: PolicyRunResult) -> tuple[str, ...]:
    final_state = run.final_state
    if final_state is None:
        return ()
    return tuple(
        sorted(
            {
                object_state.object_id
                for cell in final_state.known_state.known_cells.values()
                for object_state in [None if cell.object_id is None else final_state.objects.get(cell.object_id)]
                if object_state is not None and object_state.object_type in _VALUE_OBJECT_TYPES
            }
        )
    )


def _acquired_value_object_ids(run: PolicyRunResult) -> tuple[str, ...]:
    final_state = run.final_state
    if final_state is None:
        return ()
    return tuple(
        sorted(
            {
                object_id
                for object_id in (
                    *final_state.persistent_state.consumed_object_ids,
                    *final_state.persistent_state.activated_object_ids,
                )
                if _object_matches_type(final_state, object_id, expected_types=_VALUE_OBJECT_TYPES)
            }
        )
    )


def _observation_counts(run: PolicyRunResult) -> dict[int, int]:
    counts = {3: 0, 5: 0}
    final_state = run.final_state
    if final_state is None:
        return counts
    for event in run.event_log:
        if event.event_type != OBSERVATION_UPDATED:
            continue
        center = event.payload.get("center")
        if not isinstance(center, list) or len(center) != 2:
            continue
        position = Position(center[0], center[1])
        terrain = final_state.actual_map.cell_at(position).terrain
        size = 3 if terrain is SrsTerrainType.NEBULA else 5
        counts[size] += 1
    return counts


def _blocked_edge_attempt_count(run: PolicyRunResult) -> int:
    return sum(
        1
        for event in run.event_log
        if event.event_type == WARP_EXIT_REJECTED and event.payload.get("outcome") == "REJECTED_BLOCKED_EDGE"
    )


def _object_matches_type(
    final_state: SrsGameState,
    object_id: str,
    *,
    expected_types: frozenset[SrsObjectType],
) -> bool:
    object_state = final_state.objects.get(object_id)
    return object_state is not None and object_state.object_type in expected_types


def _has_acquired_value_object(run: PolicyRunResult) -> bool:
    return bool(_acquired_value_object_ids(run))


def _has_used_station(run: PolicyRunResult) -> bool:
    return _persistent_object_match(
        run,
        object_ids=run.final_state.persistent_state.activated_object_ids if run.final_state else (),
        expected_type=SrsObjectType.STATION,
    )


def _has_used_resource_cache(run: PolicyRunResult) -> bool:
    return _persistent_object_match(
        run,
        object_ids=run.final_state.persistent_state.consumed_object_ids if run.final_state else (),
        expected_type=SrsObjectType.RESOURCE_CACHE,
    )


def _has_acquired_salvage(run: PolicyRunResult) -> bool:
    return _persistent_object_match(
        run,
        object_ids=run.final_state.persistent_state.consumed_object_ids if run.final_state else (),
        expected_type=SrsObjectType.SALVAGE,
    )


def _persistent_object_match(
    run: PolicyRunResult,
    *,
    object_ids: Iterable[str],
    expected_type: SrsObjectType | None,
) -> bool:
    final_state = run.final_state
    if final_state is None:
        return False
    for object_id in object_ids:
        object_state = final_state.objects.get(object_id)
        if object_state is None:
            continue
        if expected_type is None or object_state.object_type is expected_type:
            return True
    return False


def _attempted_blocked_edge(run: PolicyRunResult) -> bool:
    return _blocked_edge_attempt_count(run) > 0


_POLICY_COMMAND_GENERATORS: Mapping[str, Callable[..., SrsCommand | None]] = MappingProxyType(
    {
        EXIT_GREEDY_POLICY_NAME: choose_exit_greedy_command,
        EXPLORE_THEN_EXIT_POLICY_NAME: choose_explore_then_exit_command,
        OBJECT_GREEDY_POLICY_NAME: choose_object_greedy_command,
    }
)
_REJECTED_RUN_EVENT_TYPES = frozenset({MOVE_REJECTED, INTERACT_REJECTED, WARP_EXIT_REJECTED})


def run_policy_evaluation_case(
    evaluation_case: EvaluationCase,
    policy_name: str,
    *,
    contracts: SrsContracts,
    max_srs_turn: int = DEFAULT_MAX_SRS_TURN,
    max_commands: int = DEFAULT_MAX_COMMANDS,
) -> PolicyRunResult:
    if max_srs_turn < 0:
        raise ValueError("max_srs_turn must be non-negative")
    if max_commands < 0:
        raise ValueError("max_commands must be non-negative")

    try:
        current_state = evaluation_case.build_initial_state(contracts=contracts)
    except (EvaluationCaseError, SrsModelError, ValueError):
        return PolicyRunResult(
            evaluation_case=evaluation_case,
            policy_name=policy_name,
            outcome=PolicyRunOutcome.GENERATION_ERROR,
            final_state=None,
            command_count=0,
            event_log=(),
            action_sequence=(),
        )

    events: list[SrsTurnEvent] = []
    action_sequence: list[SrsCommand] = []
    rejected_commands: set[SrsCommand] = set()
    rejected_object_ids: set[str] = set()

    while True:
        if current_state.srs_turn >= max_srs_turn or len(action_sequence) >= max_commands:
            return _build_policy_run_result(
                evaluation_case=evaluation_case,
                policy_name=policy_name,
                outcome=PolicyRunOutcome.ABORTED_TURN_LIMIT,
                final_state=current_state,
                events=events,
                action_sequence=action_sequence,
            )

        try:
            command = choose_policy_command(
                current_state,
                policy_name=policy_name,
                contracts=contracts,
                selected_exit_edge=evaluation_case.selected_exit_edge,
                rejected_object_ids=rejected_object_ids,
            )
        except (EvaluationCaseError, SrsModelError, ValueError):
            return _build_policy_run_result(
                evaluation_case=evaluation_case,
                policy_name=policy_name,
                outcome=PolicyRunOutcome.GENERATION_ERROR,
                final_state=current_state,
                events=events,
                action_sequence=action_sequence,
            )

        if command is None or command in rejected_commands:
            return _build_policy_run_result(
                evaluation_case=evaluation_case,
                policy_name=policy_name,
                outcome=PolicyRunOutcome.ABORTED_NO_POLICY_ACTION,
                final_state=current_state,
                events=events,
                action_sequence=action_sequence,
            )

        result = apply_srs_command(
            current_state,
            command,
            contracts=contracts,
            cost_mode=evaluation_case.cost_mode,
        )
        current_state = result.state
        action_sequence.append(command)
        events.extend(result.events)

        outcome = _classify_run_outcome(result.events)
        if outcome is not None:
            return _build_policy_run_result(
                evaluation_case=evaluation_case,
                policy_name=policy_name,
                outcome=outcome,
                final_state=current_state,
                events=events,
                action_sequence=action_sequence,
            )

        if _command_was_rejected(result.events):
            rejected_commands.add(command)
            if command.command_type == "INTERACT" and command.target_object_id is not None:
                rejected_object_ids.add(command.target_object_id)


def choose_policy_command(
    state: SrsGameState,
    *,
    policy_name: str,
    contracts: SrsContracts,
    selected_exit_edge: Direction,
    rejected_object_ids: Iterable[str] = (),
) -> SrsCommand | None:
    generator = _POLICY_COMMAND_GENERATORS.get(policy_name)
    if generator is None:
        raise EvaluationCaseError(f"unsupported policy_name: {policy_name}")
    if policy_name == OBJECT_GREEDY_POLICY_NAME:
        return generator(
            state,
            contracts=contracts,
            selected_exit_edge=selected_exit_edge,
            rejected_object_ids=rejected_object_ids,
        )
    return generator(state, selected_exit_edge=selected_exit_edge)


def _build_policy_run_result(
    *,
    evaluation_case: EvaluationCase,
    policy_name: str,
    outcome: PolicyRunOutcome,
    final_state: SrsGameState | None,
    events: Iterable[SrsTurnEvent],
    action_sequence: Iterable[SrsCommand],
) -> PolicyRunResult:
    action_sequence = tuple(action_sequence)
    return PolicyRunResult(
        evaluation_case=evaluation_case,
        policy_name=policy_name,
        outcome=outcome,
        final_state=final_state,
        command_count=len(action_sequence),
        event_log=tuple(events),
        action_sequence=action_sequence,
    )


def _classify_run_outcome(events: Iterable[SrsTurnEvent]) -> PolicyRunOutcome | None:
    event_types = {event.event_type for event in events}
    if WARP_EXIT_ACCEPTED in event_types:
        return PolicyRunOutcome.EXITED
    if _contains_resource_depleted_event(events):
        return PolicyRunOutcome.RESOURCE_DEPLETED
    return None


def _contains_resource_depleted_event(events: Iterable[SrsTurnEvent]) -> bool:
    return any(
        event.payload.get("outcome") == PolicyRunOutcome.RESOURCE_DEPLETED.value
        for event in events
    )


def _command_was_rejected(events: Iterable[SrsTurnEvent]) -> bool:
    for event in events:
        return event.event_type in _REJECTED_RUN_EVENT_TYPES
    return False


def _apply_initial_reveal(
    state: SrsGameState,
    *,
    case: EvaluationCase,
    contracts: SrsContracts,
) -> SrsGameState:
    if case.initial_reveal_mode is InitialRevealMode.NONE:
        return state
    if case.initial_reveal_mode is InitialRevealMode.FULL:
        return reveal_full_observation(state)
    if case.initial_reveal_mode is InitialRevealMode.LOCAL_MOVEMENT:
        if case.sector_type is SectorType.NEBULA:
            state = _replace_player_cell_terrain(state, terrain=SrsTerrainType.NEBULA)
        return reveal_observation(state, center=state.player_position, contracts=contracts)
    raise EvaluationCaseError(f"unsupported initial_reveal_mode: {case.initial_reveal_mode.value}")


def _apply_revisit_mode(
    state: SrsGameState,
    *,
    case: EvaluationCase,
) -> SrsGameState:
    if case.revisit_mode is RevisitMode.FIRST_VISIT:
        return state

    consumed_object_ids: frozenset[str] = frozenset()
    activated_object_ids: frozenset[str] = frozenset()
    if case.revisit_mode is RevisitMode.REVISIT_AFTER_PRIMARY_INTERACTION:
        consumed_object_ids = frozenset(
            object_id
            for object_id, object_state in state.objects.items()
            if object_state.object_type in _REVISIT_CONSUMED_OBJECT_TYPES
        )
        activated_object_ids = frozenset(
            object_id
            for object_id, object_state in state.objects.items()
            if object_state.object_type is SrsObjectType.STATION
        )
    elif case.revisit_mode is not RevisitMode.REVISIT_PRESERVE_DISCOVERY:
        raise EvaluationCaseError(f"unsupported revisit_mode: {case.revisit_mode.value}")

    persistent = replace(
        state.persistent_state,
        consumed_object_ids=consumed_object_ids,
        activated_object_ids=activated_object_ids,
        discovered_cells=state.known_state.discovered_cells,
    )
    return restore_srs_state(
        descriptor=state.descriptor,
        actual_map=state.actual_map,
        persistent=persistent,
        player_position=state.player_position,
        objects=state.objects,
    )


def _replace_player_cell_terrain(
    state: SrsGameState,
    *,
    terrain: SrsTerrainType,
) -> SrsGameState:
    rows = [list(row) for row in state.actual_map.cells]
    position = state.player_position
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=terrain,
        object_id=current.object_id,
        actor_id=current.actor_id,
        warp_flags=current.warp_flags,
    )
    actual_map = replace(
        state.actual_map,
        cells=tuple(tuple(row) for row in rows),
    )
    return replace(state, actual_map=actual_map)


def _sorted_directions(directions: frozenset[Direction]) -> tuple[Direction, ...]:
    return tuple(direction for direction in _DIRECTION_ORDER if direction in directions)


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
            raise EvaluationCaseError(f"known-state route reconstruction failed: {target}")
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


if __name__ == "__main__":
    raise SystemExit(main())
