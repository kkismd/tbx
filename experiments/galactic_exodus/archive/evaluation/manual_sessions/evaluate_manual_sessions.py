#!/usr/bin/env python3
"""Validate Galactic Exodus manual-session CSV rows against GameLog v3 JSON logs."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path
from typing import Any


EXPECTED_SCHEMA_VERSION = 3
FIELDNAMES = [
    "session_id",
    "player_id",
    "requested_seed",
    "effective_seed",
    "outcome",
    "turn_count",
    "remaining_fuel",
    "base_visit_count",
    "base_refuel_count",
    "resource_visit_count",
    "resource_refuel_count",
    "rift_attempts",
    "route_decision_score",
    "information_score",
    "fuel_tension_score",
    "supply_choice_score",
    "rift_fairness_score",
    "readability_score",
    "defeat_clarity_score",
    "observation_range_score",
    "resource_reveal_score",
    "rift_asymmetry_score",
    "base_return_value_score",
    "base_loop_risk_score",
    "notes",
    "log_path",
]
OBJECTIVE_FIELDS = [
    "requested_seed",
    "effective_seed",
    "outcome",
    "turn_count",
    "remaining_fuel",
    "base_visit_count",
    "base_refuel_count",
    "resource_visit_count",
    "resource_refuel_count",
    "rift_attempts",
]
SCORE_FIELDS = [
    "route_decision_score",
    "information_score",
    "fuel_tension_score",
    "supply_choice_score",
    "rift_fairness_score",
    "readability_score",
    "defeat_clarity_score",
    "observation_range_score",
    "resource_reveal_score",
    "rift_asymmetry_score",
    "base_return_value_score",
    "base_loop_risk_score",
]


class ValidationError(ValueError):
    """Raised when CSV or JSON contents violate the manual-session contract."""


def reject_replacement_character(value: str, label: str) -> None:
    if "\uFFFD" in value:
        raise ValidationError(
            f"{label} must not contain the Unicode replacement character U+FFFD"
        )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate prototype_manual_sessions.csv against GameLog v3 JSON logs."
    )
    parser.add_argument(
        "--csv",
        type=Path,
        default=Path("experiments/galactic_exodus/results/prototype_manual_sessions.csv"),
        help="Manual-session CSV path",
    )
    parser.add_argument(
        "--seed-start",
        type=int,
        default=1,
        help="First requested_seed expected in the CSV",
    )
    parser.add_argument(
        "--seed-end",
        type=int,
        default=10,
        help="Last requested_seed expected in the CSV",
    )
    return parser.parse_args(argv)


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValidationError(f"{label} must be an object")
    return value


def require_int(mapping: dict[str, Any], key: str, label: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValidationError(f"{label}.{key} must be an integer")
    return value


def require_nonempty_string(mapping: dict[str, Any], key: str, label: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value:
        raise ValidationError(f"{label}.{key} must be a non-empty string")
    return value


def parse_row_int(row: dict[str, str], key: str, label: str) -> int:
    raw = row.get(key, "")
    if raw == "":
        raise ValidationError(f"{label}.{key} must not be empty")
    try:
        return int(raw)
    except ValueError as exc:
        raise ValidationError(f"{label}.{key} must be an integer") from exc


def load_csv_rows(path: Path) -> list[dict[str, str]]:
    try:
        with path.open(encoding="utf-8", newline="") as file:
            reader = csv.DictReader(file)
            if reader.fieldnames != FIELDNAMES:
                raise ValidationError(f"{path}: CSV columns do not match the expected schema")
            return list(reader)
    except FileNotFoundError as exc:
        raise ValidationError(f"missing CSV file: {path}") from exc


def validate_row_shape(
    row: dict[str, str],
    row_index: int,
    expected_seed: int,
) -> None:
    label = f"CSV row {row_index}"
    for field in FIELDNAMES:
        if row.get(field, "") == "":
            raise ValidationError(f"{label}.{field} must not be empty")

    requested_seed = parse_row_int(row, "requested_seed", label)
    if requested_seed != expected_seed:
        raise ValidationError(
            f"{label}.requested_seed expected {expected_seed}, got {requested_seed}"
        )

    expected_session_id = f"manual-{expected_seed:03d}"
    if row["session_id"] != expected_session_id:
        raise ValidationError(
            f"{label}.session_id expected {expected_session_id}, got {row['session_id']}"
        )
    if not row["player_id"].strip():
        raise ValidationError(f"{label}.player_id must not be blank")
    reject_replacement_character(row["player_id"], f"{label}.player_id")

    for field in SCORE_FIELDS:
        score = row[field]
        if score not in {"1", "2", "3", "4", "5"}:
            raise ValidationError(f"{label}.{field} must be one of 1..5")

    if not row["notes"].strip():
        raise ValidationError(f"{label}.notes must not be blank")
    reject_replacement_character(row["notes"], f"{label}.notes")


def load_log_summary(path: Path, expected_seed: int) -> dict[str, int | str]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValidationError(f"missing JSON log: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ValidationError(
            f"invalid JSON in {path}: line {exc.lineno}, column {exc.colno}"
        ) from exc

    root = require_mapping(payload, str(path))
    schema_version = require_int(root, "schema_version", str(path))
    if schema_version != EXPECTED_SCHEMA_VERSION:
        raise ValidationError(
            f"{path}: expected schema_version={EXPECTED_SCHEMA_VERSION}, got {schema_version}"
        )

    requested_seed = require_int(root, "requested_seed", str(path))
    if requested_seed != expected_seed:
        raise ValidationError(
            f"{path}: expected requested_seed={expected_seed}, got {requested_seed}"
        )
    if root.get("generation_error") is not None:
        raise ValidationError(f"{path}: generation_error is present")

    final_summary = require_mapping(root.get("final_summary"), f"{path}.final_summary")
    return {
        "requested_seed": requested_seed,
        "effective_seed": require_int(root, "effective_seed", str(path)),
        "outcome": require_nonempty_string(final_summary, "outcome", f"{path}.final_summary"),
        "turn_count": require_int(final_summary, "turn_count", f"{path}.final_summary"),
        "remaining_fuel": require_int(
            final_summary, "remaining_fuel", f"{path}.final_summary"
        ),
        "base_visit_count": require_int(
            final_summary, "base_visit_count", f"{path}.final_summary"
        ),
        "base_refuel_count": require_int(
            final_summary, "base_refuel_count", f"{path}.final_summary"
        ),
        "resource_visit_count": require_int(
            final_summary, "resource_visit_count", f"{path}.final_summary"
        ),
        "resource_refuel_count": require_int(
            final_summary, "resource_refuel_count", f"{path}.final_summary"
        ),
        "rift_attempts": require_int(final_summary, "rift_attempts", f"{path}.final_summary"),
    }


def compare_row_to_log(
    row: dict[str, str],
    row_index: int,
    log_summary: dict[str, int | str],
) -> None:
    label = f"CSV row {row_index}"
    for field in OBJECTIVE_FIELDS:
        row_value = row[field]
        log_value = log_summary[field]
        if isinstance(log_value, int):
            if parse_row_int(row, field, label) != log_value:
                raise ValidationError(
                    f"{label}.{field} does not match log: csv={row_value} log={log_value}"
                )
            continue
        if row_value != log_value:
            raise ValidationError(
                f"{label}.{field} does not match log: csv={row_value} log={log_value}"
            )


def validate_manual_sessions(
    csv_path: Path,
    seed_start: int,
    seed_end: int,
) -> int:
    if seed_start > seed_end:
        raise ValidationError("--seed-start must be <= --seed-end")

    rows = load_csv_rows(csv_path)
    expected_seeds = list(range(seed_start, seed_end + 1))
    if len(rows) != len(expected_seeds):
        raise ValidationError(
            f"{csv_path}: expected {len(expected_seeds)} rows, got {len(rows)}"
        )

    for index, (row, expected_seed) in enumerate(zip(rows, expected_seeds), start=1):
        validate_row_shape(row, index, expected_seed)
        log_path = Path(row["log_path"])
        log_summary = load_log_summary(log_path, expected_seed)
        compare_row_to_log(row, index, log_summary)

    return len(rows)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        row_count = validate_manual_sessions(args.csv, args.seed_start, args.seed_end)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(f"manual sessions: OK ({row_count} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
