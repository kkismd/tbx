from __future__ import annotations

import json
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any, Mapping

from experiments.galactic_exodus.srs.contracts import SrsContracts, load_default_contracts
from experiments.galactic_exodus.srs.engine import restore_srs_state, reveal_full_observation, reveal_observation, run_srs_commands
from experiments.galactic_exodus.srs.generate import create_sector
from experiments.galactic_exodus.srs.log import build_srs_log
from experiments.galactic_exodus.srs.model import (
    CostMode,
    Direction,
    ObservationMode,
    Position,
    SrsCombatPhase,
    SrsCombatState,
    SrsEnemyTier,
    SrsActualMap,
    SrsCell,
    SectorDescriptor,
    SectorType,
    SrsCommand,
    SrsGameLog,
    SrsGameState,
    SrsPlayerCombatState,
    SrsPersistentState,
    SrsTerrainType,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.render import render_known_map


REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures"

_ALLOWED_COMMAND_KEYS = frozenset({"command_type", "route", "target", "target_object_id", "exit_direction"})
_ALLOWED_COMMAND_TYPES = frozenset({"MOVE_ROUTE", "MOVE_TO", "INTERACT", "WARP_EXIT", "COMBAT_STEP"})


class SrsFixtureError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class SrsFixtureRunResult:
    fixture_id: str
    initial_state: SrsGameState
    final_state: SrsGameState
    log: SrsGameLog
    render: str
    summary: Mapping[str, Any]


def load_fixture(path: Path) -> Mapping[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SrsFixtureError(f"missing fixture file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SrsFixtureError(f"invalid fixture JSON: {path}: {exc}") from exc
    except OSError as exc:
        raise SrsFixtureError(f"failed to read fixture file {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise SrsFixtureError("fixture JSON root must be an object")
    return payload


def command_from_json(data: Mapping[str, Any]) -> SrsCommand:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("command must be an object")

    unknown_fields = sorted(set(data) - _ALLOWED_COMMAND_KEYS)
    if unknown_fields:
        raise SrsFixtureError(f"unknown command field: {unknown_fields[0]}")

    command_type = _required_str(data, "command_type")
    if command_type not in _ALLOWED_COMMAND_TYPES:
        raise SrsFixtureError(f"unknown command_type: {command_type}")

    route: tuple[Direction, ...] = ()
    target: Position | None = None
    target_object_id: str | None = None
    exit_direction: Direction | None = None

    if "route" in data:
        route_value = data["route"]
        if not isinstance(route_value, list):
            raise SrsFixtureError("route must be a list")
        route = tuple(_direction(item) for item in route_value)
    if "target" in data:
        target = _position(data["target"], field_name="target")
    if "target_object_id" in data:
        target_object_id = _required_str(data, "target_object_id")
    if "exit_direction" in data:
        exit_direction = _direction(data["exit_direction"])

    try:
        return SrsCommand(
            command_type=command_type,
            route=route,
            target=target,
            target_object_id=target_object_id,
            exit_direction=exit_direction,
        )
    except ValueError as exc:
        raise SrsFixtureError(str(exc)) from exc


def state_from_fixture(data: Mapping[str, Any], *, contracts: SrsContracts) -> SrsGameState:
    descriptor = _sector_descriptor(data.get("sector"))
    state = create_sector(descriptor, contracts=contracts)

    initial = data.get("initial", {})
    if initial is None:
        initial = {}
    if not isinstance(initial, Mapping):
        raise SrsFixtureError("initial must be an object")

    cell_overrides = initial.get("cell_overrides")
    if cell_overrides is not None:
        state = _apply_cell_overrides(state, overrides=cell_overrides)

    reveal = initial.get("reveal")
    if reveal is not None:
        state = _apply_reveal(state, reveal=reveal, contracts=contracts)

    player_position = _optional_position(initial, "player_position") or state.player_position
    fuel = _optional_int(initial, "fuel", default=state.fuel)
    max_fuel = _optional_int(initial, "max_fuel", default=state.max_fuel)
    srs_turn = _optional_int(initial, "srs_turn", default=state.srs_turn)

    persistent = initial.get("persistent")
    if persistent is not None:
        state = _apply_persistent_overrides(
            state,
            persistent=persistent,
            player_position=player_position,
        )
    else:
        state = replace(state, player_position=player_position)

    combat = initial.get("combat")
    if combat is not None:
        state = replace(state, combat_state=_combat_state(combat))

    return replace(state, fuel=fuel, max_fuel=max_fuel, srs_turn=srs_turn)


def run_fixture(path: Path, *, contracts: SrsContracts | None = None) -> SrsFixtureRunResult:
    fixture_data = load_fixture(path)
    resolved_contracts = load_default_contracts(REPO_ROOT) if contracts is None else contracts
    return run_fixture_data(fixture_data, contracts=resolved_contracts)


def run_fixture_data(data: Mapping[str, Any], *, contracts: SrsContracts) -> SrsFixtureRunResult:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("fixture data must be an object")

    fixture_id = _required_str(data, "fixture_id")
    initial_state = state_from_fixture(data, contracts=contracts)
    commands = _commands_from_fixture(data)
    cost_mode = _fixture_cost_mode(data, contracts=contracts)

    command_result = run_srs_commands(initial_state, commands, contracts=contracts, cost_mode=cost_mode)
    final_state = command_result.state
    log = build_srs_log(command_result.events)
    rendered = render_known_map(final_state)
    summary = _build_summary(
        fixture_id=fixture_id,
        final_state=final_state,
        log=log,
        render=rendered,
        cost_mode=cost_mode,
    )
    result = SrsFixtureRunResult(
        fixture_id=fixture_id,
        initial_state=initial_state,
        final_state=final_state,
        log=log,
        render=rendered,
        summary=summary,
    )
    _validate_expectations(data.get("expect"), result)
    return result


def fixture_result_to_jsonable(result: SrsFixtureRunResult) -> Mapping[str, Any]:
    final_state = result.final_state
    return {
        "fixture_id": result.fixture_id,
        "final_state": {
            "srs_turn": final_state.srs_turn,
            "fuel": final_state.fuel,
            "max_fuel": final_state.max_fuel,
            "player_position": _position_to_list(final_state.player_position),
            "consumed_object_ids": sorted(final_state.persistent_state.consumed_object_ids),
            "activated_object_ids": sorted(final_state.persistent_state.activated_object_ids),
            "discovered_count": len(final_state.known_state.discovered_cells),
            "event_count": len(result.log.events),
            "combat_phase": final_state.combat_state.phase.value if final_state.combat_state is not None else None,
            "combat_turn": final_state.combat_state.combat_turn if final_state.combat_state is not None else None,
            "enemy_presence": final_state.combat_state.enemy_presence if final_state.combat_state is not None else False,
            "combat_player_energy": final_state.combat_state.player.energy if final_state.combat_state is not None else None,
        },
        "log": {
            "events": [
                {
                    "srs_turn": event.srs_turn,
                    "event_type": event.event_type,
                    "payload": dict(event.payload),
                }
                for event in result.log.events
            ]
        },
        "render": result.render,
        "summary": dict(result.summary),
    }


def _commands_from_fixture(data: Mapping[str, Any]) -> tuple[SrsCommand, ...]:
    commands = data.get("commands")
    if not isinstance(commands, list):
        raise SrsFixtureError("commands must be a list")
    return tuple(command_from_json(command) for command in commands)


def _fixture_cost_mode(data: Mapping[str, Any], *, contracts: SrsContracts) -> CostMode:
    raw_cost_mode = data.get("cost_mode")
    if raw_cost_mode is None:
        return contracts.movement.baseline_cost_mode
    try:
        return CostMode(raw_cost_mode)
    except ValueError as exc:
        raise SrsFixtureError(f"invalid cost_mode: {raw_cost_mode}") from exc


def _sector_descriptor(data: Any) -> SectorDescriptor:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("sector must be an object")
    return SectorDescriptor(
        sector_id=_required_str(data, "sector_id"),
        sector_type=_sector_type(data.get("sector_type")),
        sector_seed=_required_int(data, "sector_seed"),
        entry_edge=_direction(data.get("entry_edge")),
        blocked_edges=frozenset(_direction(value) for value in _required_list(data, "blocked_edges")),
    )


def _apply_reveal(state: SrsGameState, *, reveal: Any, contracts: SrsContracts) -> SrsGameState:
    if not isinstance(reveal, Mapping):
        raise SrsFixtureError("initial.reveal must be an object")
    mode = reveal.get("mode")
    if mode == ObservationMode.FULL.value:
        return reveal_full_observation(state)
    if mode == ObservationMode.LOCAL_MOVEMENT.value:
        return reveal_observation(state, center=state.player_position, contracts=contracts)
    raise SrsFixtureError(f"invalid reveal mode: {mode}")


def _apply_cell_overrides(state: SrsGameState, *, overrides: Any) -> SrsGameState:
    if not isinstance(overrides, list):
        raise SrsFixtureError("initial.cell_overrides must be a list")

    rows = [list(row) for row in state.actual_map.cells]
    for index, override in enumerate(overrides, 1):
        if not isinstance(override, Mapping):
            raise SrsFixtureError(f"initial.cell_overrides[{index}] must be an object")
        position = _position(override.get("position"), field_name=f"initial.cell_overrides[{index}].position")
        if not state.actual_map.contains(position):
            raise SrsFixtureError(f"initial.cell_overrides[{index}].position out of bounds: {position}")
        terrain = _terrain_type(override.get("terrain"), field_name=f"initial.cell_overrides[{index}].terrain")
        cell = rows[position.y][position.x]
        rows[position.y][position.x] = SrsCell(
            terrain=terrain,
            object_id=cell.object_id,
            actor_id=cell.actor_id,
            warp_flags=cell.warp_flags,
        )

    actual_map = SrsActualMap(
        width=state.actual_map.width,
        height=state.actual_map.height,
        cells=tuple(tuple(row) for row in rows),
    )
    return replace(state, actual_map=actual_map)


def _apply_persistent_overrides(
    state: SrsGameState,
    *,
    persistent: Any,
    player_position: Position,
) -> SrsGameState:
    if not isinstance(persistent, Mapping):
        raise SrsFixtureError("initial.persistent must be an object")

    consumed_object_ids = _optional_str_list(
        persistent,
        "consumed_object_ids",
        default=sorted(state.persistent_state.consumed_object_ids),
    )
    activated_object_ids = _optional_str_list(
        persistent,
        "activated_object_ids",
        default=sorted(state.persistent_state.activated_object_ids),
    )
    discovered_positions = _optional_position_list(
        persistent,
        "discovered_cells",
        default=sorted(state.persistent_state.discovered_cells, key=lambda pos: (pos.y, pos.x)),
    )

    restored = restore_srs_state(
        descriptor=state.descriptor,
        actual_map=state.actual_map,
        persistent=replace(
            state.persistent_state,
            consumed_object_ids=frozenset(consumed_object_ids),
            activated_object_ids=frozenset(activated_object_ids),
            discovered_cells=frozenset(discovered_positions),
        ),
        player_position=player_position,
        objects=state.objects,
    )
    return replace(
        restored,
        fuel=state.fuel,
        max_fuel=state.max_fuel,
        srs_turn=state.srs_turn,
    )


def _combat_state(data: Any) -> SrsCombatState:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("initial.combat must be an object")

    raw_phase = data.get("phase", SrsCombatPhase.PLAYER_MOVEMENT.value)
    try:
        phase = SrsCombatPhase(raw_phase)
    except ValueError as exc:
        raise SrsFixtureError(f"invalid combat phase: {raw_phase}") from exc

    combat_turn = _optional_int(data, "combat_turn", default=0)
    player_data = data.get("player", {})
    if player_data is None:
        player_data = {}
    if not isinstance(player_data, Mapping):
        raise SrsFixtureError("initial.combat.player must be an object")

    player = SrsPlayerCombatState(
        durability=_optional_int(player_data, "durability", default=100),
        defense=_optional_int(player_data, "defense", default=0),
        movement_power=_optional_int(player_data, "movement_power", default=4),
        photon_torpedo_ammo=_optional_int(player_data, "photon_torpedo_ammo", default=6),
        photon_torpedo_ammo_capacity=_optional_int(player_data, "photon_torpedo_ammo_capacity", default=6),
        energy=_optional_int(player_data, "energy", default=6),
        energy_capacity=_optional_int(player_data, "energy_capacity", default=6),
        energy_recovery=_optional_int(player_data, "energy_recovery", default=1),
    )

    raw_enemies = data.get("enemies", [])
    if not isinstance(raw_enemies, list):
        raise SrsFixtureError("initial.combat.enemies must be a list")
    enemies = {}
    for index, raw_enemy in enumerate(raw_enemies, 1):
        if not isinstance(raw_enemy, Mapping):
            raise SrsFixtureError(f"initial.combat.enemies[{index}] must be an object")
        enemy_id = _required_str(raw_enemy, "enemy_id")
        raw_tier = _required_str(raw_enemy, "tier")
        try:
            tier = SrsEnemyTier(raw_tier)
        except ValueError as exc:
            raise SrsFixtureError(f"invalid enemy tier: {raw_tier}") from exc
        enemies[enemy_id] = create_enemy_combat_state(
            enemy_id=enemy_id,
            tier=tier,
            position=_position(raw_enemy.get("position"), field_name=f"initial.combat.enemies[{index}].position"),
        )

    player_attack_target_id = data.get("player_attack_target_id")
    if player_attack_target_id is not None and not isinstance(player_attack_target_id, str):
        raise SrsFixtureError("initial.combat.player_attack_target_id must be a string")

    try:
        return SrsCombatState(
            player=player,
            enemies=enemies,
            phase=phase,
            combat_turn=combat_turn,
            player_attack_target_id=player_attack_target_id,
        )
    except ValueError as exc:
        raise SrsFixtureError(str(exc)) from exc


def _build_summary(
    *,
    fixture_id: str,
    final_state: SrsGameState,
    log: SrsGameLog,
    render: str,
    cost_mode: CostMode,
) -> Mapping[str, Any]:
    primary_outcome = None
    if log.events:
        primary_outcome = log.events[0].payload.get("outcome")
    return {
        "fixture_id": fixture_id,
        "cost_mode": cost_mode.value,
        "srs_turn": final_state.srs_turn,
        "fuel": final_state.fuel,
        "max_fuel": final_state.max_fuel,
        "player_position": _position_to_list(final_state.player_position),
        "event_types": [event.event_type for event in log.events],
        "event_count": len(log.events),
        "consumed_object_ids": sorted(final_state.persistent_state.consumed_object_ids),
        "activated_object_ids": sorted(final_state.persistent_state.activated_object_ids),
        "discovered_count": len(final_state.known_state.discovered_cells),
        "combat_phase": final_state.combat_state.phase.value if final_state.combat_state is not None else None,
        "combat_turn": final_state.combat_state.combat_turn if final_state.combat_state is not None else None,
        "enemy_presence": final_state.combat_state.enemy_presence if final_state.combat_state is not None else False,
        "combat_player_energy": final_state.combat_state.player.energy if final_state.combat_state is not None else None,
        "outcome": primary_outcome,
        "render_line_count": len(render.splitlines()),
    }


def _validate_expectations(expect: Any, result: SrsFixtureRunResult) -> None:
    if expect is None:
        return
    if not isinstance(expect, Mapping):
        raise SrsFixtureError("expect must be an object")

    final_state = result.final_state
    event_types = [event.event_type for event in result.log.events]
    comparisons = (
        ("srs_turn", final_state.srs_turn),
        ("fuel", final_state.fuel),
        ("player_position", _position_to_list(final_state.player_position)),
        ("event_types", event_types),
        ("consumed_object_ids", sorted(final_state.persistent_state.consumed_object_ids)),
        ("activated_object_ids", sorted(final_state.persistent_state.activated_object_ids)),
        ("combat_phase", final_state.combat_state.phase.value if final_state.combat_state is not None else None),
        ("combat_turn", final_state.combat_state.combat_turn if final_state.combat_state is not None else None),
        ("enemy_presence", final_state.combat_state.enemy_presence if final_state.combat_state is not None else False),
        ("combat_player_energy", final_state.combat_state.player.energy if final_state.combat_state is not None else None),
        ("outcome", result.summary.get("outcome")),
    )
    for field_name, actual in comparisons:
        if field_name in expect and expect[field_name] != actual:
            raise SrsFixtureError(f"expect mismatch for {field_name}: expected {expect[field_name]!r}, got {actual!r}")

    render_contains = expect.get("render_contains")
    if render_contains is not None:
        for needle in _normalize_expected_strings(render_contains, field_name="render_contains"):
            if needle not in result.render:
                raise SrsFixtureError(f"expect mismatch for render_contains: missing {needle!r}")

    render_not_contains = expect.get("render_not_contains")
    if render_not_contains is not None:
        for needle in _normalize_expected_strings(render_not_contains, field_name="render_not_contains"):
            if needle in result.render:
                raise SrsFixtureError(f"expect mismatch for render_not_contains: found {needle!r}")


def _normalize_expected_strings(value: Any, *, field_name: str) -> list[str]:
    if isinstance(value, str):
        return [value]
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        raise SrsFixtureError(f"{field_name} must be a string or list of strings")
    return list(value)


def _required_str(mapping: Mapping[str, Any], field_name: str) -> str:
    value = mapping.get(field_name)
    if not isinstance(value, str) or value == "":
        raise SrsFixtureError(f"required field missing or invalid: {field_name}")
    return value


def _required_int(mapping: Mapping[str, Any], field_name: str) -> int:
    value = mapping.get(field_name)
    if not isinstance(value, int) or isinstance(value, bool):
        raise SrsFixtureError(f"required field missing or invalid: {field_name}")
    return value


def _required_list(mapping: Mapping[str, Any], field_name: str) -> list[Any]:
    value = mapping.get(field_name)
    if not isinstance(value, list):
        raise SrsFixtureError(f"required field missing or invalid: {field_name}")
    return value


def _optional_int(mapping: Mapping[str, Any], field_name: str, *, default: int) -> int:
    if field_name not in mapping:
        return default
    value = mapping[field_name]
    if not isinstance(value, int) or isinstance(value, bool):
        raise SrsFixtureError(f"{field_name} must be an integer")
    return value


def _optional_position(mapping: Mapping[str, Any], field_name: str) -> Position | None:
    if field_name not in mapping:
        return None
    return _position(mapping[field_name], field_name=field_name)


def _optional_str_list(mapping: Mapping[str, Any], field_name: str, *, default: list[str]) -> list[str]:
    if field_name not in mapping:
        return default
    value = mapping[field_name]
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        raise SrsFixtureError(f"{field_name} must be a list of strings")
    return list(value)


def _optional_position_list(mapping: Mapping[str, Any], field_name: str, *, default: list[Position]) -> list[Position]:
    if field_name not in mapping:
        return default
    value = mapping[field_name]
    if not isinstance(value, list):
        raise SrsFixtureError(f"{field_name} must be a list")
    return [_position(item, field_name=field_name) for item in value]


def _position(value: Any, *, field_name: str) -> Position:
    if not isinstance(value, list) or len(value) != 2:
        raise SrsFixtureError(f"{field_name} must be a [x, y] list")
    x, y = value
    if not isinstance(x, int) or isinstance(x, bool) or not isinstance(y, int) or isinstance(y, bool):
        raise SrsFixtureError(f"{field_name} must contain integer coordinates")
    return Position(x, y)


def _position_to_list(position: Position) -> list[int]:
    return [position.x, position.y]


def _direction(value: Any) -> Direction:
    try:
        return Direction(value)
    except ValueError as exc:
        raise SrsFixtureError(f"invalid Direction string: {value}") from exc


def _sector_type(value: Any) -> SectorType:
    try:
        return SectorType(value)
    except ValueError as exc:
        raise SrsFixtureError(f"invalid sector_type: {value}") from exc


def _terrain_type(value: Any, *, field_name: str) -> SrsTerrainType:
    try:
        return SrsTerrainType(value)
    except ValueError as exc:
        raise SrsFixtureError(f"invalid {field_name}: {value}") from exc


def _print_cli_result(result: SrsFixtureRunResult) -> None:
    print(result.fixture_id)
    print(json.dumps(result.summary, ensure_ascii=False, sort_keys=True))
    print(result.render)


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("fixture", type=Path)
    args = parser.parse_args()

    result = run_fixture(args.fixture)
    _print_cli_result(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
