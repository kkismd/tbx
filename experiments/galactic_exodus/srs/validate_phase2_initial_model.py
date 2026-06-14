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
    "WALL",
    "STATION_STRUCTURE",
    "DEBRIS",
    "NEBULA",
    "ASTEROID_FIELD",
    "ASTEROID",
    "GRAVITY_FIELD",
    "RIFT_DISTORTION",
    "RIFT_BARRIER",
}
EXPECTED_FEATURE_TYPES = {"WARP_POINT"}
EXPECTED_OBJECT_TYPES = {"STAR", "PLANET", "BASE_NODE", "RESOURCE_CACHE", "SALVAGE"}
EXPECTED_ACTOR_TYPES = {"PLAYER"}
EXPECTED_MOVEMENT_RULES = {"VECTOR_COMMAND", "MOVEMENT_POINTS", "DIRECTIONAL_THRUST"}
EXPECTED_PATH_INPUT_MODES = {"STEPWISE_ROUTE", "DESTINATION_AUTO_PATH", "ROUTE_PREVIEW"}
REQUIRED_BASELINE_KEYS = {
    "width",
    "height",
    "warp_point_width",
    "warp_clearance_depth",
    "obstacle_density",
    "observation_mode",
    "cost_mode",
    "interaction_mode",
    "object_profile",
    "rift_knowledge_mode",
    "movement_rule",
    "movement_points_per_turn",
    "path_input_mode",
    "collision_behavior",
    "max_srs_turns",
}
REQUIRED_PERSISTENT_FIELDS = {
    "generated_map_id",
    "sector_type",
    "blocked_edges",
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


def validate_celestial_profiles(path: Path, data: dict[str, Any]) -> None:
    profiles = data.get("celestial_body_profiles")
    if not isinstance(profiles, dict) or set(profiles) != {"7x7", "9x9"}:
        raise ValidationError(f"{path}: celestial_body_profiles must define 7x7 and 9x9")
    for size, profile in profiles.items():
        if not isinstance(profile, dict) or set(profile) != {"STAR", "PLANET"}:
            raise ValidationError(f"{path}: {size} celestial profile must define STAR and PLANET")
        if profile["STAR"].get("count") != 1:
            raise ValidationError(f"{path}: {size} STAR count must be exactly 1")
        planet = profile["PLANET"]
        minimum = planet.get("count_min")
        maximum = planet.get("count_max")
        if not isinstance(minimum, int) or not isinstance(maximum, int) or minimum < 1 or maximum < minimum:
            raise ValidationError(f"{path}: {size} PLANET range must be valid and start at 1 or more")


def validate_element_definitions(path: Path, data: dict[str, Any]) -> None:
    definitions = data.get("element_definitions")
    if not isinstance(definitions, dict):
        raise ValidationError(f"{path}: element_definitions must be an object")
    for object_type in ("STAR", "PLANET"):
        definition = definitions.get(object_type)
        if not isinstance(definition, dict):
            raise ValidationError(f"{path}: missing {object_type} definition")
        if definition.get("passable") is not False:
            raise ValidationError(f"{path}: {object_type} must be impassable")
        if definition.get("blocks_line_travel") is not True:
            raise ValidationError(f"{path}: {object_type} must block line travel")
        if definition.get("persistent_after_revisit") is not True:
            raise ValidationError(f"{path}: {object_type} must persist after revisit")
        if definition.get("collision_behavior") != "STOP_BEFORE":
            raise ValidationError(f"{path}: {object_type} collision behavior must be STOP_BEFORE")
        if set(definition.get("allowed_sector_types", [])) != EXPECTED_SECTOR_TYPES:
            raise ValidationError(f"{path}: {object_type} must be allowed in every sector type")
    warp = definitions.get("WARP_POINT")
    if not isinstance(warp, dict):
        raise ValidationError(f"{path}: missing WARP_POINT definition")
    if warp.get("passable") is not True or warp.get("placement") != "EDGE_MIDPOINT":
        raise ValidationError(f"{path}: WARP_POINT must be passable and placed at edge midpoint")
    if warp.get("can_host_object") is not False:
        raise ValidationError(f"{path}: WARP_POINT must not host objects")


def validate_values(path: Path) -> dict[str, Any]:
    data = load_json(path)
    if data.get("schema_version") != 2:
        raise ValidationError(f"{path}: schema_version must be 2")

    require_exact_set(path, data, "sector_types", EXPECTED_SECTOR_TYPES)
    require_exact_set(path, data, "directions", EXPECTED_DIRECTIONS)
    require_exact_set(path, data, "terrain_types", EXPECTED_TERRAIN_TYPES)
    require_exact_set(path, data, "feature_types", EXPECTED_FEATURE_TYPES)
    require_exact_set(path, data, "object_types", EXPECTED_OBJECT_TYPES)
    require_exact_set(path, data, "actor_types", EXPECTED_ACTOR_TYPES)
    require_exact_set(path, data, "movement_rules", EXPECTED_MOVEMENT_RULES)
    require_exact_set(path, data, "path_input_modes", EXPECTED_PATH_INPUT_MODES)

    invariants = data.get("invariants")
    if not isinstance(invariants, dict):
        raise ValidationError(f"{path}: invariants must be an object")
    if invariants.get("rift_blocked_edge_min") != 1:
        raise ValidationError(f"{path}: RIFT blocked-edge minimum must be 1")
    if invariants.get("rift_blocked_edge_max") != 3:
        raise ValidationError(f"{path}: RIFT blocked-edge maximum must be 3")
    if invariants.get("non_rift_blocked_edges") != []:
        raise ValidationError(f"{path}: non-RIFT blocked edges must be empty")
    for key in (
        "warp_point_must_be_open",
        "selected_warp_direction_must_be_open",
        "odd_map_dimensions_only",
        "warp_point_at_edge_midpoint",
        "all_warp_points_connected",
    ):
        if invariants.get(key) is not True:
            raise ValidationError(f"{path}: invariant {key} must be true")
    if invariants.get("star_count") != 1:
        raise ValidationError(f"{path}: invariant star_count must be 1")
    if invariants.get("planet_count_min", 0) < 1:
        raise ValidationError(f"{path}: invariant planet_count_min must be at least 1")
    if invariants.get("warp_point_object_overlap_allowed") is not False:
        raise ValidationError(f"{path}: WARP_POINT/object overlap must be forbidden")

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
    density = baseline["obstacle_density"]
    if not isinstance(density, (int, float)) or isinstance(density, bool) or not 0 <= density < 1:
        raise ValidationError(f"{path}: obstacle_density must be in [0, 1)")
    if baseline["warp_point_width"] != 1:
        raise ValidationError(f"{path}: initial warp_point_width must be 1")
    if not isinstance(baseline["warp_clearance_depth"], int) or baseline["warp_clearance_depth"] < 1:
        raise ValidationError(f"{path}: warp_clearance_depth must be an integer >= 1")
    if baseline["movement_rule"] not in EXPECTED_MOVEMENT_RULES:
        raise ValidationError(f"{path}: invalid baseline movement_rule")
    if baseline["path_input_mode"] not in EXPECTED_PATH_INPUT_MODES:
        raise ValidationError(f"{path}: invalid baseline path_input_mode")
    if baseline["collision_behavior"] != "STOP_BEFORE":
        raise ValidationError(f"{path}: baseline collision_behavior must be STOP_BEFORE")
    movement_points = baseline["movement_points_per_turn"]
    if not isinstance(movement_points, int) or movement_points < 1:
        raise ValidationError(f"{path}: movement_points_per_turn must be an integer >= 1")

    validate_celestial_profiles(path, data)
    validate_element_definitions(path, data)

    terrain_profiles = data.get("terrain_profiles")
    if not isinstance(terrain_profiles, dict) or set(terrain_profiles) != EXPECTED_SECTOR_TYPES:
        raise ValidationError(f"{path}: terrain_profiles must define every sector type")
    for sector_type, terrain_types in terrain_profiles.items():
        if not isinstance(terrain_types, list) or not terrain_types:
            raise ValidationError(f"{path}: {sector_type} terrain profile must not be empty")
        unknown = set(terrain_types) - EXPECTED_TERRAIN_TYPES
        if unknown:
            raise ValidationError(f"{path}: {sector_type} has unknown terrain types {sorted(unknown)}")

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
    if comparisons["C8"].get("field") != "movement_rule":
        raise ValidationError(f"{path}: C8 must compare movement_rule")
    if set(comparisons["C8"].get("values", [])) != EXPECTED_MOVEMENT_RULES:
        raise ValidationError(f"{path}: C8 must compare all movement rules")

    profiles = data.get("object_profiles")
    if not isinstance(profiles, dict):
        raise ValidationError(f"{path}: object_profiles must be an object")
    if set(profiles) != {"PROFILE_MINIMAL", "PROFILE_EXPLORATION"}:
        raise ValidationError(f"{path}: exactly two object profiles are required")
    for profile_name, profile in profiles.items():
        if not isinstance(profile, dict) or set(profile) != EXPECTED_SECTOR_TYPES:
            raise ValidationError(f"{path}: {profile_name} must define every sector type")

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
        "SrsFeatureType",
        "SrsObjectType",
        "SrsActorType",
        "WARP_POINT",
        "STAR",
        "PLANET",
        "STOP_BEFORE",
        "VECTOR_COMMAND",
        "MOVEMENT_POINTS",
        "DIRECTIONAL_THRUST",
        "7x7",
        "9x9",
        "LOCAL_3X3",
        "TURN_ONLY",
        "SHARED_FUEL",
        "AUTO_INTERACT",
        "EXPLICIT_INTERACT",
        "PROFILE_MINIMAL",
        "PROFILE_EXPLORATION",
        "C1",
        "C8",
        "Q1..Q16",
        "#1080",
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
