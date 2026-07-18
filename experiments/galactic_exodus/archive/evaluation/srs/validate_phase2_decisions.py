#!/usr/bin/env python3
"""Validate Phase 2 SRS decision log artifacts."""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

DECISION_FIELDS = [
    "decision_id",
    "topic",
    "status",
    "classification",
    "summary",
    "chosen_rule",
    "reason",
    "evidence_refs",
    "follow_up_issue",
    "notes",
]
VALID_STATUS = "DECIDED"
CLASSIFICATIONS = {"DECIDED", "ADJUSTMENT", "PHASE_LATER", "NO_CHANGE"}
REQUIRED_TOPICS = {
    "SRS盤面サイズ",
    "entry / exit mapping",
    "blocked edge mapping",
    "観測方式",
    "通常5x5 / NEBULA3x3",
    "1区画の想定turn",
    "B/Rへ内部到達する価値",
    "通常区画の探索価値",
    "再訪時に保持する状態",
    "SRS移動とLRS fuel/turnの関係",
    "TURN_ONLY / SHARED_FUELの採否",
    "RESOURCE_CACHE lifecycle",
    "STATION lifecycle",
    "SALVAGE placeholder lifecycle",
    "SRSを必須にする範囲と省略条件",
    "LRS/SRS切替条件",
    "GameLogとTBX state契約",
    "SRS turnを後続threat/encounterへ接続するか",
}
ISSUE_PATTERN = re.compile(r"^#\d+$")


class ValidationError(ValueError):
    """Raised when the decision log violates its contract."""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--decisions", type=Path, required=True)
    return parser.parse_args(argv)


def load_csv(path: Path) -> list[dict[str, str]]:
    try:
        with path.open(encoding="utf-8", newline="") as file:
            reader = csv.DictReader(file)
            if reader.fieldnames != DECISION_FIELDS:
                raise ValidationError(f"{path}: columns must exactly match {DECISION_FIELDS}")
            return list(reader)
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc


def validate_decisions(path: Path) -> dict[str, int]:
    rows = load_csv(path)
    if not rows:
        raise ValidationError(f"{path}: decision log must not be empty")

    seen_ids: set[str] = set()
    seen_topics: set[str] = set()
    counts = {classification: 0 for classification in CLASSIFICATIONS}
    for index, row in enumerate(rows, start=1):
        label = f"decision row {index}"
        for field in DECISION_FIELDS:
            if not row.get(field, "").strip():
                raise ValidationError(f"{label}.{field} must not be blank")

        decision_id = row["decision_id"]
        if decision_id in seen_ids:
            raise ValidationError(f"{path}: duplicate decision_id {decision_id}")
        seen_ids.add(decision_id)
        if not decision_id.startswith("P2DEC-"):
            raise ValidationError(f"{label}.decision_id must start with P2DEC-")

        topic = row["topic"]
        if topic in seen_topics:
            raise ValidationError(f"{path}: duplicate topic {topic}")
        seen_topics.add(topic)

        if row["status"] != VALID_STATUS:
            raise ValidationError(f"{label}.status must be {VALID_STATUS}")

        classification = row["classification"]
        if classification not in CLASSIFICATIONS:
            raise ValidationError(
                f"{label}.classification must be one of {sorted(CLASSIFICATIONS)}"
            )
        counts[classification] += 1

        follow_up_issue = row["follow_up_issue"].strip()
        if classification == "PHASE_LATER":
            if not ISSUE_PATTERN.fullmatch(follow_up_issue):
                raise ValidationError(
                    f"{label}.follow_up_issue must be a GitHub issue number for PHASE_LATER"
                )
        elif follow_up_issue != "-" and not ISSUE_PATTERN.fullmatch(follow_up_issue):
            raise ValidationError(
                f"{label}.follow_up_issue must be '-' or a GitHub issue number"
            )

    missing_topics = REQUIRED_TOPICS - seen_topics
    if missing_topics:
        raise ValidationError(f"{path}: missing required topics {sorted(missing_topics)}")
    return counts


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        counts = validate_decisions(args.decisions)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print("Phase 2 SRS decisions: OK")
    print(
        "classifications:",
        ", ".join(f"{key}={counts[key]}" for key in sorted(CLASSIFICATIONS)),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
