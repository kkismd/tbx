#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence, TextIO

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from experiments.galactic_exodus import engine as lrs_engine
from experiments.galactic_exodus.display import render_lrs_border_light_map
from experiments.galactic_exodus.hud import CompactHudContext, render_compact_hud
from experiments.galactic_exodus.srs import engine as srs_engine
from experiments.galactic_exodus.srs import event_format as srs_event_format
from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs import model as srs_model
from experiments.galactic_exodus.srs.render import render_display_map


REPO_ROOT = Path(__file__).resolve().parents[2]
SRS_CONTRACTS = load_default_contracts(REPO_ROOT)


COMMAND_LOOK = "LOOK"
COMMAND_STATUS = "STATUS"
COMMAND_HELP = "HELP"
COMMAND_QUIT = "QUIT"
COMMAND_MOVE = "MOVE"
COMMAND_INTERACT = "INTERACT"
COMMAND_EXIT = "EXIT"
COMMAND_UNKNOWN = "UNKNOWN"

_COMMAND_DIRECTIONS = frozenset({"N", "E", "S", "W"})
_NEBULA_SENSOR_SIZE = 3
_DEFAULT_SENSOR_SIZE = 5
_SRS_MAP_WIDTH = 9
_SRS_MAP_HEIGHT = 9
_SRS_CENTER = srs_model.Position(4, 4)
_ENTRY_POSITIONS = {
    "N": srs_model.Position(4, 8),
    "E": srs_model.Position(8, 4),
    "S": srs_model.Position(4, 0),
    "W": srs_model.Position(0, 4),
}
_DIRECTION_ENUM = {
    "N": srs_model.Direction.N,
    "E": srs_model.Direction.E,
    "S": srs_model.Direction.S,
    "W": srs_model.Direction.W,
}


class CliArgumentParser(argparse.ArgumentParser):
    def __init__(self, *, stderr: TextIO, **kwargs: object) -> None:
        super().__init__(**kwargs)
        self.stderr = stderr

    def _print_message(self, message: str | None, file: TextIO | None = None) -> None:
        super()._print_message(message, self.stderr)


@dataclass(frozen=True, slots=True)
class IntegratedCommand:
    kind: str
    directions: tuple[str, ...] = ()
    raw: str = ""


@dataclass(frozen=True, slots=True)
class IntegratedCommandResult:
    accepted: bool
    command_type: str
    summary_lines: tuple[str, ...]
    changed_lrs_position: bool = False
    changed_srs_position: bool = False
    should_quit: bool = False


@dataclass(slots=True)
class IntegratedGameState:
    lrs_state: lrs_engine.GameState
    srs_state: srs_model.SrsGameState
    last_event_summary: str | None = None
    session_aborted: bool = False


def build_parser(stderr: TextIO) -> argparse.ArgumentParser:
    parser = CliArgumentParser(
        stderr=stderr,
        description="Play the Galactic Exodus integrated command-response prototype.",
    )
    parser.add_argument("--seed", type=int, help="Requested seed.")
    return parser


def parse_args(argv: Sequence[str], stderr: TextIO) -> argparse.Namespace:
    parser = build_parser(stderr)
    if not any(argument == "--seed" or argument.startswith("--seed=") for argument in argv):
        parser.print_help(stderr)
        raise SystemExit(0)
    return parser.parse_args(argv)


def parse_integrated_command(raw: str) -> IntegratedCommand:
    normalized = _normalize_command_text(raw)
    if normalized == "HELP":
        return IntegratedCommand(kind=COMMAND_HELP, raw=normalized)
    if normalized == "LOOK":
        return IntegratedCommand(kind=COMMAND_LOOK, raw=normalized)
    if normalized == "STATUS":
        return IntegratedCommand(kind=COMMAND_STATUS, raw=normalized)
    if normalized in {"Q", "QUIT"}:
        return IntegratedCommand(kind=COMMAND_QUIT, raw=normalized)
    if normalized == "INTERACT":
        return IntegratedCommand(kind=COMMAND_INTERACT, raw=normalized)

    tokens = normalized.split()
    if len(tokens) == 1 and tokens[0] in _COMMAND_DIRECTIONS:
        return IntegratedCommand(kind=COMMAND_MOVE, directions=(tokens[0],), raw=normalized)
    if tokens and tokens[0] == "MOVE" and tokens[1:]:
        return IntegratedCommand(kind=COMMAND_MOVE, directions=tuple(tokens[1:]), raw=normalized)
    if tokens and tokens[0] == "EXIT" and _all_directions(tokens[1:]) and len(tokens[1:]) == 1:
        return IntegratedCommand(kind=COMMAND_EXIT, directions=(tokens[1],), raw=normalized)
    return IntegratedCommand(kind=COMMAND_UNKNOWN, raw=normalized)


def create_integrated_game(seed: int) -> IntegratedGameState:
    lrs_state = lrs_engine.create_game(seed)
    sector_symbol = lrs_state.known_cells.get(lrs_state.player_position)
    srs_state = _create_minimal_srs_for_sector(
        seed=lrs_state.effective_seed,
        lrs_position=lrs_state.player_position,
        sector_symbol=sector_symbol,
    )
    return IntegratedGameState(
        lrs_state=lrs_state,
        srs_state=srs_state,
        last_event_summary=f"GAME  started seed={lrs_state.effective_seed}",
    )


def execute_integrated_command(
    state: IntegratedGameState,
    command: IntegratedCommand,
) -> IntegratedCommandResult:
    if command.kind == COMMAND_HELP:
        return IntegratedCommandResult(
            accepted=True,
            command_type=COMMAND_HELP,
            summary_lines=("HELP  commands: N/E/S/W, MOVE <route>, INTERACT, EXIT <dir>, LOOK, STATUS, Q",),
        )
    if command.kind == COMMAND_LOOK:
        return IntegratedCommandResult(
            accepted=True,
            command_type=COMMAND_LOOK,
            summary_lines=("LOOK  current tactical response",),
        )
    if command.kind == COMMAND_STATUS:
        return IntegratedCommandResult(
            accepted=True,
            command_type=COMMAND_STATUS,
            summary_lines=("STATUS current ship status",),
        )
    if command.kind == COMMAND_QUIT:
        return IntegratedCommandResult(
            accepted=True,
            command_type=COMMAND_QUIT,
            summary_lines=("QUIT  session ended",),
            should_quit=True,
        )
    if command.kind == COMMAND_MOVE:
        return _execute_move_command(state, command)
    if command.kind == COMMAND_INTERACT:
        return IntegratedCommandResult(
            accepted=False,
            command_type=COMMAND_INTERACT,
            summary_lines=("INTERACT rejected: interaction is not implemented in integrated CLI yet",),
        )
    if command.kind == COMMAND_EXIT:
        return IntegratedCommandResult(
            accepted=False,
            command_type=COMMAND_EXIT,
            summary_lines=("EXIT  rejected: exit is not implemented in integrated CLI yet",),
        )
    return IntegratedCommandResult(
        accepted=False,
        command_type=COMMAND_UNKNOWN,
        summary_lines=("COMMAND rejected: unknown command",),
    )


def render_integrated_response(
    state: IntegratedGameState,
    result: IntegratedCommandResult,
) -> str:
    blocks = [
        "RESULT",
        *result.summary_lines,
        "",
        "LRS",
        render_lrs_border_light_map(state.lrs_state),
        "",
        "SRS",
        render_display_map(state.srs_state),
        "",
        "HUD",
        render_compact_hud(
            CompactHudContext(
                lrs_state=state.lrs_state,
                srs_state=state.srs_state,
                last_event_summary=state.last_event_summary,
                cost_mode="TURN_ONLY",
            )
        ),
    ]
    return "\n".join(blocks) + "\n"


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

    state = create_integrated_game(args.seed)
    initial_summary = state.last_event_summary or f"GAME  started seed={args.seed}"
    initial_result = IntegratedCommandResult(
        accepted=True,
        command_type="INIT",
        summary_lines=(initial_summary,),
    )
    effective_stdout.write(render_integrated_response(state, initial_result))
    while True:
        effective_stdout.write("COMMAND> ")
        effective_stdout.flush()
        line = effective_stdin.readline()
        if line == "":
            break
        command = parse_integrated_command(line)
        result = execute_integrated_command(state, command)
        if result.should_quit:
            break
        effective_stdout.write(render_integrated_response(state, result))
    return 0


def _normalize_command_text(raw: str) -> str:
    stripped = raw.strip().upper().replace(",", " ")
    return re.sub(r"\s+", " ", stripped)


def _all_directions(tokens: Sequence[str]) -> bool:
    return bool(tokens) and all(token in _COMMAND_DIRECTIONS for token in tokens)


def _execute_move_command(
    state: IntegratedGameState,
    command: IntegratedCommand,
) -> IntegratedCommandResult:
    if not _all_directions(command.directions):
        summary = "MOVE  rejected: invalid direction"
        state.last_event_summary = summary
        return IntegratedCommandResult(
            accepted=False,
            command_type=COMMAND_MOVE,
            summary_lines=(summary,),
            changed_lrs_position=False,
            changed_srs_position=False,
        )

    route = tuple(_DIRECTION_ENUM[direction] for direction in command.directions)
    previous_position = state.srs_state.player_position
    result = srs_engine.apply_srs_command(
        state.srs_state,
        srs_model.SrsCommand(command_type="MOVE_ROUTE", route=route),
        contracts=SRS_CONTRACTS,
    )
    state.srs_state = result.state

    summary_lines = tuple(_format_summary_lines(result.events))
    if summary_lines:
        state.last_event_summary = summary_lines[0]

    return IntegratedCommandResult(
        accepted=_movement_result_accepted(result),
        command_type=COMMAND_MOVE,
        summary_lines=summary_lines,
        changed_lrs_position=False,
        changed_srs_position=result.state.player_position != previous_position,
    )


def _format_summary_lines(events: Sequence[srs_model.SrsTurnEvent]) -> list[str]:
    summary_lines: list[str] = []
    for event in events:
        summary_lines.extend(srs_event_format.format_srs_event_summary_lines(event))
    return summary_lines


def _movement_result_accepted(result: srs_model.SrsCommandResult) -> bool:
    if not result.events:
        return False
    return result.events[0].event_type != srs_engine.MOVE_REJECTED


def _sector_type_for_lrs_symbol(symbol: str | None) -> srs_model.SectorType:
    mapping = {
        "N": srs_model.SectorType.NEBULA,
        "A": srs_model.SectorType.ASTEROID,
        "B": srs_model.SectorType.BASE,
        "R": srs_model.SectorType.RESOURCE,
        "S": srs_model.SectorType.NORMAL,
        "H": srs_model.SectorType.NORMAL,
        ".": srs_model.SectorType.NORMAL,
    }
    if symbol == "@" and hasattr(srs_model.SectorType, "GRAVITY"):
        return srs_model.SectorType.GRAVITY
    return mapping.get(symbol or "", srs_model.SectorType.NORMAL)


def _create_minimal_srs_for_sector(
    *,
    seed: int,
    lrs_position: tuple[int, int],
    sector_symbol: str | None,
    entry_direction: str | None = None,
) -> srs_model.SrsGameState:
    sector_type = _sector_type_for_lrs_symbol(sector_symbol)
    player_position = _player_position_for_entry(entry_direction)
    warp_flags = _warp_flags_for_entry(entry_direction)
    rows = _make_floor_rows(warp_flags)
    actual_map = srs_model.SrsActualMap(
        width=_SRS_MAP_WIDTH,
        height=_SRS_MAP_HEIGHT,
        cells=tuple(tuple(row) for row in rows),
    )
    discovered_cells = _observed_positions(player_position, sector_type)
    known_cells = {
        position: actual_map.cell_at(position)
        for position in discovered_cells
    }
    descriptor = srs_model.SectorDescriptor(
        sector_id=f"lrs-{lrs_position[0]}-{lrs_position[1]}",
        sector_type=sector_type,
        sector_seed=seed,
        entry_edge=_DIRECTION_ENUM.get(entry_direction or "S", srs_model.Direction.S),
        blocked_edges=frozenset(),
    )
    persistent_state = srs_model.SrsPersistentState(
        generated_map_id=f"{descriptor.sector_id}:{seed}",
        generation_schema_version=1,
        generation_seed=seed,
        sector_type=sector_type,
        blocked_edges=frozenset(),
        warp_flags={
            position: cell.warp_flags
            for position, cell in known_cells.items()
            if cell.warp_flags
        },
        discovered_cells=discovered_cells,
    )
    return srs_model.SrsGameState(
        descriptor=descriptor,
        actual_map=actual_map,
        known_state=srs_model.SrsKnownState(
            discovered_cells=discovered_cells,
            known_cells=known_cells,
            visited_cells=frozenset({player_position}),
        ),
        persistent_state=persistent_state,
        player_position=player_position,
        objects={},
        combat_state=None,
        srs_turn=0,
        fuel=0,
        max_fuel=0,
    )


def _player_position_for_entry(entry_direction: str | None) -> srs_model.Position:
    if entry_direction is None:
        return _SRS_CENTER
    return _ENTRY_POSITIONS[entry_direction]


def _warp_flags_for_entry(
    entry_direction: str | None,
) -> dict[srs_model.Position, frozenset[srs_model.Direction]]:
    if entry_direction is None:
        return {}
    direction = _DIRECTION_ENUM[entry_direction]
    return {_ENTRY_POSITIONS[entry_direction]: frozenset({direction})}


def _make_floor_rows(
    warp_flags: dict[srs_model.Position, frozenset[srs_model.Direction]],
) -> list[list[srs_model.SrsCell]]:
    rows: list[list[srs_model.SrsCell]] = []
    for y in range(_SRS_MAP_HEIGHT):
        row: list[srs_model.SrsCell] = []
        for x in range(_SRS_MAP_WIDTH):
            position = srs_model.Position(x, y)
            row.append(
                srs_model.SrsCell(
                    terrain=srs_model.SrsTerrainType.FLOOR,
                    warp_flags=warp_flags.get(position, frozenset()),
                )
            )
        rows.append(row)
    return rows


def _observed_positions(
    center: srs_model.Position,
    sector_type: srs_model.SectorType,
) -> frozenset[srs_model.Position]:
    size = _NEBULA_SENSOR_SIZE if sector_type is srs_model.SectorType.NEBULA else _DEFAULT_SENSOR_SIZE
    radius = size // 2
    positions = set()
    for dx in range(-radius, radius + 1):
        for dy in range(-radius, radius + 1):
            x = center.x + dx
            y = center.y + dy
            if 0 <= x < _SRS_MAP_WIDTH and 0 <= y < _SRS_MAP_HEIGHT:
                positions.add(srs_model.Position(x, y))
    return frozenset(positions)


if __name__ == "__main__":
    raise SystemExit(main())
