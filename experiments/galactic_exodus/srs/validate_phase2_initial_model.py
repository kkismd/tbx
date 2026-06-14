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
EXPECTED_QUESTIONS = {f"Q{index}" for index in range(1, 11)}
EXPECTED_COMPARISONS = {f"C{index}" for index in range(1, 8)}
EXPECTED_SECTOR_TYPES = {"NORMAL", "BASE", "RESOURCE", "RIFT"}
EXPECTED_DIRECTIONS = {"N", "E", "S", "W"}
REQUIRED_BASELINE_KEYS = {
    "width",
    "height",
    "entry_width",
    "obstacle_density",
    "observation_mode",
    "cost_mode",
    "interaction_mode",
    "object_profile",
    "rift_knowledge_mode",
    "max_srs_turns",
}
REQUIRED_PERSISTENT_FIELDS = {
    "generated_map_id",
    "sector_type",
    "blocked_edges",
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


def validate_values(path: Path) -> dict[str, Any]:
    data = load_json(path)
    if data.get("schema_version") != 1:
        raise ValidationError(f"{path}: schema_version must be 1")

    sector_types = set(data.get("sector_types", []))
    if sector_types != EXPECTED_SECTOR_TYPES:
        raise ValidationError(f"{path}: sector_types must be {sorted(EXPECTED_SECTOR_TYPES)}")
    directions = set(data.get("directions", []))
    if directions != EXPECTED_DIRECTIONS:
        raise ValidationError(f"{path}: directions must be {sorted(EXPECTED_DIRECTIONS)}")

    invariants = data.get("invariants")
    if not isinstance(invariants, dict):
        raise ValidationError(f"{path}: invariants must be an object")
    if invariants.get("rift_blocked_edge_min") != 1:
        raise ValidationError(f"{path}: RIFT blocked-edge minimum must be 1")
    if invariants.get("rift_blocked_edge_max") != 3:
        raise ValidationError(f"{path}: RIFT blocked-edge maximum must be 3")
    if invariants.get("non_rift_blocked_edges") != []:
        raise ValidationError(f"{path}: non-RIFT blocked edges must be empty")
    for key in ("entry_must_be_open", "selected_exit_must_be_open", "odd_map_dimensions_only"):
        if invariants.get(key) is not True:
            raise ValidationError(f"{path}: invariant {key} must be true")

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
    if baseline["entry_width"] != 1:
        raise ValidationError(f"{path}: initial entry_width must be 1")

    comparisons = data.get("comparisons")
    if not isinstance(comparisons, dict) or set(comparisons) != EXPECTED_COMPARISONS:
        raise ValidationError(f"{path}: comparisons must be C1..C7")
    for comparison_id, comparison in comparisons.items():
        if not isinstance(comparison, dict):
            raise ValidationError(f"{path}: {comparison_id} must be an object")
        if not isinstance(comparison.get("field"), str) or not comparison["field"]:
            raise ValidationError(f"{path}: {comparison_id}.field must be non-empty")
        values = comparison.get("values")
        if not isinstance(values, list) or len(values) != 2 or values[0] == values[1]:
            raise ValidationError(f"{path}: {comparison_id}.values must contain two distinct values")

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
    if ids != EXPECTED_QUESTIONS or len(rows) != 10:
        raise ValidationError(f"{path}: questions must contain Q1..Q10 exactly once")

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


def validate_model(path: Path) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    if "TBD" in text:
        raise ValidationError(f"{path}: TBD must not remain")
    required_tokens = [
        "RIFT",
        "blocked_edges",
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
        "C7",
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
    print("questions: 10")
    print("comparisons: 7")
    print("sector types: 4")
    print("TBD: 0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
