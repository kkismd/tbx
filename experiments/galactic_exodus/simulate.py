#!/usr/bin/env python3

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import heapq
import random
from typing import Callable, TypeAlias


WIDTH = 8
HEIGHT = 8
DEFAULT_SEED = 42
DEFAULT_RESOURCE_COUNT = 3
DEFAULT_RIFT_DENSITY = 0.10
DEFAULT_INITIAL_FUEL = 16
DEFAULT_BASE_SUPPLY = 8
DEFAULT_RESOURCE_SUPPLY = 5
TOTAL_UNDIRECTED_EDGES = WIDTH * (HEIGHT - 1) + HEIGHT * (WIDTH - 1)

SPECIAL_S = (1, 1)
SPECIAL_H = (8, 8)
CENTRAL_B_CANDIDATES = [(4, 4), (5, 4), (4, 5), (5, 5)]
TERRAIN_SYMBOLS = [".", "N", "A", "@"]
TERRAIN_WEIGHTS = [0.60, 0.20, 0.12, 0.08]
TERRAIN_COSTS = {
    ".": 1,
    "N": 2,
    "A": 3,
    "@": 2,
    "B": 1,
    "R": 1,
    "S": 0,
    "H": 1,
}

Position: TypeAlias = tuple[int, int]
Cells: TypeAlias = dict[Position, str]
Edge: TypeAlias = tuple[Position, Position]
CostFunction: TypeAlias = Callable[[str], int]


@dataclass(frozen=True)
class GalacticMap:
    seed: int
    resource_count: int
    rift_density: float
    b_position: Position
    r_positions: list[Position]
    rift_edges: tuple[Edge, ...]
    cells: Cells


@dataclass(frozen=True)
class PathResult:
    cost: int
    steps: int


@dataclass(frozen=True)
class PathAnalysis:
    reachable: bool
    best_cost: int | None
    best_path_length: int | None
    cost_to_base: int | None
    cost_base_to_goal: int | None
    best_cost_via_base: int | None
    best_cost_without_base: int | None
    base_route_advantage_raw: int | None
    base_is_mandatory: bool


@dataclass(frozen=True)
class CostContributionAnalysis:
    plain_cost: int | None
    terrain_only_cost: int | None
    full_cost: int | None
    terrain_extra_cost: int | None
    rift_detour_cost: int | None


@dataclass(frozen=True)
class FuelAnalysis:
    fuel_feasible_direct: bool
    fuel_feasible_via_base: bool
    fuel_feasible_via_resource: bool
    remaining_fuel_direct: int | None
    remaining_fuel_via_base: int | None
    remaining_fuel_via_resource: int | None
    remaining_fuel_at_goal: int | None
    required_supply: int | None
    best_cost_via_resource: int | None
    best_resource_position: Position | None


@dataclass(frozen=True)
class LegCost:
    position: Position
    cost_to_stop: int
    cost_to_goal: int

    @property
    def total_cost(self) -> int:
        return self.cost_to_stop + self.cost_to_goal


VERDICT_REJECT_TOO_HARD = "REJECT_TOO_HARD"
VERDICT_REJECT_BASE_MANDATORY = "REJECT_BASE_MANDATORY"
VERDICT_ACCEPT = "ACCEPT"
VERDICT_PRIORITY_ORDER = (
    VERDICT_REJECT_TOO_HARD,
    VERDICT_REJECT_BASE_MANDATORY,
    VERDICT_ACCEPT,
)
ACCEPT_NOTE = "ACCEPT is a minimal candidate verdict, not a final fun/balance judgment."


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate a deterministic 8x8 Galactic Exodus map.")
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED, help="Random seed (default: 42).")
    parser.add_argument(
        "--resource-count",
        type=int,
        default=DEFAULT_RESOURCE_COUNT,
        help="Number of resource objects to place (default: 3).",
    )
    parser.add_argument(
        "--rift-density",
        type=float,
        default=DEFAULT_RIFT_DENSITY,
        help="Density of impassable fault-line edges (default: 0.10).",
    )
    parser.add_argument(
        "--initial-fuel",
        type=int,
        default=DEFAULT_INITIAL_FUEL,
        help="Initial fuel before movement (default: 16).",
    )
    parser.add_argument(
        "--base-supply",
        type=int,
        default=DEFAULT_BASE_SUPPLY,
        help="Fuel gained when resupplying once at B (default: 8).",
    )
    parser.add_argument(
        "--resource-supply",
        type=int,
        default=DEFAULT_RESOURCE_SUPPLY,
        help="Fuel gained when resupplying once at R (default: 5).",
    )
    return parser.parse_args()


def validate_resource_count(resource_count: int) -> None:
    available_cells = WIDTH * HEIGHT - 2 - len(CENTRAL_B_CANDIDATES)
    if resource_count < 0:
        raise ValueError("resource-count must be non-negative")
    if resource_count > available_cells:
        raise ValueError(f"resource-count must be at most {available_cells}")


def validate_rift_density(rift_density: float) -> None:
    if not 0.0 <= rift_density <= 1.0:
        raise ValueError("rift-density must be between 0.0 and 1.0")


def validate_non_negative(name: str, value: int) -> None:
    if value < 0:
        raise ValueError(f"{name} must be non-negative")


def generate_map(seed: int, resource_count: int, rift_density: float = DEFAULT_RIFT_DENSITY) -> GalacticMap:
    validate_resource_count(resource_count)
    validate_rift_density(rift_density)

    rng = random.Random(seed)
    b_position = rng.choice(CENTRAL_B_CANDIDATES)

    reserved = {SPECIAL_S, SPECIAL_H, b_position, *CENTRAL_B_CANDIDATES}
    resource_candidates = [
        (x, y)
        for y in range(1, HEIGHT + 1)
        for x in range(1, WIDTH + 1)
        if (x, y) not in reserved
    ]
    r_positions = rng.sample(resource_candidates, resource_count)

    cells: dict[tuple[int, int], str] = {}
    for y in range(1, HEIGHT + 1):
        for x in range(1, WIDTH + 1):
            cells[(x, y)] = weighted_terrain(rng)

    cells[SPECIAL_S] = "S"
    cells[SPECIAL_H] = "H"
    cells[b_position] = "B"
    for position in r_positions:
        cells[position] = "R"

    rift_edges = sample_rift_edges(seed, rift_density)

    return GalacticMap(
        seed=seed,
        resource_count=resource_count,
        rift_density=rift_density,
        b_position=b_position,
        r_positions=sorted(r_positions, key=lambda pos: (-pos[1], pos[0])),
        rift_edges=rift_edges,
        cells=cells,
    )


def weighted_terrain(rng: random.Random) -> str:
    roll = rng.random()
    cumulative = 0.0
    for symbol, weight in zip(TERRAIN_SYMBOLS, TERRAIN_WEIGHTS):
        cumulative += weight
        if roll < cumulative:
            return symbol
    return TERRAIN_SYMBOLS[-1]


def terrain_cost(symbol: str) -> int:
    try:
        return TERRAIN_COSTS[symbol]
    except KeyError as exc:
        raise ValueError(f"unknown terrain symbol: {symbol!r}") from exc


def plain_movement_cost(symbol: str) -> int:
    terrain_cost(symbol)
    return 1


def neighbors(position: Position) -> list[Position]:
    x, y = position
    adjacent: list[Position] = []
    for next_x, next_y in ((x, y + 1), (x + 1, y), (x, y - 1), (x - 1, y)):
        if 1 <= next_x <= WIDTH and 1 <= next_y <= HEIGHT:
            adjacent.append((next_x, next_y))
    return adjacent


def normalize_edge(start: Position, goal: Position) -> Edge:
    return (start, goal) if start <= goal else (goal, start)


def undirected_adjacent_edges() -> list[Edge]:
    edges: list[Edge] = []
    for y in range(1, HEIGHT + 1):
        for x in range(1, WIDTH + 1):
            position = (x, y)
            for neighbor in ((x + 1, y), (x, y + 1)):
                if 1 <= neighbor[0] <= WIDTH and 1 <= neighbor[1] <= HEIGHT:
                    edges.append((position, neighbor))
    return edges


def rift_count_for_density(rift_density: float) -> int:
    validate_rift_density(rift_density)
    return round(TOTAL_UNDIRECTED_EDGES * rift_density)


def sample_rift_edges(seed: int, rift_density: float) -> tuple[Edge, ...]:
    rift_count = rift_count_for_density(rift_density)
    edges = undirected_adjacent_edges()
    rng = random.Random(f"{seed}:rift")
    selected = rng.sample(edges, rift_count)
    return tuple(sorted(selected))


def shortest_path(
    cells: Cells,
    start: Position,
    goal: Position,
    blocked_edges: set[Edge] | None = None,
    forbidden_nodes: set[Position] | None = None,
    cost_function: CostFunction = terrain_cost,
) -> PathResult | None:
    if start not in cells:
        raise ValueError(f"start position is outside map cells: {start}")
    if goal not in cells:
        raise ValueError(f"goal position is outside map cells: {goal}")
    blocked = blocked_edges or set()
    forbidden = forbidden_nodes or set()
    if start in forbidden or goal in forbidden:
        return None

    queue: list[tuple[int, int, Position]] = [(0, 0, start)]
    best: dict[Position, tuple[int, int]] = {start: (0, 0)}

    while queue:
        cost, steps, position = heapq.heappop(queue)
        if (cost, steps) != best.get(position):
            continue
        if position == goal:
            return PathResult(cost=cost, steps=steps)

        for neighbor in neighbors(position):
            if neighbor in forbidden:
                continue
            if normalize_edge(position, neighbor) in blocked:
                continue
            candidate = (cost + cost_function(cells[neighbor]), steps + 1)
            previous = best.get(neighbor)
            if previous is None or candidate < previous:
                best[neighbor] = candidate
                heapq.heappush(queue, (candidate[0], candidate[1], neighbor))

    return None


def shortest_path_cost(
    cells: Cells,
    start: Position,
    goal: Position,
    blocked_edges: set[Edge],
    *,
    forbid_home_on_route: bool = False,
) -> int | None:
    forbidden_nodes = {SPECIAL_H} if forbid_home_on_route else None
    result = shortest_path(
        cells,
        start,
        goal,
        blocked_edges,
        forbidden_nodes=forbidden_nodes,
    )
    return None if result is None else result.cost


def route_remaining_fuel(initial_fuel: int, total_cost: int, supply: int) -> int:
    return initial_fuel + supply - total_cost


def leg_costs_to_resource_positions(galactic_map: GalacticMap, blocked_edges: set[Edge]) -> list[LegCost]:
    leg_costs: list[LegCost] = []
    for position in sorted(galactic_map.r_positions):
        cost_to_stop = shortest_path_cost(
            galactic_map.cells,
            SPECIAL_S,
            position,
            blocked_edges,
            forbid_home_on_route=True,
        )
        cost_to_goal = shortest_path_cost(
            galactic_map.cells,
            position,
            SPECIAL_H,
            blocked_edges,
        )
        if cost_to_stop is None or cost_to_goal is None:
            continue
        leg_costs.append(
            LegCost(
                position=position,
                cost_to_stop=cost_to_stop,
                cost_to_goal=cost_to_goal,
            )
        )
    return leg_costs


def analyze_fuel(
    galactic_map: GalacticMap,
    *,
    initial_fuel: int,
    base_supply: int,
    resource_supply: int,
) -> FuelAnalysis:
    validate_non_negative("initial-fuel", initial_fuel)
    validate_non_negative("base-supply", base_supply)
    validate_non_negative("resource-supply", resource_supply)

    blocked_edges = set(galactic_map.rift_edges)

    direct_cost = shortest_path_cost(galactic_map.cells, SPECIAL_S, SPECIAL_H, blocked_edges)
    cost_to_base = shortest_path_cost(
        galactic_map.cells,
        SPECIAL_S,
        galactic_map.b_position,
        blocked_edges,
        forbid_home_on_route=True,
    )
    cost_base_to_goal = shortest_path_cost(
        galactic_map.cells,
        galactic_map.b_position,
        SPECIAL_H,
        blocked_edges,
    )

    fuel_feasible_direct = direct_cost is not None and direct_cost <= initial_fuel
    remaining_fuel_direct = None if not fuel_feasible_direct else initial_fuel - direct_cost

    fuel_feasible_via_base = (
        cost_to_base is not None
        and cost_base_to_goal is not None
        and cost_to_base <= initial_fuel
        and cost_base_to_goal <= initial_fuel - cost_to_base + base_supply
    )
    remaining_fuel_via_base = None
    if fuel_feasible_via_base:
        remaining_fuel_via_base = route_remaining_fuel(
            initial_fuel,
            cost_to_base + cost_base_to_goal,
            base_supply,
        )

    resource_leg_costs = leg_costs_to_resource_positions(galactic_map, blocked_edges)
    best_resource_leg = min(resource_leg_costs, key=lambda leg: (leg.total_cost, leg.position), default=None)

    feasible_resource_legs = [
        leg
        for leg in resource_leg_costs
        if leg.cost_to_stop <= initial_fuel
        and leg.cost_to_goal <= initial_fuel - leg.cost_to_stop + resource_supply
    ]
    best_remaining_resource_leg = max(
        feasible_resource_legs,
        key=lambda leg: (route_remaining_fuel(initial_fuel, leg.total_cost, resource_supply), -leg.position[0], -leg.position[1]),
        default=None,
    )

    fuel_feasible_via_resource = best_remaining_resource_leg is not None
    remaining_fuel_via_resource = None
    if best_remaining_resource_leg is not None:
        remaining_fuel_via_resource = route_remaining_fuel(
            initial_fuel,
            best_remaining_resource_leg.total_cost,
            resource_supply,
        )

    remaining_candidates = [
        value
        for value in (
            remaining_fuel_direct,
            remaining_fuel_via_base,
            remaining_fuel_via_resource,
        )
        if value is not None
    ]
    remaining_fuel_at_goal = max(remaining_candidates, default=None)

    required_supply = None
    if fuel_feasible_direct:
        required_supply = 0
    else:
        supply_candidates: list[int] = []
        if (
            cost_to_base is not None
            and cost_base_to_goal is not None
            and cost_to_base <= initial_fuel
        ):
            supply_candidates.append(max(0, cost_to_base + cost_base_to_goal - initial_fuel))
        for leg in resource_leg_costs:
            if leg.cost_to_stop <= initial_fuel:
                supply_candidates.append(max(0, leg.total_cost - initial_fuel))
        if supply_candidates:
            required_supply = min(supply_candidates)

    return FuelAnalysis(
        fuel_feasible_direct=fuel_feasible_direct,
        fuel_feasible_via_base=fuel_feasible_via_base,
        fuel_feasible_via_resource=fuel_feasible_via_resource,
        remaining_fuel_direct=remaining_fuel_direct,
        remaining_fuel_via_base=remaining_fuel_via_base,
        remaining_fuel_via_resource=remaining_fuel_via_resource,
        remaining_fuel_at_goal=remaining_fuel_at_goal,
        required_supply=required_supply,
        best_cost_via_resource=None if best_resource_leg is None else best_resource_leg.total_cost,
        best_resource_position=None if best_resource_leg is None else best_resource_leg.position,
    )


def analyze_paths(galactic_map: GalacticMap) -> PathAnalysis:
    blocked_edges = set(galactic_map.rift_edges)
    best_route = shortest_path(galactic_map.cells, SPECIAL_S, SPECIAL_H, blocked_edges)
    to_base = shortest_path(galactic_map.cells, SPECIAL_S, galactic_map.b_position, blocked_edges)
    base_to_goal = shortest_path(galactic_map.cells, galactic_map.b_position, SPECIAL_H, blocked_edges)
    without_base = shortest_path(
        galactic_map.cells,
        SPECIAL_S,
        SPECIAL_H,
        blocked_edges,
        forbidden_nodes={galactic_map.b_position},
    )
    via_base = None
    if to_base is not None and base_to_goal is not None:
        via_base = to_base.cost + base_to_goal.cost
    advantage = None
    if via_base is not None and without_base is not None:
        advantage = without_base.cost - via_base
    base_is_mandatory = without_base is None and via_base is not None

    return PathAnalysis(
        reachable=best_route is not None,
        best_cost=None if best_route is None else best_route.cost,
        best_path_length=None if best_route is None else best_route.steps,
        cost_to_base=None if to_base is None else to_base.cost,
        cost_base_to_goal=None if base_to_goal is None else base_to_goal.cost,
        best_cost_via_base=via_base,
        best_cost_without_base=None if without_base is None else without_base.cost,
        base_route_advantage_raw=advantage,
        base_is_mandatory=base_is_mandatory,
    )


def analyze_cost_contributions(galactic_map: GalacticMap) -> CostContributionAnalysis:
    plain_route = shortest_path(
        galactic_map.cells,
        SPECIAL_S,
        SPECIAL_H,
        cost_function=plain_movement_cost,
    )
    terrain_only_route = shortest_path(galactic_map.cells, SPECIAL_S, SPECIAL_H)
    full_route = shortest_path(
        galactic_map.cells,
        SPECIAL_S,
        SPECIAL_H,
        blocked_edges=set(galactic_map.rift_edges),
    )

    plain_cost = None if plain_route is None else plain_route.cost
    terrain_only_cost = None if terrain_only_route is None else terrain_only_route.cost
    full_cost = None if full_route is None else full_route.cost

    terrain_extra_cost = None
    if plain_cost is not None and terrain_only_cost is not None:
        terrain_extra_cost = terrain_only_cost - plain_cost

    rift_detour_cost = None
    if terrain_only_cost is not None and full_cost is not None:
        rift_detour_cost = full_cost - terrain_only_cost

    return CostContributionAnalysis(
        plain_cost=plain_cost,
        terrain_only_cost=terrain_only_cost,
        full_cost=full_cost,
        terrain_extra_cost=terrain_extra_cost,
        rift_detour_cost=rift_detour_cost,
    )


def format_position(position: Position) -> str:
    return f"({position[0]},{position[1]})"


def render_map(cells: Cells) -> str:
    rows: list[str] = []
    for y in range(HEIGHT, 0, -1):
        row = " ".join(cells[(x, y)] for x in range(1, WIDTH + 1))
        rows.append(row)
    return "\n".join(rows)


def terrain_distribution(cells: Cells) -> str:
    counts = Counter(value for value in cells.values() if value in TERRAIN_SYMBOLS)
    ordered = [f"{symbol}:{counts.get(symbol, 0)}" for symbol in TERRAIN_SYMBOLS]
    return ", ".join(ordered)


def format_optional_metric(value: int | None) -> str:
    return "N/A" if value is None else str(value)


def format_yes_no(value: bool) -> str:
    return "yes" if value else "no"


def format_optional_position(position: Position | None) -> str:
    return "N/A" if position is None else format_position(position)


def build_map_id(galactic_map: GalacticMap) -> str:
    return (
        f"seed-{galactic_map.seed}"
        f"-rift-{galactic_map.rift_density:.2f}"
        f"-res-{galactic_map.resource_count}"
    )


def classify_verdict(analysis: PathAnalysis) -> str:
    if analysis.best_cost is None or analysis.cost_to_base is None or analysis.cost_base_to_goal is None:
        return VERDICT_REJECT_TOO_HARD
    if analysis.base_is_mandatory:
        return VERDICT_REJECT_BASE_MANDATORY
    return VERDICT_ACCEPT


def format_output(
    galactic_map: GalacticMap,
    *,
    initial_fuel: int = DEFAULT_INITIAL_FUEL,
    base_supply: int = DEFAULT_BASE_SUPPLY,
    resource_supply: int = DEFAULT_RESOURCE_SUPPLY,
) -> str:
    analysis = analyze_paths(galactic_map)
    cost_contributions = analyze_cost_contributions(galactic_map)
    fuel_analysis = analyze_fuel(
        galactic_map,
        initial_fuel=initial_fuel,
        base_supply=base_supply,
        resource_supply=resource_supply,
    )
    verdict = classify_verdict(analysis)
    resource_positions = ", ".join(format_position(position) for position in galactic_map.r_positions) or "(none)"
    lines = [
        "MAP ID",
        f"  map_id: {build_map_id(galactic_map)}",
        "",
        "OBJECTS",
        f"  S: {format_position(SPECIAL_S)}",
        f"  H: {format_position(SPECIAL_H)}",
        f"  B: {format_position(galactic_map.b_position)}",
        f"  R: {resource_positions}",
        "",
        "PARAMETERS",
        f"  seed: {galactic_map.seed}",
        f"  size: {WIDTH}x{HEIGHT}",
        f"  resource_count: {galactic_map.resource_count}",
        f"  rift_density: {galactic_map.rift_density:.2f}",
        f"  rift_count: {len(galactic_map.rift_edges)}",
        f"  terrain_distribution: {terrain_distribution(galactic_map.cells)}",
        "",
        "FUEL PARAMETERS",
        f"  initial_fuel: {initial_fuel}",
        f"  base_supply: {base_supply}",
        f"  resource_supply: {resource_supply}",
        "",
        "FUEL ANALYSIS",
        f"  fuel_feasible_direct: {format_yes_no(fuel_analysis.fuel_feasible_direct)}",
        f"  fuel_feasible_via_base: {format_yes_no(fuel_analysis.fuel_feasible_via_base)}",
        f"  fuel_feasible_via_resource: {format_yes_no(fuel_analysis.fuel_feasible_via_resource)}",
        f"  remaining_fuel_direct: {format_optional_metric(fuel_analysis.remaining_fuel_direct)}",
        f"  remaining_fuel_via_base: {format_optional_metric(fuel_analysis.remaining_fuel_via_base)}",
        f"  remaining_fuel_via_resource: {format_optional_metric(fuel_analysis.remaining_fuel_via_resource)}",
        f"  remaining_fuel_at_goal: {format_optional_metric(fuel_analysis.remaining_fuel_at_goal)}",
        f"  required_supply: {format_optional_metric(fuel_analysis.required_supply)}",
        f"  best_cost_via_resource: {format_optional_metric(fuel_analysis.best_cost_via_resource)}",
        f"  best_resource_position: {format_optional_position(fuel_analysis.best_resource_position)}",
        "",
        "MAP",
        render_map(galactic_map.cells),
        "",
        "COSTS",
        f"  S_to_H_cost: {format_optional_metric(analysis.best_cost)}",
        f"  S_to_H_steps: {format_optional_metric(analysis.best_path_length)}",
        f"  S_to_B_cost: {format_optional_metric(analysis.cost_to_base)}",
        f"  B_to_H_cost: {format_optional_metric(analysis.cost_base_to_goal)}",
        f"  S_to_H_via_B_cost: {format_optional_metric(analysis.best_cost_via_base)}",
        f"  S_to_H_without_B_cost: {format_optional_metric(analysis.best_cost_without_base)}",
        f"  base_route_advantage_raw: {format_optional_metric(analysis.base_route_advantage_raw)}",
        f"  base_is_mandatory: {format_yes_no(analysis.base_is_mandatory)}",
        "",
        "COST CONTRIBUTIONS",
        f"  plain_cost: {format_optional_metric(cost_contributions.plain_cost)}",
        f"  terrain_only_cost: {format_optional_metric(cost_contributions.terrain_only_cost)}",
        f"  full_cost: {format_optional_metric(cost_contributions.full_cost)}",
        f"  terrain_extra_cost: {format_optional_metric(cost_contributions.terrain_extra_cost)}",
        f"  rift_detour_cost: {format_optional_metric(cost_contributions.rift_detour_cost)}",
        "",
        "VERDICT",
        f"  verdict: {verdict}",
        f"  priority_1: {VERDICT_PRIORITY_ORDER[0]}",
        f"  priority_2: {VERDICT_PRIORITY_ORDER[1]}",
        f"  priority_3: {VERDICT_PRIORITY_ORDER[2]}",
        f"  note: {ACCEPT_NOTE}",
    ]
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    try:
        galactic_map = generate_map(args.seed, args.resource_count, args.rift_density)
        validate_non_negative("initial-fuel", args.initial_fuel)
        validate_non_negative("base-supply", args.base_supply)
        validate_non_negative("resource-supply", args.resource_supply)
    except ValueError as exc:
        raise SystemExit(f"error: {exc}") from exc
    print(
        format_output(
            galactic_map,
            initial_fuel=args.initial_fuel,
            base_supply=args.base_supply,
            resource_supply=args.resource_supply,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
