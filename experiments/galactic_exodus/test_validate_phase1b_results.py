from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.archive.evaluation.phase1_lrs import validate_phase1b_results as validator


class Phase1BValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.manual = self.root / "manual.csv"
        self.runs = self.root / "runs.csv"
        self.summary = self.root / "summary.json"
        self.findings = self.root / "findings.csv"
        self.write_valid_artifacts()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write_valid_artifacts(self) -> None:
        with self.manual.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=["requested_seed", "outcome", "notes"])
            writer.writeheader()
            for seed in range(1, 11):
                writer.writerow({"requested_seed": seed, "outcome": "WON", "notes": "ok"})

        with self.runs.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=["policy", "requested_seed", "outcome"])
            writer.writeheader()
            for policy in validator.EXPECTED_POLICIES:
                for seed in range(1, 1001):
                    writer.writerow({"policy": policy, "requested_seed": seed, "outcome": "WON"})

        policy_summary = {
            "total_runs": 1000,
            "win_count": 1000,
            "win_rate": 1.0,
            "lost_fuel_count": 0,
            "lost_fuel_rate": 0.0,
            "aborted_turn_limit_count": 0,
            "aborted_turn_limit_rate": 0.0,
            "aborted_no_policy_action_count": 0,
            "aborted_no_policy_action_rate": 0.0,
            "generation_error_count": 0,
            "generation_error_rate": 0.0,
            "base_visit_run_count": 0,
            "base_visit_rate": 0.0,
            "base_refuel_run_count": 0,
            "base_refuel_rate": 0.0,
            "multiple_base_refuel_run_count": 0,
            "multiple_base_refuel_rate": 0.0,
            "resource_visit_run_count": 0,
            "resource_visit_rate": 0.0,
            "resource_refuel_run_count": 0,
            "resource_refuel_rate": 0.0,
            "multiple_resource_refuel_run_count": 0,
            "multiple_resource_refuel_rate": 0.0,
            "no_supply_win_count": 0,
            "no_supply_win_rate": 0.0,
            "rift_attempt_run_count": 0,
            "rift_attempt_rate": 0.0,
            "reroll_occurred_count": 0,
            "reroll_rate": 0.0,
        }
        self.summary.write_text(
            json.dumps(
                {
                    "schema_version": 3,
                    "seed_start": 1,
                    "seed_end": 1000,
                    "policies": {
                        policy: dict(policy_summary) for policy in validator.EXPECTED_POLICIES
                    },
                }
            ),
            encoding="utf-8",
        )

        with self.findings.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.FINDING_FIELDS)
            writer.writeheader()
            for index in range(1, 11):
                writer.writerow(
                    {
                        "finding_id": f"P1B-{index:03d}",
                        "question_id": f"Q{index}",
                        "title": "title",
                        "evidence": "manual and automated evidence",
                        "severity": "NO_CHANGE",
                        "proposed_change": "retain",
                        "affected_issues": "#1059",
                        "recommended_disposition": "retain",
                    }
                )

    def test_validate_all_accepts_valid_artifacts(self) -> None:
        self.assertEqual(
            validator.validate_all(self.manual, self.runs, self.summary, self.findings),
            0,
        )

    def test_validate_manual_rejects_wrong_seed_order(self) -> None:
        rows = self.manual.read_text(encoding="utf-8").splitlines()
        rows[1] = "2,WON,ok"
        self.manual.write_text("\n".join(rows) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "requested_seed"):
            validator.validate_manual(self.manual)

    def test_validate_summary_rejects_count_mismatch(self) -> None:
        payload = json.loads(self.summary.read_text(encoding="utf-8"))
        payload["policies"]["GOAL_GREEDY"]["win_count"] = 999
        payload["policies"]["GOAL_GREEDY"]["win_rate"] = 0.999
        self.summary.write_text(json.dumps(payload), encoding="utf-8")
        run_counts = validator.validate_runs(self.runs)
        with self.assertRaisesRegex(validator.ValidationError, "does not match runs"):
            validator.validate_summary(self.summary, run_counts)

    def test_validate_findings_requires_all_questions(self) -> None:
        with self.findings.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        with self.findings.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.FINDING_FIELDS)
            writer.writeheader()
            writer.writerows(rows[:-1])
        with self.assertRaisesRegex(validator.ValidationError, "missing findings coverage"):
            validator.validate_findings(self.findings)


if __name__ == "__main__":
    unittest.main()
