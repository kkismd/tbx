#!/usr/bin/env python3
"""Validate the Phase 2 SRS reference fixture and its replayable contracts."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from experiments.galactic_exodus.srs.model import Position
from experiments.galactic_exodus.srs.run_fixture import FIXTURES_DIR, REPO_ROOT, SrsFixtureRunResult, run_fixture


EXPECTED_SCHEMA_VERSION = 1
REQUIRED_CASE_IDS = {
    "normal_first_visit",
    "resource_cache_interaction",
    "resource_cache_consumed_revisit",
    "base_station_interaction",
    "base_upgrade_defense",
    "salvage_recover_durability",
    "nebula_3x3_observation",
    "rift_blocked_edge",
    "warp_exit_accepted",
    "warp_exit_rejected",
    "turn_only_route",
    "shared_fuel_route",
    "persistent_discovered_cells_restore",
    "combat_encounter_wait_nebula",
    "combat_enemy_counterattack",
    "combat_salvage_drop_tier3_energy",
}
ALLOWED_EXPECT_FIELDS = {
    "cost_mode",
    "srs_turn",
    "fuel",
    "max_fuel",
    "player_position",
    "player_durability",
    "player_energy",
    "player_torpedo_ammo",
    "player_salvage",
    "player_defense",
    "event_types",
    "consumed_object_ids",
    "activated_object_ids",
    "outcome",
    "discovered_count",
    "combat_phase",
    "combat_turn",
    "enemy_presence",
    "combat_player_durability",
    "combat_player_energy",
    "combat_player_torpedo_ammo",
    "combat_enemy_positions",
    "combat_enemy_durabilities",
}


class ValidationError(ValueError):
    """Raised when a Phase 2 reference artifact violates its contract."""


@dataclass(frozen=True, slots=True)
class ReferenceCase:
    case_id: str
    fixture_path: Path
    coverage_tags: tuple[str, ...]
    initial_expect: dict[str, Any]
    final_expect: dict[str, Any]


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "reference",
        type=Path,
        nargs="?",
        default=Path("experiments/galactic_exodus/srs/fixtures/phase2_reference.json"),
    )
    return parser.parse_args(argv)


def load_reference(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ValidationError(f"{path}: invalid JSON: {exc}") from exc
    if not isinstance(payload, dict):
        raise ValidationError(f"{path}: root must be an object")
    return payload


def validate(reference_path: Path) -> dict[str, Any]:
    payload = load_reference(reference_path)
    cases, comparison = _parse_reference(reference_path, payload)
    results = {case.case_id: run_fixture(case.fixture_path) for case in cases}

    for case in cases:
        _validate_case_expectations(case, results[case.case_id])
        _validate_known_map_secrecy(case.case_id, results[case.case_id])

    _validate_turn_only_vs_shared_fuel(results, comparison)
    _validate_nebula_observation(results["nebula_3x3_observation"])
    _validate_persistent_restore(results["persistent_discovered_cells_restore"])
    _validate_revisit_persistent_state(results["resource_cache_consumed_revisit"])
    _validate_base_station_recovery(results["base_station_interaction"])
    _validate_base_upgrade(results["base_upgrade_defense"])
    _validate_salvage_pickup_reward(results["salvage_recover_durability"])
    _validate_encounter_payload(results["combat_encounter_wait_nebula"])
    _validate_enemy_counterattack(results["combat_enemy_counterattack"])
    _validate_enemy_drop_reward(results["combat_salvage_drop_tier3_energy"])
    return {
        "case_count": len(cases),
        "fixture_dir": str(FIXTURES_DIR.relative_to(REPO_ROOT)),
    }


def _parse_reference(reference_path: Path, payload: dict[str, Any]) -> tuple[tuple[ReferenceCase, ...], dict[str, Any]]:
    schema_version = payload.get("reference_schema_version")
    if schema_version != EXPECTED_SCHEMA_VERSION:
        raise ValidationError(f"{reference_path}: reference_schema_version must be {EXPECTED_SCHEMA_VERSION}")

    spec_refs = payload.get("spec_refs")
    if not isinstance(spec_refs, list) or not spec_refs:
        raise ValidationError(f"{reference_path}: spec_refs must be a non-empty list")
    if any(not isinstance(item, str) or item == "" for item in spec_refs):
        raise ValidationError(f"{reference_path}: spec_refs must contain only non-empty strings")

    raw_cases = payload.get("cases")
    if not isinstance(raw_cases, list) or not raw_cases:
        raise ValidationError(f"{reference_path}: cases must be a non-empty list")
    cases = tuple(_parse_case(reference_path, raw_case) for raw_case in raw_cases)

    case_ids = {case.case_id for case in cases}
    if len(case_ids) != len(cases):
        raise ValidationError(f"{reference_path}: duplicate case_id detected")
    missing = REQUIRED_CASE_IDS - case_ids
    if missing:
        raise ValidationError(f"{reference_path}: missing required cases: {sorted(missing)}")

    comparisons = payload.get("comparisons")
    if not isinstance(comparisons, dict):
        raise ValidationError(f"{reference_path}: comparisons must be an object")
    comparison = comparisons.get("turn_only_vs_shared_fuel")
    if not isinstance(comparison, dict):
        raise ValidationError(f"{reference_path}: comparisons.turn_only_vs_shared_fuel must be an object")
    return cases, comparison


def _parse_case(reference_path: Path, raw_case: Any) -> ReferenceCase:
    if not isinstance(raw_case, dict):
        raise ValidationError(f"{reference_path}: each case must be an object")

    case_id = _require_str(raw_case, "case_id", reference_path)
    fixture_rel = _require_str(raw_case, "fixture_path", reference_path)
    coverage_tags = raw_case.get("coverage_tags")
    if not isinstance(coverage_tags, list) or not coverage_tags:
        raise ValidationError(f"{reference_path}: case {case_id} coverage_tags must be a non-empty list")
    if any(not isinstance(tag, str) or tag == "" for tag in coverage_tags):
        raise ValidationError(f"{reference_path}: case {case_id} coverage_tags must contain only strings")

    fixture_path = reference_path.parent / fixture_rel
    if not fixture_path.is_file():
        raise ValidationError(f"{reference_path}: case {case_id} fixture file not found: {fixture_rel}")

    initial_expect = _parse_expect(
        raw_case.get("initial_expect", {}),
        case_id=case_id,
        field_name="initial_expect",
        reference_path=reference_path,
    )
    final_expect = _parse_expect(
        raw_case.get("final_expect", {}),
        case_id=case_id,
        field_name="final_expect",
        reference_path=reference_path,
    )
    if not final_expect:
        raise ValidationError(f"{reference_path}: case {case_id} final_expect must not be empty")

    return ReferenceCase(
        case_id=case_id,
        fixture_path=fixture_path,
        coverage_tags=tuple(coverage_tags),
        initial_expect=initial_expect,
        final_expect=final_expect,
    )


def _parse_expect(raw_expect: Any, *, case_id: str, field_name: str, reference_path: Path) -> dict[str, Any]:
    if not isinstance(raw_expect, dict):
        raise ValidationError(f"{reference_path}: case {case_id} {field_name} must be an object")
    unknown = sorted(set(raw_expect) - ALLOWED_EXPECT_FIELDS)
    if unknown:
        raise ValidationError(f"{reference_path}: case {case_id} {field_name} has unknown field {unknown[0]}")
    return dict(raw_expect)


def _validate_case_expectations(case: ReferenceCase, result: SrsFixtureRunResult) -> None:
    _compare_expectations(case.case_id, "initial_expect", case.initial_expect, _snapshot(result.initial_state, result, use_final=False))
    _compare_expectations(case.case_id, "final_expect", case.final_expect, _snapshot(result.final_state, result, use_final=True))


def _snapshot(state: Any, result: SrsFixtureRunResult, *, use_final: bool) -> dict[str, Any]:
    summary = result.summary if use_final else {"cost_mode": result.summary["cost_mode"], "outcome": None}
    combat_state = state.combat_state
    return {
        "cost_mode": summary["cost_mode"],
        "srs_turn": state.srs_turn,
        "fuel": state.fuel,
        "max_fuel": state.max_fuel,
        "player_position": [state.player_position.x, state.player_position.y],
        "player_durability": state.player_state.durability,
        "player_energy": state.player_state.energy,
        "player_torpedo_ammo": state.player_state.photon_torpedo_ammo,
        "player_salvage": state.player_state.salvage,
        "player_defense": state.player_state.defense,
        "event_types": [event.event_type for event in result.log.events] if use_final else [],
        "consumed_object_ids": sorted(state.persistent_state.consumed_object_ids),
        "activated_object_ids": sorted(state.persistent_state.activated_object_ids),
        "outcome": summary["outcome"],
        "discovered_count": len(state.known_state.discovered_cells),
        "combat_phase": combat_state.phase.value if combat_state is not None else None,
        "combat_turn": combat_state.combat_turn if combat_state is not None else None,
        "enemy_presence": combat_state.enemy_presence if combat_state is not None else False,
        "combat_player_durability": combat_state.player.durability if combat_state is not None else None,
        "combat_player_energy": combat_state.player.energy if combat_state is not None else None,
        "combat_player_torpedo_ammo": combat_state.player.photon_torpedo_ammo if combat_state is not None else None,
        "combat_enemy_positions": _enemy_positions(combat_state),
        "combat_enemy_durabilities": _enemy_durabilities(combat_state),
    }


def _enemy_positions(combat_state: Any) -> dict[str, list[int]]:
    if combat_state is None:
        return {}
    return {
        enemy_id: [enemy.position.x, enemy.position.y]
        for enemy_id, enemy in sorted(combat_state.enemies.items())
    }


def _enemy_durabilities(combat_state: Any) -> dict[str, int]:
    if combat_state is None:
        return {}
    return {
        enemy_id: enemy.durability
        for enemy_id, enemy in sorted(combat_state.enemies.items())
    }


def _compare_expectations(case_id: str, phase: str, expected: dict[str, Any], actual: dict[str, Any]) -> None:
    for field_name, expected_value in expected.items():
        actual_value = actual[field_name]
        if actual_value != expected_value:
            raise ValidationError(f"{case_id}: {phase}.{field_name} expected {expected_value!r}, got {actual_value!r}")


def _validate_known_map_secrecy(case_id: str, result: SrsFixtureRunResult) -> None:
    state = result.final_state
    discovered = state.known_state.discovered_cells
    known_positions = set(state.known_state.known_cells)
    if known_positions != set(discovered):
        raise ValidationError(f"{case_id}: known_cells keys must exactly match discovered_cells")

    rows = result.render.splitlines()
    if len(rows) != state.actual_map.height:
        raise ValidationError(f"{case_id}: render height mismatch")
    if any(len(row) != state.actual_map.width for row in rows):
        raise ValidationError(f"{case_id}: render width mismatch")

    for y in range(state.actual_map.height):
        for x in range(state.actual_map.width):
            position = Position(x, y)
            rendered = rows[y][x]
            if position in discovered:
                if rendered == "?":
                    raise ValidationError(f"{case_id}: discovered cell rendered as unknown at {position}")
                if state.known_state.known_cells[position] != state.actual_map.cell_at(position):
                    raise ValidationError(f"{case_id}: known cell differs from actual cell at {position}")
            elif rendered != "?":
                raise ValidationError(f"{case_id}: undiscovered cell leaked into render at {position}")


def _validate_turn_only_vs_shared_fuel(results: dict[str, SrsFixtureRunResult], comparison: dict[str, Any]) -> None:
    turn_only_case_id = comparison.get("turn_only_case_id")
    shared_fuel_case_id = comparison.get("shared_fuel_case_id")
    expected_fuel_delta = comparison.get("expected_fuel_delta")
    if not isinstance(turn_only_case_id, str) or not isinstance(shared_fuel_case_id, str):
        raise ValidationError("comparisons.turn_only_vs_shared_fuel case ids must be strings")
    if not isinstance(expected_fuel_delta, int) or expected_fuel_delta <= 0:
        raise ValidationError("comparisons.turn_only_vs_shared_fuel expected_fuel_delta must be a positive integer")

    turn_only = results[turn_only_case_id]
    shared_fuel = results[shared_fuel_case_id]
    if turn_only.final_state.player_position != shared_fuel.final_state.player_position:
        raise ValidationError("TURN_ONLY and SHARED_FUEL comparison routes must end at the same position")
    if turn_only.final_state.srs_turn != shared_fuel.final_state.srs_turn:
        raise ValidationError("TURN_ONLY and SHARED_FUEL comparison routes must consume the same turns")
    if [event.event_type for event in turn_only.log.events] != [event.event_type for event in shared_fuel.log.events]:
        raise ValidationError("TURN_ONLY and SHARED_FUEL comparison routes must emit the same event types")
    if turn_only.final_state.fuel != turn_only.initial_state.fuel:
        raise ValidationError("TURN_ONLY comparison case must not consume fuel")
    actual_delta = turn_only.final_state.fuel - shared_fuel.final_state.fuel
    if actual_delta != expected_fuel_delta:
        raise ValidationError(f"TURN_ONLY vs SHARED_FUEL fuel delta expected {expected_fuel_delta}, got {actual_delta}")


def _validate_nebula_observation(result: SrsFixtureRunResult) -> None:
    discovered = result.final_state.known_state.discovered_cells
    if len(discovered) != 9:
        raise ValidationError("nebula_3x3_observation: discovered_count must be 9")
    center = result.final_state.player_position
    expected = {
        Position(x, y)
        for y in range(center.y - 1, center.y + 2)
        for x in range(center.x - 1, center.x + 2)
    }
    if discovered != expected:
        raise ValidationError("nebula_3x3_observation: discovered cells must match the 3x3 neighborhood")


def _validate_persistent_restore(result: SrsFixtureRunResult) -> None:
    initial = result.initial_state.known_state.discovered_cells
    final = result.final_state.known_state.discovered_cells
    if initial != final:
        raise ValidationError("persistent_discovered_cells_restore: discovered cells must be stable across no-op replay")
    if result.final_state.persistent_state.discovered_cells != final:
        raise ValidationError("persistent_discovered_cells_restore: persistent discovered cells must match known state")
    if result.log.events:
        raise ValidationError("persistent_discovered_cells_restore: no commands should yield no events")


def _validate_revisit_persistent_state(result: SrsFixtureRunResult) -> None:
    if "resource-cache-1" not in result.final_state.persistent_state.consumed_object_ids:
        raise ValidationError("resource_cache_consumed_revisit: consumed resource cache must stay persistent")
    if not result.final_state.objects["resource-cache-1"].consumed:
        raise ValidationError("resource_cache_consumed_revisit: object state must stay consumed")


def _validate_base_station_recovery(result: SrsFixtureRunResult) -> None:
    payload = result.log.events[0].payload
    if payload.get("fuel_delta") != 7:
        raise ValidationError("base_station_interaction: station must restore fuel by 7 from the representative fixture")
    if payload.get("player_durability_after") != 100 or payload.get("player_energy_after") != 6:
        raise ValidationError("base_station_interaction: station must fully recover combat resources")
    if payload.get("player_torpedo_ammo_after") != 6:
        raise ValidationError("base_station_interaction: station must refill torpedo ammo")
    if payload.get("salvage_spent") != 0:
        raise ValidationError("base_station_interaction: no salvage should be spent without an upgrade choice")


def _validate_base_upgrade(result: SrsFixtureRunResult) -> None:
    payload = result.log.events[0].payload
    if payload.get("selected_upgrade") != "DEFENSE" or payload.get("applied_upgrade") != "DEFENSE":
        raise ValidationError("base_upgrade_defense: DEFENSE upgrade must be selected and applied")
    if payload.get("salvage_spent") != 4:
        raise ValidationError("base_upgrade_defense: DEFENSE upgrade must spend salvage 4")
    if payload.get("salvage_after") != 0:
        raise ValidationError("base_upgrade_defense: salvage inventory must be exhausted by the representative purchase")


def _validate_salvage_pickup_reward(result: SrsFixtureRunResult) -> None:
    payload = result.log.events[0].payload
    if payload.get("selected_salvage_choice") != "RECOVER_DURABILITY":
        raise ValidationError("salvage_recover_durability: durability recovery choice must be recorded")
    if payload.get("salvage_after") != 1:
        raise ValidationError("salvage_recover_durability: salvage inventory must increase by 1")
    if payload.get("durability_delta") != 8:
        raise ValidationError("salvage_recover_durability: representative pickup must recover 8 durability")


def _validate_encounter_payload(result: SrsFixtureRunResult) -> None:
    payload = result.log.events[-1].payload
    if payload.get("terrain") != "NEBULA" or payload.get("terrain_modifier") != 0.7:
        raise ValidationError("combat_encounter_wait_nebula: encounter payload must record the NEBULA modifier")
    if payload.get("actual_encounter_chance") != 0.126:
        raise ValidationError("combat_encounter_wait_nebula: actual encounter chance must be 0.126")
    if payload.get("spawned_enemy_ids") != ["enemy-1"]:
        raise ValidationError("combat_encounter_wait_nebula: representative encounter must spawn enemy-1")
    if payload.get("outcome") != "ENCOUNTER_STARTED":
        raise ValidationError("combat_encounter_wait_nebula: encounter must start combat")


def _validate_enemy_counterattack(result: SrsFixtureRunResult) -> None:
    event_actions = result.log.events[0].payload.get("enemy_actions")
    if not isinstance(event_actions, tuple) or len(event_actions) != 1:
        raise ValidationError("combat_enemy_counterattack: representative combat must emit one enemy action")
    reaction = event_actions[0].get("reaction")
    if not isinstance(reaction, dict):
        raise ValidationError("combat_enemy_counterattack: enemy reaction payload is required")
    if reaction.get("resolved_reaction") != "COUNTERATTACK":
        raise ValidationError("combat_enemy_counterattack: resolved reaction must be COUNTERATTACK")
    if reaction.get("counterattack_damage") != 1 or reaction.get("damage_to_player") != 6:
        raise ValidationError("combat_enemy_counterattack: representative reaction damage values changed")


def _validate_enemy_drop_reward(result: SrsFixtureRunResult) -> None:
    payload = result.log.events[0].payload.get("player_action")
    if not isinstance(payload, dict):
        raise ValidationError("combat_salvage_drop_tier3_energy: player_action payload is required")
    reward = payload.get("salvage_reward")
    if not isinstance(reward, dict):
        raise ValidationError("combat_salvage_drop_tier3_energy: salvage_reward payload is required")
    if reward.get("reward_source") != "ENEMY_DROP":
        raise ValidationError("combat_salvage_drop_tier3_energy: reward source must be ENEMY_DROP")
    if reward.get("selected_salvage_choice") != "RECOVER_ENERGY":
        raise ValidationError("combat_salvage_drop_tier3_energy: energy recovery choice must be recorded")
    if reward.get("salvage_after") != 3 or reward.get("energy_after") != 6:
        raise ValidationError("combat_salvage_drop_tier3_energy: representative enemy drop reward values changed")


def _require_str(mapping: dict[str, Any], field_name: str, path: Path) -> str:
    value = mapping.get(field_name)
    if not isinstance(value, str) or value == "":
        raise ValidationError(f"{path}: {field_name} must be a non-empty string")
    return value


def validate_all(reference: Path) -> dict[str, Any]:
    return validate(reference)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        summary = validate_all(args.reference)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 2 SRS reference fixture: OK")
    print(f"cases: {summary['case_count']}")
    print(f"fixture_dir: {summary['fixture_dir']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
