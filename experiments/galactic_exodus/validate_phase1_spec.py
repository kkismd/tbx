#!/usr/bin/env python3
"""Validate Galactic Exodus Phase 1 decisions, specification, and fixtures."""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
from pathlib import Path
from typing import Any

DECISION_FIELDS = [
    "decision_id",
    "question",
    "current_behavior",
    "decision",
    "evidence",
    "source_finding_ids",
    "affected_issues",
    "tbx_impact",
    "status",
    "deferred_issue",
]
VALID_STATUSES = {"DECIDED", "DEFERRED", "BLOCKED"}
EXPECTED_FINDINGS = {f"P1B-{index:03d}" for index in range(1, 13)}
REQUIRED_FIXTURES = {
    "no_reroll_initial_board",
    "reroll_requested_effective_seed",
    "normal_terrain_move",
    "unknown_rift_failure",
    "known_rift_retry",
    "base_supply",
    "resource_supply",
    "resource_second_visit_no_supply",
    "zero_fuel_goal_arrival_wins",
    "fuel_depletion_loss",
    "generation_error",
    "turn_limit_abort",
}
ISSUE_PATTERN = re.compile(r"^#\d+$")
DECISION_ID_PATTERN = re.compile(r"\b[A-Z]+-\d{3}\b")
POSITION_KEYS = {"x", "y"}


class ValidationError(ValueError):
    """Raised when Phase 1 artifacts violate their contract."""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--decisions", type=Path, required=True)
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    return parser.parse_args(argv)


def load_decisions(path: Path) -> list[dict[str, str]]:
    try:
        with path.open(encoding="utf-8", newline="") as file:
            reader = csv.DictReader(file)
            if reader.fieldnames != DECISION_FIELDS:
                raise ValidationError(f"{path}: columns must exactly match {DECISION_FIELDS}")
            rows = list(reader)
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    if not rows:
        raise ValidationError(f"{path}: decision register must not be empty")
    return rows


def split_semicolon(value: str) -> set[str]:
    return {item.strip() for item in value.split(";") if item.strip() and item.strip() != "-"}


def validate_decisions(path: Path) -> set[str]:
    rows = load_decisions(path)
    decision_ids: set[str] = set()
    findings: set[str] = set()
    blocked: list[str] = []

    for index, row in enumerate(rows, start=1):
        label = f"decision row {index}"
        for field in DECISION_FIELDS:
            if not row.get(field, "").strip():
                raise ValidationError(f"{label}.{field} must not be blank")
        decision_id = row["decision_id"]
        if decision_id in decision_ids:
            raise ValidationError(f"{path}: duplicate decision_id {decision_id}")
        decision_ids.add(decision_id)

        status = row["status"]
        if status not in VALID_STATUSES:
            raise ValidationError(f"{label}.status must be one of {sorted(VALID_STATUSES)}")
        if status == "BLOCKED":
            blocked.append(decision_id)
        deferred_issue = row["deferred_issue"].strip()
        if status == "DEFERRED":
            if not ISSUE_PATTERN.fullmatch(deferred_issue):
                raise ValidationError(f"{label}.deferred_issue must be a GitHub issue number")
        elif deferred_issue != "-":
            raise ValidationError(f"{label}.deferred_issue must be '-' unless status is DEFERRED")

        for finding_id in split_semicolon(row["source_finding_ids"]):
            if finding_id not in EXPECTED_FINDINGS:
                raise ValidationError(f"{label}: unknown finding ID {finding_id}")
            findings.add(finding_id)

    if blocked:
        raise ValidationError(f"{path}: BLOCKED decisions remain: {blocked}")
    missing = EXPECTED_FINDINGS - findings
    if missing:
        raise ValidationError(f"{path}: findings not processed: {sorted(missing)}")
    return decision_ids


def require_object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValidationError(f"{label} must be an object")
    return value


def validate_position(value: Any, label: str) -> None:
    position = require_object(value, label)
    if set(position) != POSITION_KEYS:
        raise ValidationError(f"{label} must have exactly x and y")
    for key in POSITION_KEYS:
        coordinate = position[key]
        if isinstance(coordinate, bool) or not isinstance(coordinate, int):
            raise ValidationError(f"{label}.{key} must be an integer")
        if not 1 <= coordinate <= 8:
            raise ValidationError(f"{label}.{key} must be in 1..8")


def position_key(value: dict[str, Any]) -> tuple[int, int]:
    return int(value["x"]), int(value["y"])


def validate_injected_map(value: Any, label: str) -> None:
    actual_map = require_object(value, label)
    expected_keys = {"cells", "rift_edges", "base_position", "resource_positions"}
    if set(actual_map) != expected_keys:
        raise ValidationError(f"{label}: keys must be {sorted(expected_keys)}")

    cells = actual_map["cells"]
    if not isinstance(cells, list):
        raise ValidationError(f"{label}.cells must be an array")
    seen_positions: set[tuple[int, int]] = set()
    for index, cell in enumerate(cells):
        item = require_object(cell, f"{label}.cells[{index}]")
        validate_position(item.get("position"), f"{label}.cells[{index}].position")
        symbol = item.get("symbol")
        if not isinstance(symbol, str) or len(symbol) != 1:
            raise ValidationError(f"{label}.cells[{index}].symbol must be one character")
        key = position_key(item["position"])
        if key in seen_positions:
            raise ValidationError(f"{label}: duplicate cell position {key}")
        seen_positions.add(key)
    if len(seen_positions) != 64:
        raise ValidationError(f"{label}.cells must contain exactly 64 positions")

    edges = actual_map["rift_edges"]
    if not isinstance(edges, list):
        raise ValidationError(f"{label}.rift_edges must be an array")
    normalized_edges: set[tuple[tuple[int, int], tuple[int, int]]] = set()
    for index, edge in enumerate(edges):
        if not isinstance(edge, list) or len(edge) != 2:
            raise ValidationError(f"{label}.rift_edges[{index}] must contain two positions")
        validate_position(edge[0], f"{label}.rift_edges[{index}][0]")
        validate_position(edge[1], f"{label}.rift_edges[{index}][1]")
        first = position_key(edge[0])
        second = position_key(edge[1])
        if first >= second:
            raise ValidationError(f"{label}.rift_edges[{index}] must be lexicographically sorted")
        if abs(first[0] - second[0]) + abs(first[1] - second[1]) != 1:
            raise ValidationError(f"{label}.rift_edges[{index}] positions must be adjacent")
        normalized = (first, second)
        if normalized in normalized_edges:
            raise ValidationError(f"{label}: duplicate rift edge {normalized}")
        normalized_edges.add(normalized)

    validate_position(actual_map["base_position"], f"{label}.base_position")
    resources = actual_map["resource_positions"]
    if not isinstance(resources, list):
        raise ValidationError(f"{label}.resource_positions must be an array")
    resource_keys: set[tuple[int, int]] = set()
    for index, position in enumerate(resources):
        validate_position(position, f"{label}.resource_positions[{index}]")
        key = position_key(position)
        if key in resource_keys:
            raise ValidationError(f"{label}: duplicate resource position {key}")
        resource_keys.add(key)


def validate_fixtures(path: Path) -> None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ValidationError(f"invalid JSON in {path}: {exc}") from exc
    root = require_object(payload, str(path))
    if root.get("schema_version") != 1:
        raise ValidationError(f"{path}: fixture schema_version must be 1")
    if root.get("game_log_schema_version") != 3:
        raise ValidationError(f"{path}: game_log_schema_version must be 3")
    fixtures = root.get("fixtures")
    if not isinstance(fixtures, list):
        raise ValidationError(f"{path}.fixtures must be an array")

    names: set[str] = set()
    for index, fixture_value in enumerate(fixtures):
        fixture = require_object(fixture_value, f"fixture[{index}]")
        required = {
            "name", "purpose", "mode", "settings", "requested_seed", "effective_seed",
            "reroll_count", "initial_actual_map", "commands", "expected_initial",
            "expected_turns", "expected_final", "generation_stub", "max_turns",
        }
        missing = required - set(fixture)
        if missing:
            raise ValidationError(f"fixture[{index}] missing keys {sorted(missing)}")
        name = fixture["name"]
        if not isinstance(name, str) or not name:
            raise ValidationError(f"fixture[{index}].name must be non-empty")
        if name in names:
            raise ValidationError(f"{path}: duplicate fixture name {name}")
        names.add(name)
        if fixture["mode"] not in {"generated", "injected", "generation_error"}:
            raise ValidationError(f"fixture[{index}].mode is invalid")
        settings = require_object(fixture["settings"], f"fixture[{index}].settings")
        expected_setting_keys = {
            "width",
            "height",
            "start_position",
            "goal_position",
            "rift_density",
            "initial_fuel",
            "max_fuel",
            "resource_count",
            "resource_supply",
        }
        if set(settings) != expected_setting_keys:
            raise ValidationError(f"fixture[{index}].settings keys must be {sorted(expected_setting_keys)}")
        for position_key_name in ("start_position", "goal_position"):
            validate_position(settings.get(position_key_name), f"fixture[{index}].settings.{position_key_name}")
        if isinstance(fixture["max_turns"], bool) or not isinstance(fixture["max_turns"], int) or fixture["max_turns"] < 0:
            raise ValidationError(f"fixture[{index}].max_turns must be a non-negative integer")
        if not isinstance(fixture["commands"], list) or not all(isinstance(command, str) for command in fixture["commands"]):
            raise ValidationError(f"fixture[{index}].commands must be an array of strings")
        if not isinstance(fixture["expected_turns"], list):
            raise ValidationError(f"fixture[{index}].expected_turns must be an array")
        if fixture["expected_initial"] is not None:
            require_object(fixture["expected_initial"], f"fixture[{index}].expected_initial")
        require_object(fixture["expected_final"], f"fixture[{index}].expected_final")
        if fixture["mode"] in {"generated", "injected"}:
            validate_injected_map(fixture["initial_actual_map"], f"fixture[{index}].initial_actual_map")
            if fixture["generation_stub"] is not None:
                raise ValidationError(f"fixture[{index}].generation_stub must be null outside generation_error mode")
        else:
            if fixture["initial_actual_map"] is not None:
                raise ValidationError(f"fixture[{index}].initial_actual_map must be null in generation_error mode")
            generation_stub = require_object(fixture["generation_stub"], f"fixture[{index}].generation_stub")
            if set(generation_stub) != {"reachable_sequence"}:
                raise ValidationError(f"fixture[{index}].generation_stub keys must be ['reachable_sequence']")
            reachable_sequence = generation_stub["reachable_sequence"]
            if (
                not isinstance(reachable_sequence, list)
                or not reachable_sequence
                or not all(isinstance(item, bool) for item in reachable_sequence)
            ):
                raise ValidationError(
                    f"fixture[{index}].generation_stub.reachable_sequence must be a non-empty array of booleans"
                )

    missing_names = REQUIRED_FIXTURES - names
    if missing_names:
        raise ValidationError(f"{path}: required fixtures missing: {sorted(missing_names)}")


def validate_spec(path: Path, decision_ids: set[str]) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    if "TBD" in text:
        raise ValidationError(f"{path}: TBD must not remain")
    referenced = set(DECISION_ID_PATTERN.findall(text))
    unknown = referenced - decision_ids
    if unknown:
        raise ValidationError(f"{path}: unknown decision IDs referenced: {sorted(unknown)}")
    required_sections = [
        "盤面と既知情報", "移動", "燃料と補給", "勝敗とabort", "再抽選とseed",
        "Phase 1 UI契約", "GameLog schema v3", "Python/TBX一致契約", "reference fixture",
    ]
    for section in required_sections:
        if section not in text:
            raise ValidationError(f"{path}: missing section {section}")


def validate_all(decisions: Path, spec: Path, fixtures: Path) -> None:
    decision_ids = validate_decisions(decisions)
    validate_spec(spec, decision_ids)
    validate_fixtures(fixtures)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        validate_all(args.decisions, args.spec, args.fixtures)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 1 specification: OK")
    print(f"decisions: {len(load_decisions(args.decisions))}")
    print(f"findings processed: {len(EXPECTED_FINDINGS)}")
    print(f"required fixtures: {len(REQUIRED_FIXTURES)}")
    print("BLOCKED decisions: 0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
