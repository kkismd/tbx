from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from experiments.galactic_exodus import engine
from experiments.galactic_exodus import simulate
from experiments.galactic_exodus import validate_phase1_spec


def load_fixture_file(path: str | Path) -> dict[str, Any]:
    fixture_path = Path(path)
    try:
        payload = json.loads(fixture_path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValueError(f"missing fixture file: {fixture_path}") from exc
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid JSON in {fixture_path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise ValueError("fixture root must be an object")
    return payload


def settings_from_dict(value: Any) -> engine.GameSettings:
    if not isinstance(value, dict):
        raise ValueError("settings must be an object")
    expected_keys = {
        "width",
        "height",
        "start_position",
        "goal_position",
        "rift_density",
        "initial_fuel",
        "max_fuel",
        "resource_count",
        "resource_supply",
    }
    if set(value) != expected_keys:
        raise ValueError(f"settings keys must be {sorted(expected_keys)}")
    settings = engine.GameSettings(
        width=int(value["width"]),
        height=int(value["height"]),
        start_position=position_from_dict(value["start_position"], label="settings.start_position"),
        goal_position=position_from_dict(value["goal_position"], label="settings.goal_position"),
        rift_density=float(value["rift_density"]),
        initial_fuel=int(value["initial_fuel"]),
        max_fuel=int(value["max_fuel"]),
        resource_count=int(value["resource_count"]),
        resource_supply=int(value["resource_supply"]),
    )
    settings.validate()
    return settings


def actual_map_from_dict(value: Any) -> engine.ActualMap:
    if not isinstance(value, dict):
        raise ValueError("actual_map must be an object")
    expected_keys = {"cells", "rift_edges", "base_position", "resource_positions"}
    if set(value) != expected_keys:
        raise ValueError(f"actual_map keys must be {sorted(expected_keys)}")

    cells_value = value["cells"]
    if not isinstance(cells_value, list):
        raise ValueError("actual_map.cells must be an array")
    cells: simulate.Cells = {}
    for index, cell_value in enumerate(cells_value):
        if not isinstance(cell_value, dict):
            raise ValueError(f"actual_map.cells[{index}] must be an object")
        position = position_from_dict(cell_value.get("position"), label=f"actual_map.cells[{index}].position")
        symbol = cell_value.get("symbol")
        if not isinstance(symbol, str) or len(symbol) != 1:
            raise ValueError(f"actual_map.cells[{index}].symbol must be one character")
        if position in cells:
            raise ValueError(f"actual_map.cells has duplicate position {position}")
        cells[position] = symbol

    rift_edges_value = value["rift_edges"]
    if not isinstance(rift_edges_value, list):
        raise ValueError("actual_map.rift_edges must be an array")
    rift_edges: list[simulate.Edge] = []
    for index, edge_value in enumerate(rift_edges_value):
        if not isinstance(edge_value, dict) or set(edge_value) != {"from", "to"}:
            raise ValueError(f"actual_map.rift_edges[{index}] must have exactly from and to")
        start = position_from_dict(edge_value["from"], label=f"actual_map.rift_edges[{index}].from")
        goal = position_from_dict(edge_value["to"], label=f"actual_map.rift_edges[{index}].to")
        rift_edges.append((start, goal))

    resource_positions_value = value["resource_positions"]
    if not isinstance(resource_positions_value, list):
        raise ValueError("actual_map.resource_positions must be an array")
    resource_positions = tuple(
        position_from_dict(position_value, label=f"actual_map.resource_positions[{index}]")
        for index, position_value in enumerate(resource_positions_value)
    )

    return engine.ActualMap(
        cells=cells,
        rift_edges=tuple(rift_edges),
        base_position=position_from_dict(value["base_position"], label="actual_map.base_position"),
        resource_positions=resource_positions,
    )


def replay_fixture(fixture: dict[str, Any]) -> engine.GameLog:
    if not isinstance(fixture, dict):
        raise ValueError("fixture must be an object")

    name = fixture.get("name", "<unnamed>")
    try:
        settings = settings_from_dict(fixture.get("settings"))
        requested_seed = int(fixture["requested_seed"])
        effective_seed = fixture["effective_seed"]
        reroll_count = fixture["reroll_count"]
        max_turns = int(fixture["max_turns"])
        commands = fixture["commands"]
        if not isinstance(commands, list) or not all(isinstance(command, str) for command in commands):
            raise ValueError("commands must be an array of strings")

        mode = fixture["mode"]
        if mode == "generated":
            log = engine.run_commands(
                requested_seed,
                commands,
                settings=settings,
                max_turns=max_turns,
            )
        elif mode == "injected":
            actual_map = actual_map_from_dict(fixture["initial_actual_map"])
            state = engine.create_game_from_actual_map(
                actual_map,
                settings=settings,
                requested_seed=requested_seed,
                effective_seed=int(effective_seed),
                reroll_count=int(reroll_count),
            )
            log = engine.run_state_commands(state, commands, max_turns=max_turns)
        elif mode == "generation_error":
            generation_stub = fixture["generation_stub"]
            log = replay_generation_error_fixture(
                requested_seed=requested_seed,
                commands=commands,
                settings=settings,
                max_turns=max_turns,
                generation_stub=generation_stub,
            )
        else:
            raise ValueError(f"unknown mode {mode!r}")

        if mode == "generated":
            actual_initial_map = log.initial_state["actual_map"] if log.initial_state is not None else None
            if actual_initial_map != fixture["initial_actual_map"]:
                raise AssertionError("$.initial_actual_map: generated actual map mismatch")

        assert_partial_match(fixture["expected_initial"], log.initial_state, path="$.expected_initial")
        actual_turns = [event.to_dict() for event in log.events]
        assert_partial_match(fixture["expected_turns"], actual_turns, path="$.expected_turns")
        assert_partial_match(fixture["expected_final"], build_actual_final(log), path="$.expected_final")
        return log
    except (AssertionError, ValueError) as exc:
        raise type(exc)(f"{name}: {exc}") from exc


def assert_partial_match(expected: Any, actual: Any, path: str = "$") -> None:
    if expected is None:
        if actual is not None:
            raise AssertionError(f"{path}: expected None, got {actual!r}")
        return

    if isinstance(expected, dict):
        if not isinstance(actual, dict):
            raise AssertionError(f"{path}: expected object, got {type(actual).__name__}")
        for key, expected_value in expected.items():
            if key not in actual:
                raise AssertionError(f"{path}.{key}: missing key")
            assert_partial_match(expected_value, actual[key], path=f"{path}.{key}")
        return

    if isinstance(expected, list):
        if not isinstance(actual, list):
            raise AssertionError(f"{path}: expected array, got {type(actual).__name__}")
        if len(expected) != len(actual):
            raise AssertionError(f"{path}: expected array length {len(expected)}, got {len(actual)}")
        for index, expected_item in enumerate(expected):
            assert_partial_match(expected_item, actual[index], path=f"{path}[{index}]")
        return

    if expected != actual:
        raise AssertionError(f"{path}: expected {expected!r}, got {actual!r}")


def replay_all(path: str | Path) -> None:
    fixture_path = Path(path)
    validate_phase1_spec.validate_fixtures(fixture_path)
    payload = load_fixture_file(fixture_path)
    fixtures = payload["fixtures"]
    for fixture in fixtures:
        replay_fixture(fixture)


def position_from_dict(value: Any, *, label: str) -> simulate.Position:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    if set(value) != {"x", "y"}:
        raise ValueError(f"{label} must have exactly x and y")
    x = value["x"]
    y = value["y"]
    if isinstance(x, bool) or not isinstance(x, int) or isinstance(y, bool) or not isinstance(y, int):
        raise ValueError(f"{label} coordinates must be integers")
    return (x, y)


def build_actual_final(log: engine.GameLog) -> dict[str, Any]:
    if log.final_summary is not None:
        return log.final_summary.to_dict()
    if log.generation_error is not None:
        final = {"outcome": "GENERATION_ERROR"}
        final.update(log.generation_error.to_dict())
        return final
    raise ValueError("game log has neither final_summary nor generation_error")


def replay_generation_error_fixture(
    *,
    requested_seed: int,
    commands: list[str],
    settings: engine.GameSettings,
    max_turns: int,
    generation_stub: Any,
) -> engine.GameLog:
    if not isinstance(generation_stub, dict):
        raise ValueError("generation_stub must be an object")
    reachable_sequence = generation_stub.get("reachable_sequence")
    if (
        not isinstance(reachable_sequence, list)
        or not reachable_sequence
        or not all(isinstance(item, bool) for item in reachable_sequence)
    ):
        raise ValueError("generation_stub.reachable_sequence must be a non-empty array of booleans")

    sequence_index = {"value": 0}

    def generate_candidate(seed: int, resource_count: int, rift_density: float) -> simulate.GalacticMap:
        return simulate.generate_map(1, resource_count, rift_density)

    def is_reachable(galactic_map: simulate.GalacticMap) -> bool:
        index = min(sequence_index["value"], len(reachable_sequence) - 1)
        sequence_index["value"] += 1
        return reachable_sequence[index]

    try:
        galactic_map, effective_seed, reroll_count = engine.create_playable_map(
            requested_seed,
            settings,
            generate_candidate=generate_candidate,
            is_reachable=is_reachable,
        )
    except engine.GenerationError as exc:
        return engine.GameLog(
            schema_version=engine.SCHEMA_VERSION,
            settings=settings,
            requested_seed=requested_seed,
            effective_seed=None,
            reroll_count=None,
            initial_state=None,
            events=(),
            final_summary=None,
            generation_error=exc.to_info(),
        )

    actual_map = engine.ActualMap(
        cells=dict(galactic_map.cells),
        rift_edges=tuple(galactic_map.rift_edges),
        base_position=galactic_map.b_position,
        resource_positions=tuple(galactic_map.r_positions),
    )
    state = engine.create_game_from_actual_map(
        actual_map,
        settings=settings,
        requested_seed=requested_seed,
        effective_seed=effective_seed,
        reroll_count=reroll_count,
    )
    return engine.run_state_commands(state, commands, max_turns=max_turns)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Replay Galactic Exodus Phase 1 reference fixtures")
    parser.add_argument("--fixtures", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        replay_all(args.fixtures)
    except (AssertionError, ValueError, validate_phase1_spec.ValidationError) as exc:
        print(exc, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
