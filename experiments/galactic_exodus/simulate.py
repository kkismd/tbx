#!/usr/bin/env python3

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
from heapq import heappop, heappush
import random


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


@dataclass(frozen=True)
class GalacticMap:
    seed: int
    resource_count: int
    b_position: tuple[int, int]
    r_positions: list[tuple[int, int]]
    cells: dict[tuple[int, int], str]


@dataclass(frozen=True)
class RouteCost:
    reachable: bool
    best_cost: int
    best_path_length: int
    path: list[tuple[int, int]]


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
        "--show-path",
        action="store_true",
        help="Show the minimum-cost path coordinates for S -> H.",
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


def format_position(position: tuple[int, int]) -> str:
    return f"({position[0]},{position[1]})"


def terrain_cost(symbol: str) -> int:
    try:
        return TERRAIN_COSTS[symbol]
    except KeyError as exc:
        raise ValueError(f"unknown terrain symbol: {symbol}") from exc


def neighbors(position: tuple[int, int]) -> list[tuple[int, int]]:
    x, y = position
    adjacent: list[tuple[int, int]] = []
    if x > 1:
        adjacent.append((x - 1, y))
    if x < WIDTH:
        adjacent.append((x + 1, y))
    if y > 1:
        adjacent.append((x, y - 1))
    if y < HEIGHT:
        adjacent.append((x, y + 1))
    return adjacent


def shortest_route(
    cells: dict[tuple[int, int], str],
    start: tuple[int, int],
    goal: tuple[int, int],
) -> RouteCost:
    # All edges are available in this phase, so Dijkstra on destination terrain costs is sufficient.
    frontier: list[tuple[int, int, tuple[tuple[int, int], ...], tuple[int, int]]] = []
    start_state = (0, 0, (start,), start)
    heappush(frontier, start_state)

    best_state: dict[tuple[int, int], tuple[int, int, tuple[tuple[int, int], ...]]] = {
        start: (0, 0, (start,))
    }

    while frontier:
        cost, steps, path, position = heappop(frontier)
        if best_state.get(position) != (cost, steps, path):
            continue
        if position == goal:
            return RouteCost(reachable=True, best_cost=cost, best_path_length=steps, path=list(path))

        for neighbor in neighbors(position):
            next_state = (
                cost + terrain_cost(cells[neighbor]),
                steps + 1,
                path + (neighbor,),
            )
            current_best = best_state.get(neighbor)
            if current_best is None or next_state < current_best:
                best_state[neighbor] = next_state
                heappush(frontier, (*next_state, neighbor))

    return RouteCost(reachable=False, best_cost=0, best_path_length=0, path=[])


def render_map(cells: dict[tuple[int, int], str]) -> str:
    rows: list[str] = []
    for y in range(HEIGHT, 0, -1):
        row = " ".join(cells[(x, y)] for x in range(1, WIDTH + 1))
        rows.append(row)
    return "\n".join(rows)


def terrain_distribution(cells: dict[tuple[int, int], str]) -> str:
    counts = Counter(value for value in cells.values() if value in TERRAIN_SYMBOLS)
    ordered = [f"{symbol}:{counts.get(symbol, 0)}" for symbol in TERRAIN_SYMBOLS]
    return ", ".join(ordered)


def format_path(path: list[tuple[int, int]]) -> str:
    return " -> ".join(format_position(position) for position in path)


def route_summary(galactic_map: GalacticMap) -> dict[str, RouteCost]:
    return {
        "S_H": shortest_route(galactic_map.cells, SPECIAL_S, SPECIAL_H),
        "S_B": shortest_route(galactic_map.cells, SPECIAL_S, galactic_map.b_position),
        "B_H": shortest_route(galactic_map.cells, galactic_map.b_position, SPECIAL_H),
    }


def format_output(galactic_map: GalacticMap, show_path: bool = False) -> str:
    routes = route_summary(galactic_map)
    s_to_h = routes["S_H"]
    s_to_b = routes["S_B"]
    b_to_h = routes["B_H"]
    best_cost_via_base = s_to_b.best_cost + b_to_h.best_cost
    lines = [
        f"SEED: {galactic_map.seed}",
        f"SIZE: {WIDTH}x{HEIGHT}",
        f"S POSITION: {format_position(SPECIAL_S)}",
        f"H POSITION: {format_position(SPECIAL_H)}",
        f"B POSITION: {format_position(galactic_map.b_position)}",
        "R POSITIONS: " + ", ".join(format_position(position) for position in galactic_map.r_positions),
        f"TERRAIN DISTRIBUTION: {terrain_distribution(galactic_map.cells)}",
        "COSTS:",
        f"  reachable: {'yes' if s_to_h.reachable else 'no'}",
        f"  best_cost: {s_to_h.best_cost}",
        f"  best_path_length: {s_to_h.best_path_length}",
        f"  cost_to_base: {s_to_b.best_cost}",
        f"  cost_base_to_goal: {b_to_h.best_cost}",
        f"  best_cost_via_base: {best_cost_via_base}",
        "",
        "MAP:",
        render_map(galactic_map.cells),
    ]
    if show_path:
        lines.extend(["", "BEST PATH:", format_path(s_to_h.path)])
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    try:
        galactic_map = generate_map(args.seed, args.resource_count)
    except ValueError as exc:
        raise SystemExit(f"error: {exc}") from exc
    print(format_output(galactic_map, show_path=args.show_path))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
