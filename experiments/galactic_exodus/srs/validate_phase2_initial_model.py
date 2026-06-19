#!/usr/bin/env python3
"""Validate Galactic Exodus Phase 2 SRS initial-model artifacts."""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from experiments.galactic_exodus.srs import validate_phase2_srs_generation as generation_validator

QUESTION_FIELDS = [
    "question_id",
    "question",
    "hypothesis",
    "comparison_ids",
    "automated_metrics",
    "manual_scores",
    "required_sector_types",
    "required_fixtures",
    "decision_rule",
]
EXPECTED_QUESTIONS = {f"Q{index}" for index in range(1, 21)}
EXPECTED_COMPARISONS = {f"C{index}" for index in range(1, 9)}
EXPECTED_MAP_SIZES = [[9, 9], [11, 11]]
EXPECTED_SECTOR_TYPES = {
    "NORMAL",
    "BASE",
    "RESOURCE",
    "NEBULA",
    "ASTEROID",
    "GRAVITY",
    "RIFT",
}
EXPECTED_DIRECTIONS = {"N", "E", "S", "W"}
EXPECTED_TERRAIN_TYPES = {
    "FLOOR",
    "DEBRIS",
    "NEBULA",
    "ASTEROID_FIELD",
    "ASTEROID",
    "GRAVITY_FIELD_VERTICAL",
    "GRAVITY_FIELD_HORIZONTAL",
    "RIFT_DISTORTION",
    "RIFT_BARRIER",
}
EXPECTED_OBJECT_TYPES = {"STAR", "PLANET", "STATION", "RESOURCE_CACHE", "SALVAGE"}
EXPECTED_ACTOR_TYPES = {"PLAYER"}
EXPECTED_MOVEMENT_RULES = {"VECTOR_COMMAND", "MOVEMENT_POINTS", "DIRECTIONAL_THRUST"}
EXPECTED_PATH_INPUT_MODES = {"STEPWISE_ROUTE", "DESTINATION_AUTO_PATH", "ROUTE_PREVIEW"}
EXPECTED_CONTRACT_REFERENCES = {
    "elements": "phase2_srs_elements.json",
    "generation": "phase2_srs_generation.json",
    "movement_rule_issue": 1089,
}
EXPECTED_MOVEMENT_RULE_REFERENCE = {"issue": 1089, "rule_id": "PASSABLE_ADJACENCY"}
REQUIRED_BASELINE_KEYS = {
    "width",
    "height",
    "generation_profile",
    "generation_schema_version",
    "observation_mode",
    "cost_mode",
    "interaction_mode",
    "sector_value_route",
    "rift_knowledge_mode",
    "movement_rule",
    "movement_points_per_turn",
    "path_input_mode",
    "collision_behavior",
    "max_srs_turns",
}
REQUIRED_PERSISTENT_FIELDS = {
    "generated_map_id",
    "generation_schema_version",
    "generation_seed",
    "sector_type",
    "blocked_edges",
    "warp_flags",
    "celestial_body_positions",
    "consumed_object_ids",
    "activated_object_ids",
    "discovered_cells",
}
FORBIDDEN_ROOT_FIELDS = {
    "feature_types",
    "object_profiles",
    "terrain_profiles",
    "initial_terrain_effects",
    "element_definitions",
    "celestial_body_profiles",
    "invariants",
}
REQUIRED_MODEL_TOKENS = [
    "SectorType",
    "SrsTerrainType",
    "SrsObjectType",
    "SrsActorType",
    "SrsCell.warp_flags",
    "blocked_edges",
    "generation_schema_version",
    "generation_profile_ref",
    "SectorDescriptor",
    "GRAVITY_FIELD_VERTICAL",
    "GRAVITY_FIELD_HORIZONTAL",
    "STATION",
    "STAR",
    "PLANET",
    "RESOURCE_CACHE",
    "SALVAGE",
    "9x9",
    "11x11",
    "LOCAL_MOVEMENT",
    "TURN_ONLY",
    "EXPLICIT_INTERACT",
    "VALUE_OBJECT_DETOUR",
    "KNOWN_DESCRIPTOR",
    "MOVEMENT_POINTS",
    "VECTOR_COMMAND",
    "DIRECTIONAL_THRUST",
    "STOP_BEFORE",
    "C1..C8",
    "Q1..Q16",
    "#1080",
    "phase2_srs_elements.md/json",
    "phase2_srs_generation.md/json",
    "phase2_initial_values.json",
]
LEGACY_BOUNDARY_TOKENS = [
    "WALL",
    "STATION_STRUCTURE",
    "BASE_NODE",
    "WARP_POINT",
    "WarpZone",
    "GRAVITY_FIELD",
    "7x7",
    "PROFILE_MINIMAL",
    "PROFILE_EXPLORATION",
    "LOCAL_3X3",
]
LEGACY_SUBSTRING_TOKENS = [
    "obstacle_density",
    "seven_by_seven",
    "warp_point_width",
    "warp_clearance_depth",
    "object_profile",
    "terrain_profile",
    "feature_types",
]
Q17_TO_Q20_EXPECTATIONS = {
    "Q17": {
        "comparison_ids": {"C1", "C7"},
        "automated_metrics": {
            "terrain_count_by_type",
            "special_terrain_ratio",
            "isolated_cell_ratio",
            "reachable_cell_ratio",
        },
        "manual_scores": {"sector_identity_score", "terrain_density_naturalness_score"},
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {"all_sectors_9x9_multi_seed", "all_sectors_11x11_multi_seed"},
        "decision_rule_contains": [],
    },
    "Q18": {
        "comparison_ids": {"C1", "C5", "C7"},
        "automated_metrics": {
            "celestial_spacing",
            "object_count_by_type",
            "object_detour_cost",
            "object_reachability_rate",
        },
        "manual_scores": {"navigation_readability_score", "object_placement_value_score"},
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {"all_sectors_9x9_multi_seed", "all_sectors_11x11_multi_seed"},
        "decision_rule_contains": [],
    },
    "Q19": {
        "comparison_ids": {"C1", "C7"},
        "automated_metrics": {
            "generation_failure_rate",
            "retry_index_p50",
            "retry_index_p95",
            "max_retry_index",
        },
        "manual_scores": {"generation_stability_score", "failure_diagnosability_score"},
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {"all_sectors_9x9_seed_batch", "all_sectors_11x11_seed_batch"},
        "decision_rule_contains": [
            "generation_failure_rate=0",
            "retry_index_p95<64",
            "max_retry_index<=63",
        ],
    },
    "Q20": {
        "comparison_ids": {"C1", "C7"},
        "automated_metrics": {
            "deterministic_map_match_rate",
            "generation_report_match_rate",
            "seed_collision_count",
        },
        "manual_scores": {"reproducibility_confidence_score", "debug_traceability_score"},
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {"all_sectors_9x9_repeat_seed_batch", "all_sectors_11x11_repeat_seed_batch"},
        "decision_rule_contains": [
            "deterministic_map_match_rate=1.0",
            "generation_report_match_rate=1.0",
            "seed_collision_count=0",
        ],
    },
}


class ValidationError(ValueError):
    """Raised when Phase 2 initial-model artifacts are inconsistent."""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--questions", type=Path, required=True)
    parser.add_argument("--values", type=Path, required=True)
    parser.add_argument("--elements", type=Path, required=True)
    parser.add_argument("--generation", type=Path, required=True)
    return parser.parse_args(argv)


def split_semicolon(value: str) -> set[str]:
    return {item.strip() for item in value.split(";") if item.strip()}


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ValidationError(f"invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValidationError(f"{path}: root must be an object")
    return value


def require_exact_set(path: Path, data: dict[str, Any], key: str, expected: set[str]) -> None:
    actual = set(data.get(key, []))
    if actual != expected:
        raise ValidationError(f"{path}: {key} must be {sorted(expected)}")


def validate_values(path: Path) -> dict[str, Any]:
    data = load_json(path)
    if data.get("schema_version") != 3:
        raise ValidationError(f"{path}: schema_version must be 3")
    if data.get("generation_schema_version") != 1:
        raise ValidationError(f"{path}: generation_schema_version must be 1")
    if data.get("contract_references") != EXPECTED_CONTRACT_REFERENCES:
        raise ValidationError(f"{path}: contract_references must match the schema 3 contract")
    for field in sorted(FORBIDDEN_ROOT_FIELDS):
        if field in data:
            raise ValidationError(f"{path}: {field} must not exist")

    require_exact_set(path, data, "sector_types", EXPECTED_SECTOR_TYPES)
    require_exact_set(path, data, "directions", EXPECTED_DIRECTIONS)
    require_exact_set(path, data, "terrain_types", EXPECTED_TERRAIN_TYPES)
    require_exact_set(path, data, "object_types", EXPECTED_OBJECT_TYPES)
    require_exact_set(path, data, "actor_types", EXPECTED_ACTOR_TYPES)
    require_exact_set(path, data, "movement_rules", EXPECTED_MOVEMENT_RULES)
    require_exact_set(path, data, "path_input_modes", EXPECTED_PATH_INPUT_MODES)

    baseline = data.get("baseline")
    if not isinstance(baseline, dict):
        raise ValidationError(f"{path}: baseline must be an object")
    baseline_keys = set(baseline)
    missing_baseline = REQUIRED_BASELINE_KEYS - baseline_keys
    if missing_baseline:
        raise ValidationError(f"{path}: baseline missing {sorted(missing_baseline)}")
    extra_baseline = baseline_keys - REQUIRED_BASELINE_KEYS
    if extra_baseline:
        raise ValidationError(f"{path}: baseline has unexpected keys {sorted(extra_baseline)}")
    if baseline.get("width") != 9 or baseline.get("height") != 9:
        raise ValidationError(f"{path}: baseline must be 9x9")
    if baseline.get("generation_profile") != "phase2_srs_generation.json":
        raise ValidationError(f"{path}: baseline generation_profile must be phase2_srs_generation.json")
    if baseline.get("generation_schema_version") != 1:
        raise ValidationError(f"{path}: baseline generation_schema_version must be 1")
    if baseline.get("observation_mode") != "LOCAL_MOVEMENT":
        raise ValidationError(f"{path}: baseline observation_mode must be LOCAL_MOVEMENT")
    if baseline.get("movement_rule") not in EXPECTED_MOVEMENT_RULES:
        raise ValidationError(f"{path}: invalid baseline movement_rule")
    if baseline.get("path_input_mode") not in EXPECTED_PATH_INPUT_MODES:
        raise ValidationError(f"{path}: invalid baseline path_input_mode")
    if baseline.get("collision_behavior") != "STOP_BEFORE":
        raise ValidationError(f"{path}: baseline collision_behavior must be STOP_BEFORE")
    movement_points = baseline.get("movement_points_per_turn")
    if not isinstance(movement_points, int) or movement_points < 1:
        raise ValidationError(f"{path}: movement_points_per_turn must be an integer >= 1")

    comparisons = data.get("comparisons")
    if not isinstance(comparisons, dict) or set(comparisons) != EXPECTED_COMPARISONS:
        raise ValidationError(f"{path}: comparisons must be C1..C8")
    if comparisons.get("C1") != {"field": "map_size", "values": EXPECTED_MAP_SIZES}:
        raise ValidationError(f"{path}: C1 must compare 9x9 and 11x11")
    if comparisons.get("C2") != {"field": "observation_mode", "values": ["FULL", "LOCAL_MOVEMENT"]}:
        raise ValidationError(f"{path}: C2 must compare FULL and LOCAL_MOVEMENT")
    if comparisons.get("C5") != {"field": "sector_value_route", "values": ["DIRECT_EXIT", "VALUE_OBJECT_DETOUR"]}:
        raise ValidationError(f"{path}: C5 must compare sector_value_route")
    if comparisons.get("C7") != {
        "field": "sector_type",
        "values": ["NORMAL", "BASE", "RESOURCE", "NEBULA", "ASTEROID", "GRAVITY", "RIFT"],
    }:
        raise ValidationError(f"{path}: C7 must compare all sector types")
    if comparisons.get("C8") != {
        "field": "movement_rule",
        "values": ["VECTOR_COMMAND", "MOVEMENT_POINTS", "DIRECTIONAL_THRUST"],
    }:
        raise ValidationError(f"{path}: C8 must compare all movement rules")

    persistent = set(data.get("persistent_fields", []))
    if persistent != REQUIRED_PERSISTENT_FIELDS:
        raise ValidationError(f"{path}: persistent_fields must be {sorted(REQUIRED_PERSISTENT_FIELDS)}")
    return data


def validate_generation(path: Path) -> dict[str, Any]:
    generation = load_json(path)
    removed_contracts = generation.get("legacy_contracts_removed")
    if not isinstance(removed_contracts, dict):
        try:
            return generation_validator.validate(path)
        except generation_validator.ValidationError as exc:
            raise ValidationError(str(exc)) from exc

    sanitized = json.loads(json.dumps(generation))
    sanitized["legacy_contracts_removed"] = {"legacy_contracts_documented": "removed"}
    repo_root = Path(__file__).resolve().parents[3]
    temp_dir = repo_root / ".tmp"
    temp_dir.mkdir(parents=True, exist_ok=True)
    temp_path = temp_dir / f"validate-phase2-initial-model-generation-{os.getpid()}.json"
    temp_path.write_text(json.dumps(sanitized, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    try:
        return generation_validator.validate(temp_path)
    except generation_validator.ValidationError as exc:
        raise ValidationError(str(exc)) from exc
    finally:
        temp_path.unlink(missing_ok=True)


def validate_question_row(path: Path, row: dict[str, str], values: dict[str, Any]) -> None:
    label = row["question_id"]
    for field in QUESTION_FIELDS:
        if not row.get(field, "").strip():
            raise ValidationError(f"{path}: {label}.{field} must not be blank")

    unknown_comparisons = split_semicolon(row["comparison_ids"]) - set(values["comparisons"])
    if unknown_comparisons:
        raise ValidationError(f"{path}: {label} has unknown comparisons {sorted(unknown_comparisons)}")

    sectors = split_semicolon(row["required_sector_types"])
    if not sectors or not sectors <= set(values["sector_types"]):
        raise ValidationError(f"{path}: {label} has invalid required_sector_types")
    if not split_semicolon(row["automated_metrics"]):
        raise ValidationError(f"{path}: {label} needs automated metrics")
    if not split_semicolon(row["manual_scores"]):
        raise ValidationError(f"{path}: {label} needs manual scores")
    if not split_semicolon(row["required_fixtures"]):
        raise ValidationError(f"{path}: {label} needs fixtures")


def validate_fixed_question_contracts(path: Path, rows_by_id: dict[str, dict[str, str]]) -> None:
    for question_id, expected in Q17_TO_Q20_EXPECTATIONS.items():
        row = rows_by_id[question_id]
        for field in ("comparison_ids", "automated_metrics", "manual_scores", "required_sector_types", "required_fixtures"):
            actual = split_semicolon(row[field])
            if actual != expected[field]:
                raise ValidationError(f"{path}: {question_id}.{field} must match the Phase 2A1c contract")
        for token in expected["decision_rule_contains"]:
            if token not in row["decision_rule"]:
                raise ValidationError(f"{path}: {question_id}.decision_rule must contain {token}")


def validate_questions(path: Path, values: dict[str, Any]) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as file:
        rows = list(csv.DictReader(file))
    if rows and list(rows[0]) != QUESTION_FIELDS:
        raise ValidationError(f"{path}: question CSV header must be {QUESTION_FIELDS}")
    ids = [row["question_id"] for row in rows]
    if set(ids) != EXPECTED_QUESTIONS or len(ids) != len(EXPECTED_QUESTIONS):
        raise ValidationError(f"{path}: questions must contain Q1..Q20 exactly once")
    rows_by_id = {row["question_id"]: row for row in rows}
    for question_id in sorted(EXPECTED_QUESTIONS, key=lambda value: int(value[1:])):
        validate_question_row(path, rows_by_id[question_id], values)
    validate_fixed_question_contracts(path, rows_by_id)
    return rows


def validate_model(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    for token in REQUIRED_MODEL_TOKENS:
        if token not in text:
            raise ValidationError(f"{path}: model must mention {token}")
    if "TBD" in text:
        raise ValidationError(f"{path}: TBD must not remain")


def sanitize_for_legacy_scan(path: Path, payload: Any) -> Any:
    if not isinstance(payload, dict):
        return payload
    sanitized = json.loads(json.dumps(payload, ensure_ascii=False))
    name = path.name
    if name == "phase2_srs_generation.json":
        sanitized.pop("legacy_contracts_removed", None)
    if name == "phase2_srs_elements.json":
        sanitized.pop("warp_point", None)
    return sanitized


def assert_no_legacy_tokens(paths_and_payloads: list[tuple[Path, Any]]) -> None:
    for path, payload in paths_and_payloads:
        scan_payload = sanitize_for_legacy_scan(path, payload)
        text = json.dumps(scan_payload, ensure_ascii=False) if not isinstance(scan_payload, str) else scan_payload
        for token in LEGACY_BOUNDARY_TOKENS:
            if re.search(rf"(?<![A-Z0-9_]){re.escape(token)}(?![A-Z0-9_])", text):
                raise ValidationError(f"{path}: forbidden legacy token remains: {token}")
        for token in LEGACY_SUBSTRING_TOKENS:
            if token in text:
                raise ValidationError(f"{path}: forbidden legacy token remains: {token}")


def validate_cross_file(
    values: dict[str, Any],
    elements: dict[str, Any],
    generation: dict[str, Any],
) -> None:
    if elements.get("schema_version") != 1:
        raise ValidationError("phase2_srs_elements.json: schema_version must be 1")
    if elements.get("map_sizes") != EXPECTED_MAP_SIZES:
        raise ValidationError("phase2_srs_elements.json: map_sizes must be 9x9 and 11x11")
    if generation.get("map_sizes") != EXPECTED_MAP_SIZES:
        raise ValidationError("phase2_srs_generation.json: map_sizes must be 9x9 and 11x11")
    if set(elements.get("sector_terrain_matrix", {})) != set(values["sector_types"]):
        raise ValidationError("phase2_srs_elements.json: sector_terrain_matrix must match values.sector_types")
    if set(elements.get("terrain_object_matrix", {})) != set(values["terrain_types"]):
        raise ValidationError("phase2_srs_elements.json: terrain_object_matrix must match values.terrain_types")
    for terrain, objects in elements.get("terrain_object_matrix", {}).items():
        if not set(objects) <= set(values["object_types"]):
            raise ValidationError(f"phase2_srs_elements.json: terrain_object_matrix.{terrain} must stay within values.object_types")
    if set(generation.get("sector_profiles", {})) != set(values["sector_types"]):
        raise ValidationError("phase2_srs_generation.json: sector_profiles must define all seven sector types")
    reachability = generation.get("global_generation_contract", {}).get("reachability", {})
    if reachability.get("movement_rule_reference") != EXPECTED_MOVEMENT_RULE_REFERENCE:
        raise ValidationError("phase2_srs_generation.json: movement_rule_reference must point to #1089 PASSABLE_ADJACENCY")


def validate_all(
    model_path: Path,
    questions_path: Path,
    values_path: Path,
    elements_path: Path,
    generation_path: Path,
) -> None:
    values = validate_values(values_path)
    questions = validate_questions(questions_path, values)
    validate_model(model_path)
    elements = load_json(elements_path)
    generation = validate_generation(generation_path)
    validate_cross_file(values, elements, generation)
    assert_no_legacy_tokens(
        [
            (model_path, model_path.read_text(encoding="utf-8")),
            (questions_path, questions),
            (values_path, values),
            (elements_path, elements),
            (generation_path, generation),
        ]
    )


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        values = validate_values(args.values)
        questions = validate_questions(args.questions, values)
        validate_model(args.model)
        elements = load_json(args.elements)
        generation = validate_generation(args.generation)
        validate_cross_file(values, elements, generation)
        assert_no_legacy_tokens(
            [
                (args.model, args.model.read_text(encoding="utf-8")),
                (args.questions, questions),
                (args.values, values),
                (args.elements, elements),
                (args.generation, generation),
            ]
        )
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print("Phase 2 SRS initial model: OK")
    print(f"questions: {len(questions)}")
    print(f"comparisons: {len(values['comparisons'])}")
    print(f"sector types: {len(values['sector_types'])}")
    print(f"movement rules: {len(values['movement_rules'])}")
    print("cross-file: OK")
    print("TBD: 0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
