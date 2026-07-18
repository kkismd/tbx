from __future__ import annotations

import csv
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.archive.evaluation.srs import validate_phase2_decisions as validator


class Phase2DecisionValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.decisions = self.root / "phase2_decisions.csv"
        self.write_valid_decisions()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write_valid_decisions(self) -> None:
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            for index, topic in enumerate(sorted(validator.REQUIRED_TOPICS), start=1):
                writer.writerow(
                    {
                        "decision_id": f"P2DEC-{index:03d}",
                        "topic": topic,
                        "status": "DECIDED",
                        "classification": "NO_CHANGE",
                        "summary": "summary",
                        "chosen_rule": "rule",
                        "reason": "reason",
                        "evidence_refs": "evidence",
                        "follow_up_issue": "-",
                        "notes": "notes",
                    }
                )

    def test_validate_decisions_accepts_valid_file(self) -> None:
        counts = validator.validate_decisions(self.decisions)
        self.assertEqual(counts["NO_CHANGE"], len(validator.REQUIRED_TOPICS))

    def test_validate_decisions_requires_follow_up_for_phase_later(self) -> None:
        with self.decisions.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows[0]["classification"] = "PHASE_LATER"
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "follow_up_issue"):
            validator.validate_decisions(self.decisions)

    def test_validate_decisions_requires_required_topics(self) -> None:
        with self.decisions.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows.pop()
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "missing required topics"):
            validator.validate_decisions(self.decisions)

    def test_validate_decisions_rejects_unknown_status(self) -> None:
        with self.decisions.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows[0]["status"] = "OPEN"
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "status must be DECIDED"):
            validator.validate_decisions(self.decisions)


if __name__ == "__main__":
    unittest.main()
