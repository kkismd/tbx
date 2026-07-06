from __future__ import annotations

from experiments.galactic_exodus import engine, simulate

_OUTER_BORDER = "  +---+---+---+---+---+---+---+---+"
_X_AXIS_LABEL = "    1   2   3   4   5   6   7   8"


def render_lrs_border_light_map(state: engine.GameState) -> str:
    """Render the LRS known macro map using the #1076 border-light baseline."""
    lines = [_OUTER_BORDER]
    for y in range(simulate.HEIGHT, 0, -1):
        lines.append(_render_cell_row(state, y))
        if y > 1:
            lines.append(_render_horizontal_edge_row(state, y))
        else:
            lines.append(_OUTER_BORDER)
    lines.append(_X_AXIS_LABEL)
    return "\n".join(lines)


def _render_cell_row(state: engine.GameState, y: int) -> str:
    segments: list[str] = []
    for x in range(1, simulate.WIDTH + 1):
        position = (x, y)
        segments.append(f" {_lrs_cell_symbol(state, position)} ")
        if x < simulate.WIDTH:
            segments.append("|" if _has_known_vertical_rift(state, position, (x + 1, y)) else " ")
    return f"{y} |{''.join(segments)}|"


def _render_horizontal_edge_row(state: engine.GameState, y: int) -> str:
    content = list(" " * 31)
    for x in range(1, simulate.WIDTH):
        upper_edge = _has_known_vertical_rift(state, (x, y), (x + 1, y))
        lower_edge = _has_known_vertical_rift(state, (x, y - 1), (x + 1, y - 1))
        if upper_edge or lower_edge:
            content[4 * x - 1] = "+"
    for x in range(1, simulate.WIDTH + 1):
        if not _has_known_horizontal_rift(state, (x, y), (x, y - 1)):
            continue
        if x == 1:
            content[0:4] = list("---+")
        elif x == simulate.WIDTH:
            content[27:31] = list("+---")
        else:
            start = 4 * (x - 1) - 1
            content[start : start + 5] = list("+---+")
    return f"  +{''.join(content)}+"


def _has_known_vertical_rift(
    state: engine.GameState,
    position_a: simulate.Position,
    position_b: simulate.Position,
) -> bool:
    edge = simulate.normalize_edge(position_a, position_b)
    return state.known_routes.get(edge) == engine.ROUTE_RIFT


def _has_known_horizontal_rift(
    state: engine.GameState,
    position_a: simulate.Position,
    position_b: simulate.Position,
) -> bool:
    edge = simulate.normalize_edge(position_a, position_b)
    return state.known_routes.get(edge) == engine.ROUTE_RIFT


def _lrs_cell_symbol(state: engine.GameState, position: simulate.Position) -> str:
    if position == state.player_position:
        return "@"
    if position == state.settings.start_position:
        return "S"
    if position == state.settings.goal_position:
        return "H"
    if position in state.used_resource_positions:
        if state.known_cells.get(position) == engine.RESOURCE_CELL:
            return "r"
        return "?"
    symbol = state.known_cells.get(position)
    if symbol in engine.VALID_CELL_SYMBOLS:
        return symbol
    return "?"
