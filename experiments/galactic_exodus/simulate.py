#!/usr/bin/env python3

from __future__ import annotations

import argparse
from dataclasses import dataclass
import random
from collections import Counter


WIDTH = 8
HEIGHT = 8
DEFAULT_SEED = 42
DEFAULT_RESOURCE_COUNT = 3

SPECIAL_S = (1, 1)
SPECIAL_H = (8, 8)
CENTRAL_B_CANDIDATES = [(4, 4), (5, 4), (4, 5), (5, 5)]
TERRAIN_SYMBOLS = [".", "N", "A", "@"]
TERRAIN_WEIGHTS = [0.60, 0.20, 0.12, 0.08]


@dataclass(frozen=True)
class GalacticMap:
    seed: int
    resource_count: int
    b_position: tuple[int, int]
    r_positions: list[tuple[int, int]]
    cells: dict[tuple[int, int], str]


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


def format_position(position: tuple[int, int]) -> str:
    return f"({position[0]},{position[1]})"


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


def format_output(galactic_map: GalacticMap) -> str:
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
