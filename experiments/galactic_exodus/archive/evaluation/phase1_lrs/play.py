#!/usr/bin/env python3

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Sequence, TextIO

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[5]))

from experiments.galactic_exodus import engine, event_format, simulate
from experiments.galactic_exodus.display import render_lrs_border_light_map
from experiments.galactic_exodus.hud import CompactHudContext, render_compact_hud

ABORTED_BY_USER = engine.FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION
KNOWN_TERRAIN_SYMBOLS = {".", "N", "A", "@", "B", "R"}
DIRECTION_ORDER = ("N", "E", "S", "W")


class CliArgumentParser(argparse.ArgumentParser):
    def __init__(self, *, stderr: TextIO, **kwargs: object) -> None:
        super().__init__(**kwargs)
        self.stderr = stderr

    def _print_message(self, message: str | None, file: TextIO | None = None) -> None:
        super()._print_message(message, self.stderr)


def build_parser(stderr: TextIO) -> argparse.ArgumentParser:
    parser = CliArgumentParser(
        stderr=stderr,
        description="Play the Galactic Exodus Phase 1A2 prototype.",
    )
    parser.add_argument("--seed", type=int, help="Requested seed.")
    parser.add_argument(
        "--json-log",
        type=Path,
        help="Write the final GameLog schema_version=3 JSON to this path.",
    )
    return parser


def parse_args(argv: Sequence[str], stderr: TextIO) -> argparse.Namespace:
    parser = build_parser(stderr)
    if not any(
        argument == "--seed" or argument.startswith("--seed=") for argument in argv
    ):
        parser.print_help(stderr)
        raise SystemExit(0)
    return parser.parse_args(argv)


def main(
    argv: Sequence[str] | None = None,
    *,
    stdin: TextIO | None = None,
    stdout: TextIO | None = None,
    stderr: TextIO | None = None,
) -> int:
    effective_argv = list(sys.argv[1:] if argv is None else argv)
    effective_stdin = sys.stdin if stdin is None else stdin
    effective_stdout = sys.stdout if stdout is None else stdout
    effective_stderr = sys.stderr if stderr is None else stderr
    try:
        args = parse_args(effective_argv, effective_stderr)
    except SystemExit as exc:
        return int(exc.code) if isinstance(exc.code, int) else 1

    try:
        state = engine.create_game(args.seed)
    except engine.GenerationError as exc:
        print_generation_error(exc, effective_stdout)
        if args.json_log is not None:
            write_json_log(args.json_log, build_generation_error_log(args.seed, exc))
        return 1

    initial_state = engine.snapshot_state(state)
    events: list[engine.TurnEvent] = []

    render_state(state, effective_stdout)
    while state.game_status == engine.GAME_STATUS_IN_PROGRESS:
        effective_stdout.write("COMMAND> ")
        effective_stdout.flush()
        line = effective_stdin.readline()
        if line == "":
            break
        if line.strip().upper() == "Q":
            break
        event = engine.apply_command(state, line)
        events.append(event)
        render_event(state, event, effective_stdout)
        render_state(state, effective_stdout)

    if args.json_log is not None:
        write_json_log(
            args.json_log, build_session_log(args.seed, state, initial_state, events)
        )
    return 0


def render_state(state: engine.GameState, output: TextIO) -> None:
    output.write("MAP:\n")
    output.write(render_lrs_border_light_map(state))
    output.write("\n")
    output.write("HUD:\n")
    output.write(render_compact_hud(CompactHudContext(lrs_state=state)))
    output.write("\n")
    output.write(
        f"SEED: requested={state.requested_seed} effective={state.effective_seed} rerolls={state.reroll_count}\n"
    )
    output.write(f"POSITION: {format_position(state.player_position)}\n")
    output.write(f"FUEL: {state.remaining_fuel}/{state.settings.max_fuel}\n")
    output.write(f"LAST SUPPLY: {format_last_supply_status(state)}\n")
    output.write(f"USED R: {format_used_resources(state)}\n")
    output.write(f"TURN: {state.turn_count}\n")
    output.write(f"STATUS: {format_status(state.game_status)}\n")
    output.write(f"BLOCKED: {format_blocked_directions(state)}\n")


def board_lines(state: engine.GameState) -> list[str]:
    rows: list[str] = []
    for y in range(simulate.HEIGHT, 0, -1):
        symbols = [display_symbol(state, (x, y)) for x in range(1, simulate.WIDTH + 1)]
        rows.append(f"y={y} {' '.join(symbols)}")
    return rows


def display_symbol(state: engine.GameState, position: simulate.Position) -> str:
    if position == state.player_position:
        return "P"
    if position == state.settings.start_position:
        return "S"
    if position == state.settings.goal_position:
        return "H"
    symbol = state.known_cells.get(position)
    if symbol in KNOWN_TERRAIN_SYMBOLS:
        return symbol
    return "?"


def render_event(
    state: engine.GameState, event: engine.TurnEvent, output: TextIO
) -> None:
    for line in format_event_messages(state, event):
        output.write(f"{line}\n")


def format_event_messages(
    state: engine.GameState, event: engine.TurnEvent
) -> list[str]:
    if event.outcome == engine.OUTCOME_MOVED:
        messages = [event_format.format_lrs_event_summary(event)]
        supply_message = format_supply_message(state, event)
        if supply_message is not None:
            messages.append(supply_message)
        if event.status_after == engine.GAME_STATUS_WON:
            messages.append(f"HOME  reached at LRS={format_position(event.to_position)}")
        elif event.status_after == engine.GAME_STATUS_LOST_FUEL:
            messages.append("STATUS fuel depleted: no further move is possible")
        return messages
    if event.outcome == engine.OUTCOME_BLOCKED_UNKNOWN_RIFT:
        messages = [event_format.format_lrs_event_summary(event)]
        if event.status_after == engine.GAME_STATUS_LOST_FUEL:
            messages.append("STATUS fuel depleted: no further move is possible")
        return messages
    return [event_format.format_lrs_event_summary(event)]


def format_supply_message(state: engine.GameState, event: engine.TurnEvent) -> str | None:
    source = event.supply_source
    if event.supply_result == engine.SUPPLY_RESULT_NONE:
        return None
    if source is None:
        raise ValueError("supply event must include supply_source")
    if event.fuel_before_supply is None or event.fuel_after_supply is None:
        raise ValueError("supply event must include fuel_before_supply and fuel_after_supply")
    if event.supply_result == engine.SUPPLY_RESULT_BASE_REFUELED:
        return (
            "BASE  refueled: "
            f"{event.fuel_before_supply} -> {event.fuel_after_supply} at LRS={format_position(source.position)}"
        )
    if event.supply_result == engine.SUPPLY_RESULT_BASE_ALREADY_FULL:
        return f"BASE  already full at LRS={format_position(source.position)}"
    if event.supply_result == engine.SUPPLY_RESULT_RESOURCE_REFUELED:
        fuel_after = event.fuel_after_supply
        if fuel_after is None:
            raise ValueError("resource refuel event must include fuel_after_supply")
        return (
            "CACHE acquired: "
            f"fuel +{event.supply_amount} -> {fuel_after}/{state.settings.max_fuel} "
            f"at LRS={format_position(source.position)}"
        )
    if event.supply_result == engine.SUPPLY_RESULT_RESOURCE_ALREADY_FULL:
        return f"CACHE full: no refuel at LRS={format_position(source.position)}"
    if event.supply_result == engine.SUPPLY_RESULT_RESOURCE_ALREADY_USED:
        return f"CACHE already used at LRS={format_position(source.position)}"
    raise ValueError(f"unexpected supply result: {event.supply_result}")


def direction_for_event(event: engine.TurnEvent) -> str:
    if event.attempted_position is None:
        raise ValueError("event does not have an attempted position")
    dx = event.attempted_position[0] - event.from_position[0]
    dy = event.attempted_position[1] - event.from_position[1]
    delta_map = {
        (0, 1): "N",
        (1, 0): "E",
        (0, -1): "S",
        (-1, 0): "W",
    }
    try:
        return delta_map[(dx, dy)]
    except KeyError as exc:
        raise ValueError(f"unexpected move delta: {(dx, dy)!r}") from exc


def format_position(position: simulate.Position) -> str:
    return f"({position[0]},{position[1]})"


def format_status(status: str) -> str:
    if status == engine.GAME_STATUS_IN_PROGRESS:
        return "IN PROGRESS"
    if status == engine.GAME_STATUS_WON:
        return "WON"
    if status == engine.GAME_STATUS_LOST_FUEL:
        return "LOST FUEL"
    raise ValueError(f"unexpected game status: {status}")


def format_last_supply_status(state: engine.GameState) -> str:
    if state.last_supply_source is None:
        return "none"
    return format_supply_source(state.last_supply_source)


def format_supply_source(source: engine.SupplySource) -> str:
    return f"{source.kind}{format_position(source.position)}"


def format_used_resources(state: engine.GameState) -> str:
    if not state.used_resource_positions:
        return "-"
    return ",".join(format_position(position) for position in sorted(state.used_resource_positions))


def format_blocked_directions(state: engine.GameState) -> str:
    blocked: list[str] = []
    for direction in DIRECTION_ORDER:
        delta = engine.COMMAND_DELTAS[direction]
        neighbor = engine.move_position(state.player_position, delta)
        if not engine.is_inside_board(neighbor):
            continue
        edge = simulate.normalize_edge(state.player_position, neighbor)
        if state.known_routes.get(edge) == engine.ROUTE_RIFT:
            blocked.append(direction)
    return "-" if not blocked else ",".join(blocked)


def build_generation_error_log(
    requested_seed: int, exc: engine.GenerationError
) -> engine.GameLog:
    return engine.GameLog(
        schema_version=engine.SCHEMA_VERSION,
        settings=engine.DEFAULT_SETTINGS,
        requested_seed=requested_seed,
        effective_seed=None,
        reroll_count=None,
        initial_state=None,
        events=(),
        final_summary=None,
        generation_error=exc.to_info(),
    )


def build_session_log(
    requested_seed: int,
    state: engine.GameState,
    initial_state: dict[str, object],
    events: list[engine.TurnEvent],
) -> engine.GameLog:
    if state.game_status == engine.GAME_STATUS_IN_PROGRESS:
        final_outcome = ABORTED_BY_USER
    else:
        final_outcome = engine.final_outcome_for_status(state.game_status)
    return engine.build_game_log(
        settings=state.settings,
        requested_seed=requested_seed,
        state=state,
        initial_state=initial_state,
        events=events,
        final_outcome=final_outcome,
    )


def print_generation_error(exc: engine.GenerationError, output: TextIO) -> None:
    last_candidate_seed = (
        "none" if exc.last_candidate_seed is None else str(exc.last_candidate_seed)
    )
    output.write(
        "GENERATION ERROR: "
        f"requested={exc.requested_seed} attempts={exc.attempts} "
        f"last_candidate_seed={last_candidate_seed} "
        f"reason={exc.reason} message={exc}\n"
    )


def write_json_log(path: Path, log: engine.GameLog) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(log.to_json() + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
