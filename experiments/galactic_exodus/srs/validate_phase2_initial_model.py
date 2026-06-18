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
EXPECTED_BASELINE_MAP_SIZE = [9, 9]
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
    "LOCAL_3X3",
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
]
LEGACY_SUBSTRING_TOKENS = [
    "obstacle_density",
    "seven_by_seven",
    "warp_point",
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
        "manual_scores": {
            "sector_identity_score",
            "terrain_density_naturalness_score",
        },
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {
            "all_sectors_9x9_multi_seed",
            "all_sectors_11x11_multi_seed",
        },
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
        "manual_scores": {
            "navigation_readability_score",
            "object_placement_value_score",
        },
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {
            "all_sectors_9x9_multi_seed",
            "all_sectors_11x11_multi_seed",
        },
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
        "manual_scores": {
            "generation_stability_score",
            "failure_diagnosability_score",
        },
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {
            "all_sectors_9x9_seed_batch",
            "all_sectors_11x11_seed_batch",
        },
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
        "manual_scores": {
            "reproducibility_confidence_score",
            "debug_traceability_score",
        },
        "required_sector_types": EXPECTED_SECTOR_TYPES,
        "required_fixtures": {
            "all_sectors_9x9_repeat_seed_batch",
            "all_sectors_11x11_repeat_seed_batch",
        },
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


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


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
    if comparisons.get("C5") != {
        "field": "sector_value_route",
        "values": ["DIRECT_EXIT", "VALUE_OBJECT_DETOUR"],
    }:
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
        raise ValidationError(
            f"{path}: persistent_fields must be {sorted(REQUIRED_PERSISTENT_FIELDS)}"
        )
    return data


def load_elements(path: Path) -> dict[str, Any]:
    data = load_json(path)
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
        for field in (
            "comparison_ids",
            "automated_metrics",
            "manual_scores",
            "required_sector_types",
            "required_fixtures",
        ):
            actual = split_semicolon(row[field])
            if actual != expected[field]:
                raise ValidationError(f"{path}: {question_id}.{field} must match the Phase 2A1c contract")
        for token in expected["decision_rule_contains"]:
            if token not in row["decision_rule"]:
                raise ValidationError(f"{path}: {question_id}.decision_rule must contain {token}")


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
    if ids != EXPECTED_QUESTIONS or len(rows) != 20:
        raise ValidationError(f"{path}: questions must contain Q1..Q20 exactly once")

    for row in rows:
        validate_question_row(path, row, values)

    rows_by_id = {row["question_id"]: row for row in rows}
    movement_question_ids = {"Q11", "Q12", "Q13", "Q14", "Q15", "Q16"}
    movement_rows = {question_id: rows_by_id[question_id] for question_id in movement_question_ids}
    if set(movement_rows) != movement_question_ids:
        raise ValidationError(f"{path}: movement questions Q11..Q16 are required")
    for question_id, row in movement_rows.items():
        if "C8" not in split_semicolon(row["comparison_ids"]):
            raise ValidationError(f"{path}: {question_id} must include C8")

    validate_fixed_question_contracts(path, rows_by_id)


def validate_model(path: Path) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc

    if "TBD" in text:
        raise ValidationError(f"{path}: TBD must not remain")
    for token in REQUIRED_MODEL_TOKENS:
        if token not in text:
            raise ValidationError(f"{path}: required token missing: {token}")


def find_legacy_token_in_text(text: str) -> str | None:
    for token in LEGACY_BOUNDARY_TOKENS:
        pattern = rf"(?<![A-Za-z0-9_]){re.escape(token)}(?![A-Za-z0-9_])"
        if re.search(pattern, text):
            return token
    for token in LEGACY_SUBSTRING_TOKENS:
        if token in text:
            return token
    return None


def validate_legacy_text(path: Path) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    token = find_legacy_token_in_text(text)
    if token is not None:
        raise ValidationError(f"{path}: forbidden legacy token remains: {token}")


def validate_legacy_json(
    path: Path,
    data: dict[str, Any],
    *,
    allow_removed_contracts: bool = False,
    ignored_prefixes: tuple[tuple[str, ...], ...] = (),
    ignored_tokens: frozenset[str] = frozenset(),
) -> None:
    def walk(value: Any, components: tuple[str, ...]) -> None:
        if allow_removed_contracts and components[:2] == ("root", "legacy_contracts_removed"):
            return
        if any(components[: len(prefix)] == prefix for prefix in ignored_prefixes):
            return
        if isinstance(value, dict):
            for key, nested in value.items():
                child_components = (*components, str(key))
                if any(child_components[: len(prefix)] == prefix for prefix in ignored_prefixes):
                    continue
                token = find_legacy_token_in_text(str(key))
                if token is not None and token not in ignored_tokens:
                    raise ValidationError(f"{path}: forbidden legacy token remains: {token}")
                walk(nested, child_components)
            return
        if isinstance(value, list):
            for index, nested in enumerate(value):
                walk(nested, (*components, str(index)))
            return
        if isinstance(value, str):
            token = find_legacy_token_in_text(value)
            if token is not None and token not in ignored_tokens:
                raise ValidationError(f"{path}: forbidden legacy token remains: {token}")

    walk(data, ("root",))


def validate_cross_file(
    values_path: Path,
    values: dict[str, Any],
    elements_path: Path,
    elements: dict[str, Any],
    generation_path: Path,
    generation: dict[str, Any],
) -> None:
    if values.get("generation_schema_version") != generation.get("generation_schema_version"):
        raise ValidationError(f"{values_path}: generation_schema_version must match generation")
    if elements.get("schema_version") != 1:
        raise ValidationError(f"{elements_path}: schema_version must be 1")
    if values.get("generation_schema_version") != 1:
        raise ValidationError(f"{values_path}: generation_schema_version must be 1")

    if elements.get("map_sizes") != EXPECTED_MAP_SIZES:
        raise ValidationError(f"{elements_path}: map_sizes must be 9x9 and 11x11")
    if generation.get("map_sizes") != EXPECTED_MAP_SIZES:
        raise ValidationError(f"{generation_path}: map_sizes must be 9x9 and 11x11")
    if elements.get("map_sizes") != generation.get("map_sizes"):
        raise ValidationError(f"{elements_path}: map_sizes must match generation")
    if elements.get("baseline_map_size") != EXPECTED_BASELINE_MAP_SIZE:
        raise ValidationError(f"{elements_path}: baseline_map_size must be [9, 9]")
    baseline = values["baseline"]
    baseline_size = [baseline["width"], baseline["height"]]
    if baseline_size not in generation.get("map_sizes", []):
        raise ValidationError(f"{values_path}: baseline size must exist in generation.map_sizes")
    if values["comparisons"].get("C1") != {"field": "map_size", "values": EXPECTED_MAP_SIZES}:
        raise ValidationError(f"{values_path}: C1 must compare 9x9 and 11x11")

    value_sector_types = set(values["sector_types"])
    generation_sector_types = set(generation.get("sector_types", []))
    generation_profile_types = set(generation.get("sector_profiles", {}).keys())
    element_sector_types = set(elements.get("sector_terrain_matrix", {}).keys())
    if value_sector_types != EXPECTED_SECTOR_TYPES:
        raise ValidationError(f"{values_path}: sector_types must be {sorted(EXPECTED_SECTOR_TYPES)}")
    if generation_sector_types != value_sector_types:
        raise ValidationError(f"{generation_path}: sector_types must match values.sector_types")
    if generation_profile_types != value_sector_types:
        raise ValidationError(f"{generation_path}: sector_profiles must match values.sector_types")
    if element_sector_types != value_sector_types:
        raise ValidationError(f"{elements_path}: sector_terrain_matrix must match values.sector_types")

    value_terrain_types = set(values["terrain_types"])
    generation_terrain_types = set(generation.get("terrain_types", []))
    element_terrain_types = set(elements.get("terrain_types", {}).keys())
    if value_terrain_types != EXPECTED_TERRAIN_TYPES:
        raise ValidationError(f"{values_path}: terrain_types must be {sorted(EXPECTED_TERRAIN_TYPES)}")
    if generation_terrain_types != value_terrain_types:
        raise ValidationError(f"{generation_path}: terrain_types must match values.terrain_types")
    if element_terrain_types != value_terrain_types:
        raise ValidationError(f"{elements_path}: terrain_types must match values.terrain_types")

    value_object_types = set(values["object_types"])
    generation_object_types = set(generation.get("object_types", []))
    element_object_types = set(elements.get("object_types", {}).keys())
    if value_object_types != EXPECTED_OBJECT_TYPES:
        raise ValidationError(f"{values_path}: object_types must be {sorted(EXPECTED_OBJECT_TYPES)}")
    if generation_object_types != value_object_types:
        raise ValidationError(f"{generation_path}: object_types must match values.object_types")
    if element_object_types != value_object_types:
        raise ValidationError(f"{elements_path}: object_types must match values.object_types")

    contract_references = values.get("contract_references")
    if contract_references != EXPECTED_CONTRACT_REFERENCES:
        raise ValidationError(f"{values_path}: contract_references must match the schema 3 contract")
    if contract_references.get("elements") != elements_path.name:
        raise ValidationError(f"{values_path}: contract_references.elements must match {elements_path.name}")
    if contract_references.get("generation") != generation_path.name:
        raise ValidationError(
            f"{values_path}: contract_references.generation must match {generation_path.name}"
        )
    if contract_references.get("movement_rule_issue") != 1089:
        raise ValidationError(f"{values_path}: movement_rule_issue must be 1089")

    reachability = generation.get("global_generation_contract", {}).get("reachability", {})
    if reachability.get("movement_rule_reference") != EXPECTED_MOVEMENT_RULE_REFERENCE:
        raise ValidationError(
            f"{generation_path}: movement_rule_reference must match issue 1089 PASSABLE_ADJACENCY"
        )

    required_profile_keys = {
        "required_terrain",
        "optional_terrain",
        "forbidden_terrain",
        "object_count_ranges",
        "placement_constraints",
    }
    sector_profiles = generation.get("sector_profiles", {})
    if set(sector_profiles) != value_sector_types:
        raise ValidationError(f"{generation_path}: sector_profiles must match values.sector_types")
    for sector_type, profile in sector_profiles.items():
        if not isinstance(profile, dict):
            raise ValidationError(f"{generation_path}: {sector_type} profile must be an object")
        missing_keys = required_profile_keys - set(profile)
        if missing_keys:
            raise ValidationError(
                f"{generation_path}: {sector_type} profile missing {sorted(missing_keys)}"
            )

    sector_matrix = elements.get("sector_terrain_matrix", {})
    if set(sector_matrix) != value_sector_types:
        raise ValidationError(f"{elements_path}: sector_terrain_matrix must match values.sector_types")
    for sector_type, terrain_rules in sector_matrix.items():
        if not isinstance(terrain_rules, dict):
            raise ValidationError(f"{elements_path}: sector_terrain_matrix.{sector_type} must be an object")
        for field in ("required", "optional", "forbidden"):
            terrain_names = set(terrain_rules.get(field, []))
            if not terrain_names <= value_terrain_types:
                raise ValidationError(
                    f"{elements_path}: sector_terrain_matrix.{sector_type}.{field} must stay within values.terrain_types"
                )
        if "edge_required" in terrain_rules:
            edge_required = set(terrain_rules.get("edge_required", []))
            if not edge_required <= value_terrain_types:
                raise ValidationError(
                    f"{elements_path}: sector_terrain_matrix.{sector_type}.edge_required must stay within values.terrain_types"
                )

    terrain_matrix = elements.get("terrain_object_matrix", {})
    if set(terrain_matrix) != value_terrain_types:
        raise ValidationError(f"{elements_path}: terrain_object_matrix must match values.terrain_types")
    for terrain_type, object_names in terrain_matrix.items():
        if not set(object_names) <= value_object_types:
            raise ValidationError(
                f"{elements_path}: terrain_object_matrix.{terrain_type} must stay within values.object_types"
            )


def validate_all(
    model: Path,
    questions: Path,
    values_path: Path,
    elements_path: Path,
    generation_path: Path,
) -> None:
    validate_legacy_text(model)
    validate_legacy_text(questions)
    raw_values = load_json(values_path)
    raw_elements = load_elements(elements_path)
    raw_generation = load_json(generation_path)
    validate_legacy_json(values_path, raw_values)
    validate_legacy_json(
        elements_path,
        raw_elements,
        ignored_prefixes=(("root", "warp_point"),),
        ignored_tokens=frozenset({"WARP_POINT", "warp_point"}),
    )
    validate_legacy_json(generation_path, raw_generation, allow_removed_contracts=True)
    generation = validate_generation(generation_path)
    values = validate_values(values_path)
    elements = load_elements(elements_path)

    validate_cross_file(values_path, values, elements_path, elements, generation_path, generation)
    validate_questions(questions, values)
    validate_model(model)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        validate_all(
            args.model,
            args.questions,
            args.values,
            args.elements,
            args.generation,
        )
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 2 SRS initial model: OK")
    print("questions: 20")
    print("comparisons: 8")
    print("sector types: 7")
    print("movement rules: 3")
    print("cross-file: OK")
    print("TBD: 0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
