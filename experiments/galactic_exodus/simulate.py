#!/usr/bin/env python3

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import heapq
import random
from typing import TypeAlias


WIDTH = 8
HEIGHT = 8
DEFAULT_SEED = 42
DEFAULT_RESOURCE_COUNT = 3

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


@dataclass(frozen=True)
class GalacticMap:
    seed: int
    resource_count: int
    b_position: Position
    r_positions: list[Position]
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate a deterministic 8x8 Galactic Exodus map.")
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED, help="Random seed (default: 42).")
    parser.add_argument(
        "--resource-count",
        type=int,
        default=DEFAULT_RESOURCE_COUNT,
        help="Number of resource objects to place (default: 3).",
    )
    return parser.parse_args()


def validate_resource_count(resource_count: int) -> None:
    available_cells = WIDTH * HEIGHT - 2 - len(CENTRAL_B_CANDIDATES)
    if resource_count < 0:
        raise ValueError("resource-count must be non-negative")
    if resource_count > available_cells:
        raise ValueError(f"resource-count must be at most {available_cells}")


def generate_map(seed: int, resource_count: int) -> GalacticMap:
    validate_resource_count(resource_count)

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

    return GalacticMap(
        seed=seed,
        resource_count=resource_count,
        b_position=b_position,
        r_positions=sorted(r_positions, key=lambda pos: (-pos[1], pos[0])),
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


def neighbors(position: Position) -> list[Position]:
    x, y = position
    adjacent: list[Position] = []
    for next_x, next_y in ((x, y + 1), (x + 1, y), (x, y - 1), (x - 1, y)):
        if 1 <= next_x <= WIDTH and 1 <= next_y <= HEIGHT:
            adjacent.append((next_x, next_y))
    return adjacent


def shortest_path(cells: Cells, start: Position, goal: Position) -> PathResult | None:
    if start not in cells:
        raise ValueError(f"start position is outside map cells: {start}")
    if goal not in cells:
        raise ValueError(f"goal position is outside map cells: {goal}")

    queue: list[tuple[int, int, Position]] = [(0, 0, start)]
    best: dict[Position, tuple[int, int]] = {start: (0, 0)}

    while queue:
        cost, steps, position = heapq.heappop(queue)
        if (cost, steps) != best.get(position):
            continue
        if position == goal:
            return PathResult(cost=cost, steps=steps)

        for neighbor in neighbors(position):
            candidate = (cost + terrain_cost(cells[neighbor]), steps + 1)
            previous = best.get(neighbor)
            if previous is None or candidate < previous:
                best[neighbor] = candidate
                heapq.heappush(queue, (candidate[0], candidate[1], neighbor))

    return None


def analyze_paths(galactic_map: GalacticMap) -> PathAnalysis:
    best_route = shortest_path(galactic_map.cells, SPECIAL_S, SPECIAL_H)
    to_base = shortest_path(galactic_map.cells, SPECIAL_S, galactic_map.b_position)
    base_to_goal = shortest_path(galactic_map.cells, galactic_map.b_position, SPECIAL_H)
    via_base = None
    if to_base is not None and base_to_goal is not None:
        via_base = to_base.cost + base_to_goal.cost

    return PathAnalysis(
        reachable=best_route is not None,
        best_cost=None if best_route is None else best_route.cost,
        best_path_length=None if best_route is None else best_route.steps,
        cost_to_base=None if to_base is None else to_base.cost,
        cost_base_to_goal=None if base_to_goal is None else base_to_goal.cost,
        best_cost_via_base=via_base,
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


def format_output(galactic_map: GalacticMap) -> str:
    analysis = analyze_paths(galactic_map)
    lines = [
        f"SEED: {galactic_map.seed}",
        f"SIZE: {WIDTH}x{HEIGHT}",
        f"S POSITION: {format_position(SPECIAL_S)}",
        f"H POSITION: {format_position(SPECIAL_H)}",
        f"B POSITION: {format_position(galactic_map.b_position)}",
        "R POSITIONS: " + ", ".join(format_position(position) for position in galactic_map.r_positions),
        f"TERRAIN DISTRIBUTION: {terrain_distribution(galactic_map.cells)}",
        "",
        "MAP:",
        render_map(galactic_map.cells),
        "",
        "COSTS:",
        f"  reachable: {'yes' if analysis.reachable else 'no'}",
        f"  best_cost: {format_optional_metric(analysis.best_cost)}",
        f"  best_path_length: {format_optional_metric(analysis.best_path_length)}",
        f"  cost_to_base: {format_optional_metric(analysis.cost_to_base)}",
        f"  cost_base_to_goal: {format_optional_metric(analysis.cost_base_to_goal)}",
        f"  best_cost_via_base: {format_optional_metric(analysis.best_cost_via_base)}",
    ]
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    try:
        galactic_map = generate_map(args.seed, args.resource_count)
    except ValueError as exc:
        raise SystemExit(f"error: {exc}") from exc
    print(format_output(galactic_map))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
