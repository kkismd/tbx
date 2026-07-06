from __future__ import annotations

from experiments.galactic_exodus.srs.model import Direction, Position, SrsGameState, SrsObjectType, SrsTerrainType


UNKNOWN_SYMBOL = "?"

_TERRAIN_SYMBOLS = {
    SrsTerrainType.FLOOR: ".",
    SrsTerrainType.DEBRIS: ",",
    SrsTerrainType.NEBULA: "~",
    SrsTerrainType.ASTEROID_FIELD: ":",
    SrsTerrainType.ASTEROID: "#",
    SrsTerrainType.RIFT_BARRIER: "#",
    SrsTerrainType.GRAVITY_FIELD_VERTICAL: ".",
    SrsTerrainType.GRAVITY_FIELD_HORIZONTAL: ".",
    SrsTerrainType.RIFT_DISTORTION: ".",
}

_OBJECT_SYMBOLS = {
    SrsObjectType.STAR: "*",
    SrsObjectType.PLANET: "o",
    SrsObjectType.STATION: "S",
    SrsObjectType.RESOURCE_CACHE: "R",
    SrsObjectType.SALVAGE: "$",
}

_CONSUMED_OBJECT_SYMBOLS = {
    SrsObjectType.RESOURCE_CACHE: "r",
    SrsObjectType.SALVAGE: "s",
}

_WARP_SYMBOLS = {
    frozenset({Direction.N}): "^",
    frozenset({Direction.E}): ">",
    frozenset({Direction.S}): "v",
    frozenset({Direction.W}): "<",
}


def render_known_map(state: SrsGameState) -> str:
    return _render_known_map(state, cell_separator="")


def render_known_map_spaced(state: SrsGameState) -> str:
    return _render_known_map(state, cell_separator=" ")


def to_display_position(position: Position) -> tuple[int, int]:
    return (position.x + 1, position.y + 1)


def from_display_position(x: int, y: int) -> Position:
    return Position(x - 1, y - 1)


def render_row_for_internal_y(*, height: int, y: int) -> int:
    return height - 1 - y


def _render_known_map(state: SrsGameState, *, cell_separator: str) -> str:
    rows: list[str] = []
    for y in range(state.actual_map.height - 1, -1, -1):
        chars: list[str] = []
        for x in range(state.actual_map.width):
            position = Position(x, y)
            chars.append(_render_position(state, position))
        rows.append(cell_separator.join(chars))
    return "\n".join(rows)


def _render_position(state: SrsGameState, position: Position) -> str:
    if position not in state.known_state.discovered_cells:
        return UNKNOWN_SYMBOL
    if position == state.player_position:
        return "@"

    cell = state.known_state.known_cells[position]
    if cell.object_id is not None:
        object_state = state.objects[cell.object_id]
        consumed_symbol = _CONSUMED_OBJECT_SYMBOLS.get(object_state.object_type)
        if consumed_symbol is not None and object_state.consumed:
            return consumed_symbol
        return _OBJECT_SYMBOLS[object_state.object_type]

    if cell.warp_flags:
        return _WARP_SYMBOLS.get(cell.warp_flags, "+")

    return _TERRAIN_SYMBOLS[cell.terrain]
