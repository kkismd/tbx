#!/usr/bin/env python3
"""Validate Galactic Exodus Phase 2 SRS initial-model artifacts."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path
from typing import Any

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
EXPECTED_QUESTIONS = {f"Q{index}" for index in range(1, 17)}
EXPECTED_COMPARISONS = {f"C{index}" for index in range(1, 9)}
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


class ValidationError(ValueError):
    """Raised when Phase 2 initial-model artifacts are inconsistent."""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--questions", type=Path, required=True)
    parser.add_argument("--values", type=Path, required=True)
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

    require_exact_set(path, data, "sector_types", EXPECTED_SECTOR_TYPES)
    require_exact_set(path, data, "directions", EXPECTED_DIRECTIONS)
    require_exact_set(path, data, "terrain_types", EXPECTED_TERRAIN_TYPES)
    require_exact_set(path, data, "object_types", EXPECTED_OBJECT_TYPES)
    require_exact_set(path, data, "actor_types", EXPECTED_ACTOR_TYPES)
    require_exact_set(path, data, "movement_rules", EXPECTED_MOVEMENT_RULES)
    require_exact_set(path, data, "path_input_modes", EXPECTED_PATH_INPUT_MODES)
    if "feature_types" in data:
        raise ValidationError(f"{path}: feature_types must be removed")

    contract_references = data.get("contract_references")
    if not isinstance(contract_references, dict):
        raise ValidationError(f"{path}: contract_references must be an object")
    expected_refs = {
        "elements": "phase2_srs_elements.json",
        "generation": "phase2_srs_generation.json",
        "movement_rule_issue": 1089,
    }
    if contract_references != expected_refs:
        raise ValidationError(f"{path}: contract_references must match {expected_refs}")

    baseline = data.get("baseline")
    if not isinstance(baseline, dict):
        raise ValidationError(f"{path}: baseline must be an object")
    missing_baseline = REQUIRED_BASELINE_KEYS - set(baseline)
    if missing_baseline:
        raise ValidationError(f"{path}: baseline missing {sorted(missing_baseline)}")
    for dimension in ("width", "height"):
        value = baseline[dimension]
        if isinstance(value, bool) or not isinstance(value, int) or value < 5 or value % 2 == 0:
            raise ValidationError(f"{path}: baseline {dimension} must be an odd integer >= 5")
    if [baseline["width"], baseline["height"]] != [9, 9]:
        raise ValidationError(f"{path}: baseline map size must be 9x9")
    if baseline["generation_profile"] != "phase2_srs_generation.json":
        raise ValidationError(f"{path}: generation_profile must reference phase2_srs_generation.json")
    if baseline["generation_schema_version"] != 1:
        raise ValidationError(f"{path}: baseline generation_schema_version must be 1")
    if baseline["movement_rule"] not in EXPECTED_MOVEMENT_RULES:
        raise ValidationError(f"{path}: invalid baseline movement_rule")
    if baseline["path_input_mode"] not in EXPECTED_PATH_INPUT_MODES:
        raise ValidationError(f"{path}: invalid baseline path_input_mode")
    if baseline["collision_behavior"] != "STOP_BEFORE":
        raise ValidationError(f"{path}: baseline collision_behavior must be STOP_BEFORE")
    if baseline["sector_value_route"] not in {"DIRECT_EXIT", "VALUE_OBJECT_DETOUR"}:
        raise ValidationError(f"{path}: invalid baseline sector_value_route")
    movement_points = baseline["movement_points_per_turn"]
    if not isinstance(movement_points, int) or movement_points < 1:
        raise ValidationError(f"{path}: movement_points_per_turn must be an integer >= 1")

    comparisons = data.get("comparisons")
    if not isinstance(comparisons, dict) or set(comparisons) != EXPECTED_COMPARISONS:
        raise ValidationError(f"{path}: comparisons must be C1..C8")
    for comparison_id, comparison in comparisons.items():
        if not isinstance(comparison, dict):
            raise ValidationError(f"{path}: {comparison_id} must be an object")
        if not isinstance(comparison.get("field"), str) or not comparison["field"]:
            raise ValidationError(f"{path}: {comparison_id}.field must be non-empty")
        values = comparison.get("values")
        if not isinstance(values, list) or len(values) < 2 or len({json.dumps(v, sort_keys=True) for v in values}) != len(values):
            raise ValidationError(f"{path}: {comparison_id}.values must contain distinct values")
    if comparisons["C1"] != {"field": "map_size", "values": [[9, 9], [11, 11]]}:
        raise ValidationError(f"{path}: C1 must compare 9x9 and 11x11")
    if comparisons["C5"] != {
        "field": "sector_value_route",
        "values": ["DIRECT_EXIT", "VALUE_OBJECT_DETOUR"],
    }:
        raise ValidationError(f"{path}: C5 must compare sector_value_route")
    if comparisons["C7"] != {
        "field": "sector_type",
        "values": ["NORMAL", "BASE", "RESOURCE", "NEBULA", "ASTEROID", "GRAVITY", "RIFT"],
    }:
        raise ValidationError(f"{path}: C7 must compare all sector types")
    if comparisons["C8"].get("field") != "movement_rule":
        raise ValidationError(f"{path}: C8 must compare movement_rule")
    if set(comparisons["C8"].get("values", [])) != EXPECTED_MOVEMENT_RULES:
        raise ValidationError(f"{path}: C8 must compare all movement rules")

    persistent = set(data.get("persistent_fields", []))
    if persistent != REQUIRED_PERSISTENT_FIELDS:
        raise ValidationError(
            f"{path}: persistent_fields must be {sorted(REQUIRED_PERSISTENT_FIELDS)}"
        )
    return data


def validate_questions(path: Path, values: dict[str, Any]) -> None:
    try:
        with path.open(encoding="utf-8", newline="") as file:
            reader = csv.DictReader(file)
            if reader.fieldnames != QUESTION_FIELDS:
                raise ValidationError(f"{path}: columns must exactly match {QUESTION_FIELDS}")
            rows = list(reader)
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc

    ids = {row.get("question_id", "") for row in rows}
    if ids != EXPECTED_QUESTIONS or len(rows) != 16:
        raise ValidationError(f"{path}: questions must contain Q1..Q16 exactly once")

    comparisons = set(values["comparisons"])
    for row in rows:
        label = row["question_id"]
        for field in QUESTION_FIELDS:
            if not row.get(field, "").strip():
                raise ValidationError(f"{path}: {label}.{field} must not be blank")
        unknown_comparisons = split_semicolon(row["comparison_ids"]) - comparisons
        if unknown_comparisons:
            raise ValidationError(f"{path}: {label} has unknown comparisons {sorted(unknown_comparisons)}")
        sectors = split_semicolon(row["required_sector_types"])
        if not sectors or not sectors <= EXPECTED_SECTOR_TYPES:
            raise ValidationError(f"{path}: {label} has invalid required_sector_types")
        if not split_semicolon(row["automated_metrics"]):
            raise ValidationError(f"{path}: {label} needs automated metrics")
        if not split_semicolon(row["manual_scores"]):
            raise ValidationError(f"{path}: {label} needs manual scores")
        if not split_semicolon(row["required_fixtures"]):
            raise ValidationError(f"{path}: {label} needs fixtures")

    movement_question_ids = {"Q11", "Q12", "Q13", "Q14", "Q15", "Q16"}
    movement_rows = {row["question_id"]: row for row in rows if row["question_id"] in movement_question_ids}
    if set(movement_rows) != movement_question_ids:
        raise ValidationError(f"{path}: movement questions Q11..Q16 are required")
    for question_id, row in movement_rows.items():
        if "C8" not in split_semicolon(row["comparison_ids"]):
            raise ValidationError(f"{path}: {question_id} must include C8")


def validate_model(path: Path) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    if "TBD" in text:
        raise ValidationError(f"{path}: TBD must not remain")
    required_tokens = [
        "NEBULA",
        "ASTEROID",
        "GRAVITY",
        "RIFT",
        "blocked_edges",
        "SrsTerrainType",
        "SrsObjectType",
        "SrsActorType",
        "warp_flags",
        "STAR",
        "PLANET",
        "9x9",
        "11x11",
        "generation_profile_ref",
        "phase2_srs_elements.json",
        "phase2_srs_generation.json",
        "#1097",
        "#1098",
    ]
    for token in required_tokens:
        if token not in text:
            raise ValidationError(f"{path}: required token missing: {token}")


def validate_all(model: Path, questions: Path, values_path: Path) -> None:
    values = validate_values(values_path)
    validate_questions(questions, values)
    validate_model(model)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        validate_all(args.model, args.questions, args.values)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 2 SRS initial model: OK")
    print("questions: 16")
    print("comparisons: 8")
    print("sector types: 7")
    print("movement rules: 3")
    print("TBD: 0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
