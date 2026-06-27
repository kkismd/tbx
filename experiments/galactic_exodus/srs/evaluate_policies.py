from __future__ import annotations

from collections import deque
from dataclasses import dataclass, replace
from enum import Enum
from types import MappingProxyType
from typing import Any, Iterable, Mapping

from experiments.galactic_exodus.srs.contracts import SrsContracts
from experiments.galactic_exodus.srs.engine import restore_srs_state, reveal_full_observation, reveal_observation
from experiments.galactic_exodus.srs.generate import create_sector
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
OBJECT_GREEDY_POLICY_NAME = "OBJECT_GREEDY"
_OBJECT_GREEDY_PRIORITY = {
    SrsObjectType.RESOURCE_CACHE: 0,
    SrsObjectType.STATION: 1,
    SrsObjectType.SALVAGE: 2,
}


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
