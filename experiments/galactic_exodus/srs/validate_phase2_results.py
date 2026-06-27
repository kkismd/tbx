#!/usr/bin/env python3
"""Validate Phase 2 SRS integrated evaluation artifacts."""

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

FINDING_FIELDS = [
    "finding_id",
    "question_id",
    "category",
    "candidate_classification",
    "summary",
    "evidence_type",
    "evidence_ref",
    "case_id",
    "policy",
    "cost_mode",
    "impact",
    "suggested_next_action",
    "notes",
]
CLASSIFICATIONS = {"BLOCKER", "ADJUSTMENT", "PHASE_LATER", "NO_CHANGE"}
QUESTION_IDS = {f"Q{index}" for index in range(1, 11)}
REQUIRED_PLAYTEST_SNIPPETS = (
    "## 2. 手動評価の要約",
    "## 3. 自動評価の要約",
    "policy 別の特徴",
    "TURN_ONLY / SHARED_FUEL",
    "RESOURCE_CACHE",
    "STATION",
    "SALVAGE placeholder",
    "NEBULA 3x3",
    "Q1.",
    "Q10.",
)


class ValidationError(ValueError):
    """Raised when a Phase 2 artifact violates its contract."""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--playtest", type=Path, required=True)
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


def validate_playtest(path: Path) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    for snippet in REQUIRED_PLAYTEST_SNIPPETS:
        if snippet not in text:
            raise ValidationError(f"{path}: missing required section text: {snippet}")


def _split_semicolon(raw: str) -> list[str]:
    return [part.strip() for part in raw.split(";") if part.strip()]


def validate_findings(path: Path) -> dict[str, int]:
    fields, rows = load_csv(path)
    if fields != FINDING_FIELDS:
        raise ValidationError(f"{path}: columns must exactly match {FINDING_FIELDS}")
    if len(rows) < 10:
        raise ValidationError(f"{path}: expected at least 10 rows, got {len(rows)}")

    seen_ids: set[str] = set()
    covered_questions: set[str] = set()
    counts = {classification: 0 for classification in CLASSIFICATIONS}
    for index, row in enumerate(rows, 1):
        label = f"finding row {index}"
        for field in FINDING_FIELDS:
            if not row.get(field, "").strip():
                raise ValidationError(f"{label}.{field} must not be blank")
        if row["finding_id"] in seen_ids:
            raise ValidationError(f"{path}: duplicate finding_id {row['finding_id']}")
        seen_ids.add(row["finding_id"])
        if not row["finding_id"].startswith("P2SRS-"):
            raise ValidationError(f"{label}.finding_id must start with P2SRS-")
        classification = row["candidate_classification"]
        if classification not in CLASSIFICATIONS:
            raise ValidationError(
                f"{label}.candidate_classification must be one of {sorted(CLASSIFICATIONS)}"
            )
        counts[classification] += 1
        questions = _split_semicolon(row["question_id"])
        if not questions:
            raise ValidationError(f"{label}.question_id must not be empty")
        for question_id in questions:
            if question_id not in QUESTION_IDS:
                raise ValidationError(f"{label}.question_id has unknown value {question_id}")
            covered_questions.add(question_id)

    missing = QUESTION_IDS - covered_questions
    if missing:
        raise ValidationError(f"{path}: missing question coverage for {sorted(missing)}")
    return counts


def validate_all(playtest: Path, findings: Path) -> dict[str, int]:
    validate_playtest(playtest)
    return validate_findings(findings)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        counts = validate_all(args.playtest, args.findings)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 2 SRS integrated results: OK")
    print(
        "classifications:",
        ", ".join(f"{key}={counts[key]}" for key in sorted(CLASSIFICATIONS)),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
