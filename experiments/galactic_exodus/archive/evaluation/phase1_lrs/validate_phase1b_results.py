#!/usr/bin/env python3
"""Validate integrated Galactic Exodus Phase 1B evaluation artifacts."""

from __future__ import annotations

import argparse
import csv
import json
import math
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

EXPECTED_SCHEMA_VERSION = 3
EXPECTED_POLICIES = ("GOAL_GREEDY", "SUPPLY_AWARE")
EXPECTED_OUTCOMES = (
    "WON",
    "LOST_FUEL",
    "ABORTED_TURN_LIMIT",
    "ABORTED_NO_POLICY_ACTION",
    "GENERATION_ERROR",
)
FINDING_FIELDS = [
    "finding_id",
    "question_id",
    "title",
    "evidence",
    "severity",
    "proposed_change",
    "affected_issues",
    "recommended_disposition",
]
SEVERITIES = {"BLOCKER", "ADJUSTMENT", "PHASE_2", "NO_CHANGE"}
RATE_COUNT_PAIRS = (
    ("win_count", "win_rate"),
    ("lost_fuel_count", "lost_fuel_rate"),
    ("aborted_turn_limit_count", "aborted_turn_limit_rate"),
    ("aborted_no_policy_action_count", "aborted_no_policy_action_rate"),
    ("generation_error_count", "generation_error_rate"),
    ("base_visit_run_count", "base_visit_rate"),
    ("base_refuel_run_count", "base_refuel_rate"),
    ("multiple_base_refuel_run_count", "multiple_base_refuel_rate"),
    ("resource_visit_run_count", "resource_visit_rate"),
    ("resource_refuel_run_count", "resource_refuel_rate"),
    ("multiple_resource_refuel_run_count", "multiple_resource_refuel_rate"),
    ("no_supply_win_count", "no_supply_win_rate"),
    ("rift_attempt_run_count", "rift_attempt_rate"),
    ("reroll_occurred_count", "reroll_rate"),
)


class ValidationError(ValueError):
    """Raised when a Phase 1B artifact violates its contract."""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manual", type=Path, required=True)
    parser.add_argument("--runs", type=Path, required=True)
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--findings", type=Path, required=True)
    return parser.parse_args(argv)


def load_csv(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    try:
        with path.open(encoding="utf-8", newline="") as file:
            reader = csv.DictReader(file)
            if reader.fieldnames is None:
                raise ValidationError(f"{path}: missing CSV header")
            return reader.fieldnames, list(reader)
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc


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


def parse_int(row: dict[str, str], field: str, label: str) -> int:
    raw = row.get(field, "")
    try:
        return int(raw)
    except ValueError as exc:
        raise ValidationError(f"{label}.{field} must be an integer") from exc


def validate_manual(path: Path) -> None:
    fields, rows = load_csv(path)
    required = {"requested_seed", "outcome", "notes"}
    if not required.issubset(fields):
        raise ValidationError(f"{path}: missing manual columns {sorted(required - set(fields))}")
    if len(rows) != 10:
        raise ValidationError(f"{path}: expected 10 rows, got {len(rows)}")
    seeds = [parse_int(row, "requested_seed", f"manual row {index}") for index, row in enumerate(rows, 1)]
    if seeds != list(range(1, 11)):
        raise ValidationError(f"{path}: requested_seed must be 1..10 in order")


def validate_runs(path: Path) -> dict[str, Counter[str]]:
    fields, rows = load_csv(path)
    required = {"policy", "requested_seed", "outcome"}
    if not required.issubset(fields):
        raise ValidationError(f"{path}: missing run columns {sorted(required - set(fields))}")
    if len(rows) != 2000:
        raise ValidationError(f"{path}: expected 2000 rows, got {len(rows)}")

    rows_by_policy: dict[str, list[dict[str, str]]] = defaultdict(list)
    counts: dict[str, Counter[str]] = {}
    for row in rows:
        rows_by_policy[row.get("policy", "")].append(row)

    if tuple(rows_by_policy) != EXPECTED_POLICIES:
        raise ValidationError(f"{path}: policy order must be {EXPECTED_POLICIES}")
    for policy in EXPECTED_POLICIES:
        policy_rows = rows_by_policy[policy]
        if len(policy_rows) != 1000:
            raise ValidationError(f"{path}: {policy} expected 1000 rows, got {len(policy_rows)}")
        seeds = [parse_int(row, "requested_seed", f"{policy} row {index}") for index, row in enumerate(policy_rows, 1)]
        if seeds != list(range(1, 1001)):
            raise ValidationError(f"{path}: {policy} requested_seed must be 1..1000 in order")
        outcomes = Counter(row.get("outcome", "") for row in policy_rows)
        unknown = set(outcomes) - set(EXPECTED_OUTCOMES)
        if unknown:
            raise ValidationError(f"{path}: {policy} has unknown outcomes {sorted(unknown)}")
        counts[policy] = outcomes
    return counts


def require_number(mapping: dict[str, Any], key: str, label: str) -> float:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValidationError(f"{label}.{key} must be numeric")
    return float(value)


def validate_summary(path: Path, run_counts: dict[str, Counter[str]]) -> None:
    summary = load_json(path)
    if summary.get("schema_version") != EXPECTED_SCHEMA_VERSION:
        raise ValidationError(f"{path}: expected schema_version={EXPECTED_SCHEMA_VERSION}")
    if summary.get("seed_start") != 1 or summary.get("seed_end") != 1000:
        raise ValidationError(f"{path}: expected seed range 1..1000")
    policies = summary.get("policies")
    if not isinstance(policies, dict) or tuple(policies) != EXPECTED_POLICIES:
        raise ValidationError(f"{path}: expected policies {EXPECTED_POLICIES}")

    outcome_keys = {
        "WON": "win_count",
        "LOST_FUEL": "lost_fuel_count",
        "ABORTED_TURN_LIMIT": "aborted_turn_limit_count",
        "ABORTED_NO_POLICY_ACTION": "aborted_no_policy_action_count",
        "GENERATION_ERROR": "generation_error_count",
    }
    for policy in EXPECTED_POLICIES:
        data = policies[policy]
        if not isinstance(data, dict):
            raise ValidationError(f"{path}: policies.{policy} must be an object")
        total = int(require_number(data, "total_runs", policy))
        if total != 1000:
            raise ValidationError(f"{path}: {policy}.total_runs expected 1000, got {total}")
        for outcome, count_key in outcome_keys.items():
            summary_count = int(require_number(data, count_key, policy))
            if summary_count != run_counts[policy][outcome]:
                raise ValidationError(
                    f"{path}: {policy}.{count_key}={summary_count} does not match runs={run_counts[policy][outcome]}"
                )
        if sum(int(require_number(data, key, policy)) for key in outcome_keys.values()) != total:
            raise ValidationError(f"{path}: {policy} outcome counts do not sum to total_runs")
        for count_key, rate_key in RATE_COUNT_PAIRS:
            count = require_number(data, count_key, policy)
            rate = require_number(data, rate_key, policy)
            expected_rate = count / total
            if not math.isclose(rate, expected_rate, rel_tol=0.0, abs_tol=1e-12):
                raise ValidationError(
                    f"{path}: {policy}.{rate_key}={rate} expected {expected_rate} from {count_key}"
                )


def validate_findings(path: Path) -> int:
    fields, rows = load_csv(path)
    if fields != FINDING_FIELDS:
        raise ValidationError(f"{path}: columns must exactly match {FINDING_FIELDS}")
    if not rows:
        raise ValidationError(f"{path}: findings must not be empty")

    ids: set[str] = set()
    covered_questions: set[str] = set()
    blocker_count = 0
    for index, row in enumerate(rows, 1):
        label = f"finding row {index}"
        for field in FINDING_FIELDS:
            if not row.get(field, "").strip():
                raise ValidationError(f"{label}.{field} must not be blank")
        expected_id = f"P1B-{index:03d}"
        if row["finding_id"] != expected_id:
            raise ValidationError(f"{label}.finding_id expected {expected_id}, got {row['finding_id']}")
        if row["finding_id"] in ids:
            raise ValidationError(f"{path}: duplicate finding_id {row['finding_id']}")
        ids.add(row["finding_id"])
        if row["severity"] not in SEVERITIES:
            raise ValidationError(f"{label}.severity must be one of {sorted(SEVERITIES)}")
        if row["severity"] == "BLOCKER":
            blocker_count += 1
        for question in (part.strip() for part in row["question_id"].split(";")):
            if question:
                covered_questions.add(question)

    expected_questions = {f"Q{index}" for index in range(1, 11)}
    missing = expected_questions - covered_questions
    if missing:
        raise ValidationError(f"{path}: missing findings coverage for {sorted(missing)}")
    return blocker_count


def validate_all(manual: Path, runs: Path, summary: Path, findings: Path) -> int:
    validate_manual(manual)
    run_counts = validate_runs(runs)
    validate_summary(summary, run_counts)
    return validate_findings(findings)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        blocker_count = validate_all(args.manual, args.runs, args.summary, args.findings)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 1B results: OK")
    print("manual sessions: 10")
    print("policy runs: 2000 (2 policies x 1000 seeds)")
    print(f"BLOCKER findings: {blocker_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
