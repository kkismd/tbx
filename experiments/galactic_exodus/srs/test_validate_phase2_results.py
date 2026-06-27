from __future__ import annotations

import csv
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs import validate_phase2_results as validator


class Phase2ResultsValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.playtest = self.root / "phase2_playtest.md"
        self.findings = self.root / "phase2_findings.csv"
        self.write_valid_artifacts()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write_valid_artifacts(self) -> None:
        self.playtest.write_text(
            "\n".join(
                [
                    "# title",
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
                ]
            )
            + "\n",
            encoding="utf-8",
        )
        with self.findings.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.FINDING_FIELDS)
            writer.writeheader()
            for index in range(1, 11):
                writer.writerow(
                    {
                        "finding_id": f"P2SRS-{index:03d}",
                        "question_id": f"Q{index}",
                        "category": "CATEGORY",
                        "candidate_classification": "NO_CHANGE",
                        "summary": "summary",
                        "evidence_type": "manual+automated",
                        "evidence_ref": "ref",
                        "case_id": "case",
                        "policy": "policy",
                        "cost_mode": "TURN_ONLY",
                        "impact": "impact",
                        "suggested_next_action": "next",
                        "notes": "notes",
                    }
                )

    def test_validate_all_accepts_valid_artifacts(self) -> None:
        counts = validator.validate_all(self.playtest, self.findings)
        self.assertEqual(counts["NO_CHANGE"], 10)

    def test_validate_playtest_requires_expected_sections(self) -> None:
        self.playtest.write_text("# title\n", encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "missing required section text"):
            validator.validate_playtest(self.playtest)

    def test_validate_findings_requires_question_coverage(self) -> None:
        with self.findings.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows[-1]["question_id"] = "Q9"
        with self.findings.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.FINDING_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "missing question coverage"):
            validator.validate_findings(self.findings)

    def test_validate_findings_rejects_unknown_classification(self) -> None:
        with self.findings.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows[0]["candidate_classification"] = "UNKNOWN"
        with self.findings.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.FINDING_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "candidate_classification"):
            validator.validate_findings(self.findings)


if __name__ == "__main__":
    unittest.main()
