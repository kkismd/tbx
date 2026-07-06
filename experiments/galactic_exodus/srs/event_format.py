from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any

from experiments.galactic_exodus.srs import log
from experiments.galactic_exodus.srs.model import Position
from experiments.galactic_exodus.srs.render import to_display_position


def format_srs_event_summary(event: log.SrsTurnEvent) -> str:
    """Format one SRS turn event for normal user-facing output."""
    lines = format_srs_event_summary_lines(event)
    return lines[0]


def format_srs_debug_event(event: log.SrsTurnEvent) -> str:
    """Format one SRS turn event for debug/manual inspection."""
    payload = dict(event.payload)
    tokens = [event.event_type]
    tokens.extend(_debug_position_tokens(payload))
    tokens.extend(_compact_payload_tokens(payload))
    return " ".join(tokens)


def format_srs_event_summary_lines(event: log.SrsTurnEvent) -> list[str]:
    """Return one or more normal summary lines for an SRS event."""
    payload = dict(event.payload)
    event_type = event.event_type

    if event_type == log.MOVE_ACCEPTED:
        return [_format_move_accepted(payload)]
    if event_type == log.MOVE_REJECTED:
        return [_format_rejected("MOVE", payload)]
    if event_type == log.STOPPED_BEFORE_IMPASSABLE:
        return [_format_stopped(payload)]
    if event_type == log.OBSERVATION_UPDATED:
        return _format_observation(payload)
    if event_type == log.INTERACT_ACCEPTED:
        return [_format_interact_accepted(payload)]
    if event_type == log.INTERACT_REJECTED:
        return [_format_rejected("INTERACT", payload)]
    if event_type == log.OBJECT_CONSUMED:
        return [_format_object_consumed(payload)]
    if event_type == log.STATION_ACTIVATED:
        return _format_station_activated(payload)
    if event_type == log.WARP_EXIT_ACCEPTED:
        return [_format_warp_accepted(payload)]
    if event_type == log.WARP_EXIT_REJECTED:
        return [_format_warp_rejected(payload)]
    if event_type == log.COMBAT_TRANSITIONED:
        return [_format_combat_transitioned(payload)]
    if event_type == log.COMBAT_REJECTED:
        return [_format_rejected("COMBAT", payload)]
    if event_type == log.ENCOUNTER_ROLLED:
        return [_format_encounter(payload)]
    return [f"EVENT {event_type}"]


def _format_move_accepted(payload: Mapping[str, Any]) -> str:
    parts = ["MOVE  accepted"]
    route = _route_text(payload.get("resolved_route") or payload.get("route"))
    if route is not None:
        parts.append(f"route={route}")
    destination = _display_position_text(
        payload.get("target_position")
        or payload.get("destination")
        or payload.get("player_position_after")
        or payload.get("end_position")
        or payload.get("to")
    )
    if destination is not None:
        parts.append(f"to SRS={destination}")
    return " ".join(parts)


def _format_rejected(prefix: str, payload: Mapping[str, Any]) -> str:
    reason = _reason_text(payload)
    if reason is None:
        return f"{prefix}  rejected"
    return f"{prefix}  rejected: {reason}"


def _format_stopped(payload: Mapping[str, Any]) -> str:
    terrain = payload.get("terrain")
    position = _display_position_text(payload.get("blocked_position") or payload.get("position"))
    if terrain is not None and position is not None:
        return f"STOP  blocked by {terrain} at SRS={position}"
    if position is not None:
        return f"STOP  blocked at SRS={position}"
    if terrain is not None:
        return f"STOP  blocked by {terrain}"
    return "STOP  blocked"


def _format_observation(payload: Mapping[str, Any]) -> list[str]:
    size = _observation_size_text(payload)
    update_line = (
        f"SCAN  {size} update: +{payload.get('newly_discovered_count', '-')} known cells, "
        f"total={payload.get('total_discovered_count', '-')}"
    )
    if payload.get("nebula_interference"):
        return [
            "SCAN  NEBULA interference: sensor range reduced to 3x3",
            update_line,
        ]
    return [update_line]


def _format_interact_accepted(payload: Mapping[str, Any]) -> str:
    object_type = payload.get("object_type")
    position = _display_position_text(payload.get("position"))
    if object_type is None:
        return "INTERACT accepted"
    if position is None:
        return f"INTERACT accepted: {object_type}"
    return f"INTERACT accepted: {object_type} at SRS={position}"


def _format_object_consumed(payload: Mapping[str, Any]) -> str:
    object_type = payload.get("object_type")
    position = _display_position_text(payload.get("position"))
    if object_type == "RESOURCE_CACHE":
        fuel_delta = payload.get("fuel_delta")
        fuel_after = payload.get("fuel_after")
        fuel_capacity = payload.get("max_fuel") or payload.get("fuel_capacity")
        target = _with_capacity(fuel_after, fuel_capacity)
        if fuel_delta is not None and fuel_after is not None:
            return f"CACHE acquired: fuel +{fuel_delta} -> {target}"
    if object_type == "SALVAGE":
        salvage_after = payload.get("salvage_after")
        durability_delta = payload.get("durability_delta")
        durability_after = payload.get("durability_after")
        durability_capacity = payload.get("durability_capacity")
        if salvage_after is not None and durability_delta is not None and durability_after is not None:
            durability_text = _with_capacity(durability_after, durability_capacity)
            return (
                "SALVAGE acquired: "
                f"+1 inventory, durability +{durability_delta} -> {durability_text}"
            )
    if object_type is not None and position is not None:
        return f"OBJECT consumed: {object_type} at SRS={position}"
    if object_type is not None:
        return f"OBJECT consumed: {object_type}"
    return "OBJECT consumed"


def _format_station_activated(payload: Mapping[str, Any]) -> list[str]:
    lines = ["BASE station activated: full recovery complete"]
    applied_upgrade = payload.get("applied_upgrade")
    salvage_before = payload.get("salvage_before")
    salvage_after = payload.get("salvage_after")
    if applied_upgrade is not None and salvage_before is not None and salvage_after is not None:
        upgrade_label = _upgrade_label(str(applied_upgrade))
        lines.append(f"UPGRADE {upgrade_label}, salvage {salvage_before} -> {salvage_after}")
    return lines


def _format_warp_accepted(payload: Mapping[str, Any]) -> str:
    direction = payload.get("exit_direction", "-")
    start = _display_position_text(payload.get("start_position") or payload.get("from_position"))
    if start is None:
        return f"WARP  {direction} accepted"
    return f"WARP  {direction} accepted from SRS={start}"


def _format_warp_rejected(payload: Mapping[str, Any]) -> str:
    reason = _reason_text(payload)
    if reason is None:
        return "WARP  rejected"
    return f"WARP  rejected: {reason}"


def _format_combat_transitioned(payload: Mapping[str, Any]) -> str:
    phase = payload.get("phase_to") or payload.get("phase") or payload.get("phase_from")
    summary = f"COMBAT phase={phase}" if phase is not None else "COMBAT"
    player_action = payload.get("player_action")
    target_id = None
    if isinstance(player_action, Mapping):
        target_id = player_action.get("target_enemy_id")
    target_position = _display_position_text(
        payload.get("target_position")
        or payload.get("enemy_position")
        or payload.get("position")
    )
    if target_id is not None and target_position is not None:
        return f"{summary} target={target_id} at SRS={target_position}"
    return summary


def _format_encounter(payload: Mapping[str, Any]) -> str:
    roll = payload.get("roll") or payload.get("encounter_roll") or payload.get("actual_roll")
    threshold = (
        payload.get("threshold")
        or payload.get("actual_encounter_chance")
        or payload.get("encounter_threshold")
    )
    prefix = "ENCOUNTER checked"
    if roll is not None and threshold is not None:
        prefix = f"ENCOUNTER roll={_format_number(roll)} threshold={_format_number(threshold)}"
    spawned_enemy = _spawned_enemy_text(payload)
    if spawned_enemy is not None:
        return f"{prefix} -> spawned {spawned_enemy}"
    return f"{prefix} -> none"


def _debug_position_tokens(payload: Mapping[str, Any]) -> list[str]:
    pairs = (
        ("position", "position"),
        ("start_position", "start"),
        ("end_position", "end"),
        ("blocked_position", "blocked"),
        ("target_position", "target"),
        ("center", "center"),
    )
    tokens: list[str] = []
    for key, label in pairs:
        if key not in payload:
            continue
        internal = _internal_position_text(payload.get(key))
        display = _display_position_text(payload.get(key))
        if internal is None or display is None:
            continue
        tokens.append(f"{label}_internal={internal}")
        tokens.append(f"{label}_display={display}")
    return tokens


def _compact_payload_tokens(payload: Mapping[str, Any]) -> list[str]:
    tokens: list[str] = []
    for key in sorted(payload):
        value = payload[key]
        if value is None:
            continue
        if key in {
            "position",
            "start_position",
            "end_position",
            "blocked_position",
            "target_position",
            "center",
        }:
            continue
        if isinstance(value, Mapping):
            tokens.append(f"{key}={_mapping_debug_summary(value)}")
            continue
        if isinstance(value, list):
            tokens.append(f"{key}={_list_debug_summary(value)}")
            continue
        tokens.append(f"{key}={value}")
    return tokens


def _mapping_debug_summary(mapping: Mapping[str, Any]) -> str:
    compact_parts: list[str] = []
    for key in sorted(mapping):
        value = mapping[key]
        if isinstance(value, Mapping):
            compact_parts.append(f"{key}=...")
        elif isinstance(value, list):
            compact_parts.append(f"{key}=[{len(value)}]")
        else:
            compact_parts.append(f"{key}={value}")
    return "{" + ", ".join(compact_parts) + "}"


def _list_debug_summary(values: Sequence[Any]) -> str:
    if len(values) <= 4 and all(not isinstance(value, Mapping) for value in values):
        return "[" + ",".join(str(value) for value in values) + "]"
    return f"[{len(values)}]"


def _observation_size_text(payload: Mapping[str, Any]) -> str:
    size = payload.get("sensor_range") or payload.get("observation_size") or payload.get("size")
    if size == 3:
        return "3x3"
    return "5x5"


def _route_text(value: object) -> str | None:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        return None
    items = [str(item) for item in value]
    if not items:
        return None
    return ",".join(items)


def _reason_text(payload: Mapping[str, Any]) -> str | None:
    explicit_reason = payload.get("reason")
    if explicit_reason:
        return str(explicit_reason)
    outcome = payload.get("outcome")
    if outcome == "REJECTED_BLOCKED_EDGE":
        direction = payload.get("exit_direction", "-")
        return f"{direction} edge is blocked by RIFT_BARRIER"
    if outcome == "REJECTED_NO_WARP_FLAG":
        direction = payload.get("exit_direction", "-")
        return f"{direction} edge has no warp exit"
    if outcome == "REJECTED_ENEMY_PRESENCE":
        return "enemy presence blocks warp exit"
    if outcome == "REJECTED_ALREADY_CONSUMED":
        return "already consumed"
    if outcome == "REJECTED_NO_EFFECT":
        return "no effect"
    if outcome == "REJECTED_OUT_OF_BOUNDS":
        return "out of bounds"
    if outcome == "REJECTED_UNKNOWN_TARGET":
        return "unknown target"
    if outcome == "REJECTED_SAME_POSITION":
        return "same position"
    if outcome == "REJECTED_UPGRADE_UNAVAILABLE":
        return "upgrade unavailable"
    if isinstance(outcome, str) and outcome.startswith("REJECTED_"):
        return outcome.removeprefix("REJECTED_").replace("_", " ").lower()
    return None


def _spawned_enemy_text(payload: Mapping[str, Any]) -> str | None:
    enemy_id = payload.get("enemy_id")
    enemy_tier = payload.get("enemy_tier")
    spawn_position = (
        _display_position_text(payload.get("spawn_position"))
        or _display_position_text(payload.get("enemy_position"))
        or _display_position_text(payload.get("position"))
    )
    if enemy_id is None and enemy_tier is None and spawn_position is None:
        return None
    tokens = [str(enemy_id or "enemy")]
    if enemy_tier is not None:
        tokens.append(str(enemy_tier))
    if spawn_position is not None:
        tokens.append(f"at SRS={spawn_position}")
    return " ".join(tokens)


def _upgrade_label(applied_upgrade: str) -> str:
    labels = {
        "DEFENSE": "defense +1",
        "EVASION": "evasion +1",
        "PHASER_POWER": "phaser +1",
        "PHOTON_TORPEDO_POWER": "torpedo +1",
        "ENERGY_CAPACITY": "energy capacity +1",
        "PHOTON_TORPEDO_AMMO_CAPACITY": "torpedo capacity +1",
    }
    return labels.get(applied_upgrade, applied_upgrade.lower())


def _with_capacity(value: object, capacity: object) -> str:
    if capacity is None:
        return str(value)
    return f"{value}/{capacity}"


def _format_number(value: object) -> str:
    if isinstance(value, float):
        return f"{value:.2f}"
    return str(value)


def _display_position_text(value: object) -> str | None:
    position = _coerce_position(value)
    if position is None:
        return None
    display_x, display_y = to_display_position(position)
    return f"({display_x},{display_y})"


def _internal_position_text(value: object) -> str | None:
    position = _coerce_position(value)
    if position is None:
        return None
    return f"({position.x},{position.y})"


def _coerce_position(value: object) -> Position | None:
    if isinstance(value, Position):
        return value
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes)) and len(value) == 2:
        x, y = value
        if isinstance(x, int) and isinstance(y, int):
            return Position(x, y)
    return None
