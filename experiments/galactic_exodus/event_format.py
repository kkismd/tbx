from __future__ import annotations

from typing import Iterable

from experiments.galactic_exodus import engine, simulate


def format_lrs_event_summary(event: engine.TurnEvent) -> str:
    """Format one LRS turn event for normal user-facing output."""
    if event.outcome == engine.OUTCOME_MOVED:
        return f"MOVE  accepted to LRS={_format_position(event.to_position)}"
    if event.outcome == engine.OUTCOME_BLOCKED_UNKNOWN_RIFT:
        return (
            "RIFT  discovered: "
            f"LRS edge {_format_position(event.from_position)}-{_direction_for_event(event)} is blocked"
        )
    if event.outcome == engine.OUTCOME_REJECTED_KNOWN_RIFT:
        return f"MOVE  rejected: known RIFT blocks {_direction_for_event(event)}"
    if event.outcome == engine.OUTCOME_REJECTED_INSUFFICIENT_FUEL:
        return "MOVE  rejected: insufficient fuel"
    if event.outcome == engine.OUTCOME_INVALID_COMMAND:
        return "MOVE  rejected: invalid command"
    if event.outcome == engine.OUTCOME_OUT_OF_BOUNDS:
        return "MOVE  rejected: out of bounds"
    return f"EVENT {event.outcome}"


def format_lrs_debug_event(event: engine.TurnEvent) -> str:
    """Format one LRS turn event for debug/manual inspection."""
    tokens = [
        event.outcome,
        f"turn={event.turn}",
        f"from={_format_position(event.from_position)}",
    ]
    if event.attempted_position is not None:
        tokens.append(f"attempted={_format_position(event.attempted_position)}")
    tokens.extend(
        [
            f"to={_format_position(event.to_position)}",
            f"fuel={event.fuel_before}->{event.fuel_after}",
            f"spent={event.fuel_spent}",
        ]
    )
    if event.required_fuel is not None:
        tokens.append(f"required={event.required_fuel}")
    if event.discovered_cells:
        tokens.append(f"discovered={len(event.discovered_cells)}")
        tokens.append(f"cells={_format_discovered_cells(event.discovered_cells)}")
    if event.discovered_rift:
        tokens.append("discovered_rift=true")
    if event.supply_result != engine.SUPPLY_RESULT_NONE:
        tokens.append(f"supply={event.supply_result}")
    tokens.append(f"status={event.status_after}")
    return " ".join(tokens)


def _direction_for_event(event: engine.TurnEvent) -> str:
    if event.attempted_position is None:
        return "-"
    dx = event.attempted_position[0] - event.from_position[0]
    dy = event.attempted_position[1] - event.from_position[1]
    delta_map = {
        (0, 1): "N",
        (1, 0): "E",
        (0, -1): "S",
        (-1, 0): "W",
    }
    return delta_map.get((dx, dy), "?")


def _format_position(position: simulate.Position) -> str:
    return f"({position[0]},{position[1]})"


def _format_discovered_cells(cells: Iterable[engine.DiscoveredCell]) -> str:
    return ",".join(f"{cell.symbol}@{_format_position(cell.position)}" for cell in cells)
