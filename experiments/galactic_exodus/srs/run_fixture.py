from __future__ import annotations

import json
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any, Mapping

from experiments.galactic_exodus.srs.contracts import SrsContracts, load_default_contracts
from experiments.galactic_exodus.srs.encounter import (
    EncounterRollDisposition,
    FixedEncounterRoll,
    combat_state_from_fixed_encounter,
    encounter_roll_disposition,
    resolve_fixed_encounter_roll,
)
from experiments.galactic_exodus.srs.engine import apply_srs_command, restore_srs_state, reveal_full_observation, reveal_observation
from experiments.galactic_exodus.srs.generate import create_sector
from experiments.galactic_exodus.srs.log import build_srs_log
from experiments.galactic_exodus.srs.model import (
    SrsBaseUpgrade,
    CostMode,
    Direction,
    ObservationMode,
    Position,
    SrsCombatPhase,
    SrsCombatState,
    SrsEnemyTier,
    SrsEnemyReaction,
    SrsActualMap,
    SrsCell,
    SrsPlayerAttackAction,
    SectorDescriptor,
    SectorType,
    SrsCommand,
    SrsGameLog,
    SrsGameState,
    SrsSalvageChoice,
    SrsPlayerCombatState,
    SrsPersistentState,
    SrsTerrainType,
    SrsWeaponType,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.render import render_known_map


REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures"

_ALLOWED_COMMAND_KEYS = frozenset(
    {
        "command_type",
        "route",
        "target",
        "target_object_id",
        "exit_direction",
        "player_attack_action",
        "player_attack_weapon",
        "enemy_reactions",
        "salvage_choice",
        "base_upgrade_choice",
        "encounter_roll",
    }
)
_ALLOWED_COMMAND_TYPES = frozenset({"MOVE_ROUTE", "MOVE_TO", "INTERACT", "WARP_EXIT", "COMBAT_STEP", "WAIT"})


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


@dataclass(frozen=True, slots=True)
class FixtureCommandPlan:
    command: SrsCommand
    encounter_roll: FixedEncounterRoll | None = None


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
    player_attack_action: SrsPlayerAttackAction | None = None
    player_attack_weapon: SrsWeaponType | None = None
    enemy_reactions: Mapping[str, SrsEnemyReaction] = {}
    salvage_choice: SrsSalvageChoice | None = None
    base_upgrade_choice: SrsBaseUpgrade | None = None

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
    if "player_attack_action" in data:
        try:
            player_attack_action = SrsPlayerAttackAction(_required_str(data, "player_attack_action"))
        except ValueError as exc:
            raise SrsFixtureError(f"invalid player_attack_action: {data['player_attack_action']}") from exc
    if "player_attack_weapon" in data:
        try:
            player_attack_weapon = SrsWeaponType(_required_str(data, "player_attack_weapon"))
        except ValueError as exc:
            raise SrsFixtureError(f"invalid player_attack_weapon: {data['player_attack_weapon']}") from exc
    if "enemy_reactions" in data:
        raw_enemy_reactions = data["enemy_reactions"]
        if not isinstance(raw_enemy_reactions, Mapping):
            raise SrsFixtureError("enemy_reactions must be an object")
        enemy_reactions = {}
        for enemy_id, reaction in raw_enemy_reactions.items():
            if not isinstance(enemy_id, str) or enemy_id == "":
                raise SrsFixtureError("enemy_reactions keys must be non-empty strings")
            if not isinstance(reaction, str):
                raise SrsFixtureError("enemy_reactions values must be strings")
            try:
                enemy_reactions[enemy_id] = SrsEnemyReaction(reaction)
            except ValueError as exc:
                raise SrsFixtureError(f"invalid enemy reaction: {reaction}") from exc
    if "salvage_choice" in data:
        try:
            salvage_choice = SrsSalvageChoice(_required_str(data, "salvage_choice"))
        except ValueError as exc:
            raise SrsFixtureError(f"invalid salvage_choice: {data['salvage_choice']}") from exc
    if "base_upgrade_choice" in data:
        try:
            base_upgrade_choice = SrsBaseUpgrade(_required_str(data, "base_upgrade_choice"))
        except ValueError as exc:
            raise SrsFixtureError(f"invalid base_upgrade_choice: {data['base_upgrade_choice']}") from exc

    try:
        return SrsCommand(
            command_type=command_type,
            route=route,
            target=target,
            target_object_id=target_object_id,
            exit_direction=exit_direction,
            player_attack_action=player_attack_action,
            player_attack_weapon=player_attack_weapon,
            enemy_reactions=enemy_reactions,
            salvage_choice=salvage_choice,
            base_upgrade_choice=base_upgrade_choice,
        )
    except ValueError as exc:
        raise SrsFixtureError(str(exc)) from exc


def fixture_command_plan_from_json(data: Mapping[str, Any]) -> FixtureCommandPlan:
    command = command_from_json(data)
    encounter_roll = _encounter_roll_from_json(data.get("encounter_roll"))
    return FixtureCommandPlan(command=command, encounter_roll=encounter_roll)


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
    player_state = _player_state(initial.get("player"))

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
        state = replace(state, player_state=player_state, combat_state=_combat_state(state, combat, player_state=player_state))

    return replace(state, fuel=fuel, max_fuel=max_fuel, srs_turn=srs_turn, player_state=player_state)


def run_fixture(path: Path, *, contracts: SrsContracts | None = None) -> SrsFixtureRunResult:
    fixture_data = load_fixture(path)
    resolved_contracts = load_default_contracts(REPO_ROOT) if contracts is None else contracts
    return run_fixture_data(fixture_data, contracts=resolved_contracts)


def run_fixture_data(data: Mapping[str, Any], *, contracts: SrsContracts) -> SrsFixtureRunResult:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("fixture data must be an object")

    fixture_id = _required_str(data, "fixture_id")
    initial_state = state_from_fixture(data, contracts=contracts)
    command_plans = _commands_from_fixture(data)
    cost_mode = _fixture_cost_mode(data, contracts=contracts)

    current_state = initial_state
    all_events: list[Any] = []
    for index, command_plan in enumerate(command_plans, start=1):
        result = apply_srs_command(current_state, command_plan.command, contracts=contracts, cost_mode=cost_mode)
        all_events.extend(result.events)
        disposition = encounter_roll_disposition(
            current_state,
            command_type=command_plan.command.command_type,
            next_state=result.state,
        )
        if command_plan.encounter_roll is None:
            current_state = result.state
            continue
        if disposition is EncounterRollDisposition.REQUIRED:
            result_state, encounter_event = resolve_fixed_encounter_roll(
                result.state,
                command_type=command_plan.command.command_type,
                roll=command_plan.encounter_roll,
            )
            current_state = result_state
            all_events.append(encounter_event)
            continue
        if command_plan.encounter_roll is not None:
            raise SrsFixtureError(
                f"commands[{index}].encounter_roll is not allowed when encounter disposition is {disposition.value}"
            )
        current_state = result.state

    final_state = current_state
    log = build_srs_log(tuple(all_events))
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
    enemy_positions = _enemy_positions_payload(final_state)
    enemy_actions = _flatten_enemy_actions(result.log)
    return {
        "fixture_id": result.fixture_id,
        "final_state": {
            "srs_turn": final_state.srs_turn,
            "fuel": final_state.fuel,
            "max_fuel": final_state.max_fuel,
            "player_position": _position_to_list(final_state.player_position),
            "player_durability": final_state.player_state.durability,
            "player_energy": final_state.player_state.energy,
            "player_torpedo_ammo": final_state.player_state.photon_torpedo_ammo,
            "player_salvage": final_state.player_state.salvage,
            "player_energy_capacity": final_state.player_state.energy_capacity,
            "player_torpedo_ammo_capacity": final_state.player_state.photon_torpedo_ammo_capacity,
            "player_phaser_power": final_state.player_state.phaser_power,
            "player_photon_torpedo_power": final_state.player_state.photon_torpedo_power,
            "player_defense": final_state.player_state.defense,
            "player_evasion": final_state.player_state.evasion,
            "consumed_object_ids": sorted(final_state.persistent_state.consumed_object_ids),
            "activated_object_ids": sorted(final_state.persistent_state.activated_object_ids),
            "discovered_count": len(final_state.known_state.discovered_cells),
            "event_count": len(result.log.events),
            "combat_phase": final_state.combat_state.phase.value if final_state.combat_state is not None else None,
            "combat_turn": final_state.combat_state.combat_turn if final_state.combat_state is not None else None,
            "enemy_presence": final_state.combat_state.enemy_presence if final_state.combat_state is not None else False,
            "combat_player_energy": final_state.combat_state.player.energy if final_state.combat_state is not None else None,
            "combat_enemy_positions": enemy_positions,
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
        "summary": dict(result.summary) | {"enemy_actions": enemy_actions},
    }


def _commands_from_fixture(data: Mapping[str, Any]) -> tuple[FixtureCommandPlan, ...]:
    commands = data.get("commands")
    if not isinstance(commands, list):
        raise SrsFixtureError("commands must be a list")
    return tuple(fixture_command_plan_from_json(command) for command in commands)


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
    objects = dict(state.objects)
    for index, override in enumerate(overrides, 1):
        if not isinstance(override, Mapping):
            raise SrsFixtureError(f"initial.cell_overrides[{index}] must be an object")
        position = _position(override.get("position"), field_name=f"initial.cell_overrides[{index}].position")
        if not state.actual_map.contains(position):
            raise SrsFixtureError(f"initial.cell_overrides[{index}].position out of bounds: {position}")
        terrain = _terrain_type(override.get("terrain"), field_name=f"initial.cell_overrides[{index}].terrain")
        cell = rows[position.y][position.x]
        object_id = cell.object_id
        if "object_id" in override:
            raw_object_id = override["object_id"]
            if raw_object_id is not None and not isinstance(raw_object_id, str):
                raise SrsFixtureError(f"initial.cell_overrides[{index}].object_id must be a string or null")
            if raw_object_id is not None and raw_object_id != cell.object_id:
                raise SrsFixtureError(
                    f"initial.cell_overrides[{index}].object_id may only preserve the current object or clear it with null"
                )
            object_id = raw_object_id
            if object_id is None and cell.object_id is not None:
                objects.pop(cell.object_id, None)
        rows[position.y][position.x] = SrsCell(
            terrain=terrain,
            object_id=object_id,
            actor_id=cell.actor_id,
            warp_flags=cell.warp_flags,
        )

    actual_map = SrsActualMap(
        width=state.actual_map.width,
        height=state.actual_map.height,
        cells=tuple(tuple(row) for row in rows),
    )
    return replace(state, actual_map=actual_map, objects=objects)


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


def _combat_state(state: SrsGameState, data: Any, *, player_state: SrsPlayerCombatState) -> SrsCombatState:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("initial.combat must be an object")

    raw_phase = data.get("phase", SrsCombatPhase.PLAYER_MOVEMENT.value)
    try:
        phase = SrsCombatPhase(raw_phase)
    except ValueError as exc:
        raise SrsFixtureError(f"invalid combat phase: {raw_phase}") from exc

    combat_turn = _optional_int(data, "combat_turn", default=0)
    player = _player_state(data.get("player"), default=player_state)

    player_attack_target_id = data.get("player_attack_target_id")
    if player_attack_target_id is not None and not isinstance(player_attack_target_id, str):
        raise SrsFixtureError("initial.combat.player_attack_target_id must be a string")

    try:
        base_combat_state = SrsCombatState(
            player=player,
            phase=phase,
            combat_turn=combat_turn,
        )
        if "encounter" in data:
            return _combat_state_from_encounter_fixture(
                state,
                data["encounter"],
                base_combat_state=base_combat_state,
                player_attack_target_id=player_attack_target_id,
            )
        return replace(
            base_combat_state,
            enemies=_combat_enemies(data),
            player_attack_target_id=player_attack_target_id,
        )
    except ValueError as exc:
        raise SrsFixtureError(str(exc)) from exc


def _combat_enemies(data: Mapping[str, Any]) -> Mapping[str, Any]:
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
        enemy = create_enemy_combat_state(
            enemy_id=enemy_id,
            tier=tier,
            position=_position(raw_enemy.get("position"), field_name=f"initial.combat.enemies[{index}].position"),
            drop_salvage=_optional_bool(raw_enemy, "drop_salvage", default=False),
        )
        durability = _optional_int(raw_enemy, "durability", default=enemy.durability)
        attack_damage = _optional_int(raw_enemy, "attack_damage", default=enemy.attack_damage)
        movement_power = _optional_int(raw_enemy, "movement_power", default=enemy.movement_power)
        enemies[enemy_id] = replace(
            enemy,
            durability=durability,
            attack_damage=attack_damage,
            movement_power=movement_power,
        )
    return enemies


def _combat_state_from_encounter_fixture(
    state: SrsGameState,
    data: Any,
    *,
    base_combat_state: SrsCombatState,
    player_attack_target_id: str | None,
) -> SrsCombatState:
    if not isinstance(data, Mapping):
        raise SrsFixtureError("initial.combat.encounter must be an object")
    danger_score = _required_int(data, "danger_score")
    raw_composition = data.get("composition")
    if not isinstance(raw_composition, list):
        raise SrsFixtureError("initial.combat.encounter.composition must be a list")

    composition: list[SrsEnemyTier] = []
    for index, raw_tier in enumerate(raw_composition, 1):
        if not isinstance(raw_tier, str):
            raise SrsFixtureError(f"initial.combat.encounter.composition[{index}] must be a string")
        try:
            composition.append(SrsEnemyTier(raw_tier))
        except ValueError as exc:
            raise SrsFixtureError(f"invalid enemy tier: {raw_tier}") from exc

    try:
        return combat_state_from_fixed_encounter(
            state,
            danger_score=danger_score,
            composition=tuple(composition),
            player_attack_target_id=player_attack_target_id,
            base_combat_state=base_combat_state,
        )
    except ValueError as exc:
        raise SrsFixtureError(str(exc)) from exc


def _encounter_roll_from_json(data: Any) -> FixedEncounterRoll | None:
    if data is None:
        return None
    if not isinstance(data, Mapping):
        raise SrsFixtureError("encounter_roll must be an object")

    roll_result = _required_str(data, "roll_result")
    raw_danger_score = data.get("danger_score")
    danger_score = None
    if raw_danger_score is not None:
        if not isinstance(raw_danger_score, int) or isinstance(raw_danger_score, bool):
            raise SrsFixtureError("encounter_roll.danger_score must be an integer")
        danger_score = raw_danger_score

    raw_composition = data.get("composition")
    composition: list[SrsEnemyTier] = []
    if raw_composition is not None:
        if not isinstance(raw_composition, list):
            raise SrsFixtureError("encounter_roll.composition must be a list")
        for index, raw_tier in enumerate(raw_composition, 1):
            if not isinstance(raw_tier, str):
                raise SrsFixtureError(f"encounter_roll.composition[{index}] must be a string")
            try:
                composition.append(SrsEnemyTier(raw_tier))
            except ValueError as exc:
                raise SrsFixtureError(f"invalid enemy tier: {raw_tier}") from exc

    try:
        return FixedEncounterRoll(
            roll_result=roll_result,
            danger_score=danger_score,
            composition=tuple(composition),
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
    enemy_positions = _enemy_positions_payload(final_state)
    enemy_actions = _flatten_enemy_actions(log)
    return {
        "fixture_id": fixture_id,
        "cost_mode": cost_mode.value,
        "srs_turn": final_state.srs_turn,
        "fuel": final_state.fuel,
        "max_fuel": final_state.max_fuel,
        "player_position": _position_to_list(final_state.player_position),
        "player_durability": final_state.player_state.durability,
        "player_energy": final_state.player_state.energy,
        "player_torpedo_ammo": final_state.player_state.photon_torpedo_ammo,
        "player_salvage": final_state.player_state.salvage,
        "player_energy_capacity": final_state.player_state.energy_capacity,
        "player_torpedo_ammo_capacity": final_state.player_state.photon_torpedo_ammo_capacity,
        "player_phaser_power": final_state.player_state.phaser_power,
        "player_photon_torpedo_power": final_state.player_state.photon_torpedo_power,
        "player_defense": final_state.player_state.defense,
        "player_evasion": final_state.player_state.evasion,
        "event_types": [event.event_type for event in log.events],
        "event_count": len(log.events),
        "consumed_object_ids": sorted(final_state.persistent_state.consumed_object_ids),
        "activated_object_ids": sorted(final_state.persistent_state.activated_object_ids),
        "discovered_count": len(final_state.known_state.discovered_cells),
        "combat_phase": final_state.combat_state.phase.value if final_state.combat_state is not None else None,
        "combat_turn": final_state.combat_state.combat_turn if final_state.combat_state is not None else None,
        "enemy_presence": final_state.combat_state.enemy_presence if final_state.combat_state is not None else False,
        "combat_player_durability": final_state.combat_state.player.durability if final_state.combat_state is not None else None,
        "combat_player_energy": final_state.combat_state.player.energy if final_state.combat_state is not None else None,
        "combat_player_torpedo_ammo": (
            final_state.combat_state.player.photon_torpedo_ammo
            if final_state.combat_state is not None
            else None
        ),
        "combat_enemy_positions": enemy_positions,
        "combat_enemy_durabilities": _enemy_durabilities_payload(final_state),
        "enemy_actions": enemy_actions,
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
    enemy_positions = _enemy_positions_payload(final_state)
    enemy_actions = _flatten_enemy_actions(result.log)
    comparisons = (
        ("srs_turn", final_state.srs_turn),
        ("fuel", final_state.fuel),
        ("player_position", _position_to_list(final_state.player_position)),
        ("player_durability", final_state.player_state.durability),
        ("player_energy", final_state.player_state.energy),
        ("player_torpedo_ammo", final_state.player_state.photon_torpedo_ammo),
        ("player_salvage", final_state.player_state.salvage),
        ("player_energy_capacity", final_state.player_state.energy_capacity),
        ("player_torpedo_ammo_capacity", final_state.player_state.photon_torpedo_ammo_capacity),
        ("player_phaser_power", final_state.player_state.phaser_power),
        ("player_photon_torpedo_power", final_state.player_state.photon_torpedo_power),
        ("player_defense", final_state.player_state.defense),
        ("player_evasion", final_state.player_state.evasion),
        ("event_types", event_types),
        ("consumed_object_ids", sorted(final_state.persistent_state.consumed_object_ids)),
        ("activated_object_ids", sorted(final_state.persistent_state.activated_object_ids)),
        ("combat_phase", final_state.combat_state.phase.value if final_state.combat_state is not None else None),
        ("combat_turn", final_state.combat_state.combat_turn if final_state.combat_state is not None else None),
        ("enemy_presence", final_state.combat_state.enemy_presence if final_state.combat_state is not None else False),
        ("combat_player_durability", final_state.combat_state.player.durability if final_state.combat_state is not None else None),
        ("combat_player_energy", final_state.combat_state.player.energy if final_state.combat_state is not None else None),
        (
            "combat_player_torpedo_ammo",
            final_state.combat_state.player.photon_torpedo_ammo if final_state.combat_state is not None else None,
        ),
        ("combat_enemy_positions", enemy_positions),
        ("combat_enemy_durabilities", _enemy_durabilities_payload(final_state)),
        ("enemy_actions", enemy_actions),
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


def _enemy_positions_payload(final_state: SrsGameState) -> Mapping[str, list[int]]:
    if final_state.combat_state is None:
        return {}
    return {
        enemy_id: _position_to_list(enemy.position)
        for enemy_id, enemy in sorted(final_state.combat_state.enemies.items())
    }


def _enemy_durabilities_payload(final_state: SrsGameState) -> Mapping[str, int]:
    if final_state.combat_state is None:
        return {}
    return {
        enemy_id: enemy.durability
        for enemy_id, enemy in sorted(final_state.combat_state.enemies.items())
    }


def _flatten_enemy_actions(log: SrsGameLog) -> list[Mapping[str, Any]]:
    actions = []
    for event in log.events:
        event_actions = event.payload.get("enemy_actions")
        if not isinstance(event_actions, (list, tuple)):
            continue
        for action in event_actions:
            if isinstance(action, Mapping):
                actions.append(dict(action))
    return actions


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


def _optional_bool(mapping: Mapping[str, Any], field_name: str, *, default: bool) -> bool:
    if field_name not in mapping:
        return default
    value = mapping[field_name]
    if not isinstance(value, bool):
        raise SrsFixtureError(f"{field_name} must be a boolean")
    return value


def _optional_position(mapping: Mapping[str, Any], field_name: str) -> Position | None:
    if field_name not in mapping:
        return None
    return _position(mapping[field_name], field_name=field_name)


def _player_state(data: Any, *, default: SrsPlayerCombatState | None = None) -> SrsPlayerCombatState:
    defaults = SrsPlayerCombatState() if default is None else default
    if data is None:
        return defaults
    if not isinstance(data, Mapping):
        raise SrsFixtureError("player must be an object")
    return SrsPlayerCombatState(
        durability=_optional_int(data, "durability", default=defaults.durability),
        durability_capacity=_optional_int(data, "durability_capacity", default=defaults.durability_capacity),
        defense=_optional_int(data, "defense", default=defaults.defense),
        evasion=_optional_int(data, "evasion", default=defaults.evasion),
        movement_power=_optional_int(data, "movement_power", default=defaults.movement_power),
        photon_torpedo_ammo=_optional_int(data, "photon_torpedo_ammo", default=defaults.photon_torpedo_ammo),
        photon_torpedo_ammo_capacity=_optional_int(
            data,
            "photon_torpedo_ammo_capacity",
            default=defaults.photon_torpedo_ammo_capacity,
        ),
        photon_torpedo_power=_optional_int(data, "photon_torpedo_power", default=defaults.photon_torpedo_power),
        energy=_optional_int(data, "energy", default=defaults.energy),
        energy_capacity=_optional_int(data, "energy_capacity", default=defaults.energy_capacity),
        phaser_power=_optional_int(data, "phaser_power", default=defaults.phaser_power),
        energy_recovery=_optional_int(data, "energy_recovery", default=defaults.energy_recovery),
        salvage=_optional_int(data, "salvage", default=defaults.salvage),
    )


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
