from __future__ import annotations

from dataclasses import dataclass

from experiments.galactic_exodus import engine
from experiments.galactic_exodus.srs import model as srs_model
from experiments.galactic_exodus.srs.render import to_display_position


_DIRECTION_ORDER = (
    srs_model.Direction.N,
    srs_model.Direction.E,
    srs_model.Direction.S,
    srs_model.Direction.W,
)

_LRS_SYMBOL_TO_SECTOR_TYPE = {
    ".": "NORMAL",
    "N": "NEBULA",
    "A": "ASTEROID",
    "@": "GRAVITY",
    "B": "BASE",
    "R": "RESOURCE",
    "S": "START",
    "H": "HOME",
    "?": "UNKNOWN",
}

_REWARD_LABELS = {
    srs_model.SrsObjectType.SALVAGE: "SALVAGE",
    srs_model.SrsObjectType.RESOURCE_CACHE: "CACHE",
    srs_model.SrsObjectType.STATION: "STATION",
}


@dataclass(frozen=True, slots=True)
class CompactHudContext:
    lrs_state: engine.GameState | None = None
    srs_state: srs_model.SrsGameState | None = None
    last_event_summary: str | None = None
    status: str | None = None
    cost_mode: str | None = None


def render_compact_hud(context: CompactHudContext) -> str:
    """Render the #1076 compact HUD block for normal display."""
    lines = [
        _render_sector_line(context),
        _render_turn_line(context),
        _render_fuel_line(context),
        _render_player_line(context),
        _render_combat_line(context),
        _render_warp_line(context),
        _render_reward_line(context),
        _render_last_line(context),
    ]
    return "\n".join(lines)


def _render_sector_line(context: CompactHudContext) -> str:
    lrs_coord = _lrs_coordinate_text(context.lrs_state)
    sector_type = _sector_type_text(context)
    srs_coord = _srs_coordinate_text(context.srs_state)
    sensor = _sensor_range_text(context.srs_state)
    return (
        f"SECTOR  LRS={_pad(lrs_coord, 5)}  "
        f"TYPE={_pad(sector_type, 4)}  "
        f"SRS={_pad(srs_coord, 5)}  "
        f"SENSOR={sensor}"
    )


def _render_turn_line(context: CompactHudContext) -> str:
    lrs_turn = "-" if context.lrs_state is None else str(context.lrs_state.turn_count)
    srs_turn = "-" if context.srs_state is None else str(context.srs_state.srs_turn)
    cost_mode = _cost_mode_text(context)
    return (
        f"TURN    LRS={_pad(lrs_turn, 5)}  "
        f"SRS={_pad(srs_turn, 5)}  "
        f"COST={cost_mode}"
    )


def _render_fuel_line(context: CompactHudContext) -> str:
    fuel = _fuel_text(context)
    status = _status_text(context)
    return f"FUEL    {_pad(fuel, 9)}  STATUS={status}"


def _render_player_line(context: CompactHudContext) -> str:
    player = None if context.srs_state is None else context.srs_state.player_state
    if player is None:
        return "PLAYER  DUR=-      EN=-      TORP=-      SALVAGE=-"
    return (
        f"PLAYER  DUR={_pad(f'{player.durability}/{player.durability_capacity}', 5)}  "
        f"EN={_pad(f'{player.energy}/{player.energy_capacity}', 5)}  "
        f"TORP={_pad(f'{player.photon_torpedo_ammo}/{player.photon_torpedo_ammo_capacity}', 5)}  "
        f"SALVAGE={player.salvage}"
    )


def _render_combat_line(context: CompactHudContext) -> str:
    combat_state = None if context.srs_state is None else context.srs_state.combat_state
    if combat_state is None or not combat_state.enemies:
        return "COMBAT  none"
    enemy = _select_enemy(combat_state)
    enemy_position = _display_position_text(enemy.position)
    return (
        f"COMBAT  PHASE={combat_state.phase.value}  "
        f"ENEMY={enemy.enemy_id} {enemy.tier.value} "
        f"hp={enemy.durability} at SRS={enemy_position}"
    )


def _render_warp_line(context: CompactHudContext) -> str:
    summary = _warp_summary_text(context.srs_state)
    return f"WARP    {summary}"


def _render_reward_line(context: CompactHudContext) -> str:
    summary = _reward_summary_text(context.srs_state)
    return f"REWARD  {summary}"


def _render_last_line(context: CompactHudContext) -> str:
    return f"LAST    {context.last_event_summary or '-'}"


def _lrs_coordinate_text(state: engine.GameState | None) -> str:
    if state is None:
        return "-"
    return f"({state.player_position[0]},{state.player_position[1]})"


def _sector_type_text(context: CompactHudContext) -> str:
    if context.srs_state is not None:
        return context.srs_state.descriptor.sector_type.value
    if context.lrs_state is None:
        return "-"
    symbol = context.lrs_state.known_cells.get(context.lrs_state.player_position)
    if symbol is None:
        return "UNKNOWN"
    return _LRS_SYMBOL_TO_SECTOR_TYPE.get(symbol, "UNKNOWN")


def _srs_coordinate_text(state: srs_model.SrsGameState | None) -> str:
    if state is None:
        return "-"
    return _display_position_text(state.player_position)


def _sensor_range_text(state: srs_model.SrsGameState | None) -> str:
    if state is None:
        return "-"
    if state.descriptor.sector_type is srs_model.SectorType.NEBULA:
        return "3x3"
    return "5x5"


def _cost_mode_text(context: CompactHudContext) -> str:
    if context.cost_mode is not None:
        return context.cost_mode
    return "-"


def _fuel_text(context: CompactHudContext) -> str:
    if context.srs_state is not None:
        return f"{context.srs_state.fuel}/{context.srs_state.max_fuel}"
    if context.lrs_state is not None:
        return f"{context.lrs_state.remaining_fuel}/{context.lrs_state.settings.max_fuel}"
    return "-"


def _status_text(context: CompactHudContext) -> str:
    if context.status is not None:
        return context.status
    if context.lrs_state is not None or context.srs_state is not None:
        return "EXPLORING"
    return "-"


def _select_enemy(combat_state: srs_model.SrsCombatState) -> srs_model.SrsEnemyCombatState:
    target_id = combat_state.player_attack_target_id
    if target_id is not None and target_id in combat_state.enemies:
        return combat_state.enemies[target_id]
    return next(iter(combat_state.enemies.values()))


def _warp_summary_text(state: srs_model.SrsGameState | None) -> str:
    if state is None:
        return "-"
    cell = state.known_state.known_cells.get(state.player_position)
    if cell is not None and cell.warp_flags:
        directions = _direction_text(cell.warp_flags)
        return f"{directions} available at SRS={_display_position_text(state.player_position)}"
    blocked_direction = _visible_blocked_direction(state)
    if blocked_direction is not None:
        return f"{blocked_direction} blocked by RIFT_BARRIER"
    return "-"


def _visible_blocked_direction(state: srs_model.SrsGameState) -> str | None:
    player = state.player_position
    for direction in _DIRECTION_ORDER:
        neighbor = _step_position(player, direction)
        if not state.actual_map.contains(neighbor):
            if direction in state.descriptor.blocked_edges:
                return direction.value
            continue
        if neighbor not in state.known_state.discovered_cells:
            continue
        if state.known_state.known_cells[neighbor].terrain is srs_model.SrsTerrainType.RIFT_BARRIER:
            return direction.value
    return None


def _reward_summary_text(state: srs_model.SrsGameState | None) -> str:
    if state is None:
        return "-"
    player_cell = state.known_state.known_cells.get(state.player_position)
    if player_cell is not None:
        player_reward = _visible_reward_object(state, player_cell.object_id)
        if player_reward is not None:
            return _reward_detection_text(player_reward)

    candidates: list[srs_model.SrsObjectState] = []
    for position in state.known_state.discovered_cells:
        cell = state.known_state.known_cells.get(position)
        if cell is None:
            continue
        reward = _visible_reward_object(state, cell.object_id)
        if reward is not None:
            candidates.append(reward)
    if not candidates:
        return "-"

    player = state.player_position
    reward = min(
        candidates,
        key=lambda item: (
            abs(item.position.x - player.x) + abs(item.position.y - player.y),
            item.object_id,
        ),
    )
    return _reward_detection_text(reward)


def _visible_reward_object(
    state: srs_model.SrsGameState,
    object_id: str | None,
) -> srs_model.SrsObjectState | None:
    if object_id is None or object_id not in state.objects:
        return None
    object_state = state.objects[object_id]
    if object_state.consumed:
        return None
    if object_state.object_type not in _REWARD_LABELS:
        return None
    return object_state


def _reward_detection_text(object_state: srs_model.SrsObjectState) -> str:
    label = _REWARD_LABELS[object_state.object_type]
    return f"{label} detected at SRS={_display_position_text(object_state.position)}"


def _display_position_text(position: srs_model.Position) -> str:
    display_x, display_y = to_display_position(position)
    return f"({display_x},{display_y})"


def _direction_text(directions: frozenset[srs_model.Direction]) -> str:
    return ",".join(direction.value for direction in _DIRECTION_ORDER if direction in directions)


def _step_position(
    position: srs_model.Position,
    direction: srs_model.Direction,
) -> srs_model.Position:
    deltas = {
        srs_model.Direction.N: (0, 1),
        srs_model.Direction.E: (1, 0),
        srs_model.Direction.S: (0, -1),
        srs_model.Direction.W: (-1, 0),
    }
    dx, dy = deltas[direction]
    return srs_model.Position(position.x + dx, position.y + dy)


def _pad(value: str, min_width: int) -> str:
    return value.ljust(max(min_width, len(value)))
