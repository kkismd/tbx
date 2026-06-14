from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs import validate_phase2_initial_model as validator


class Phase2InitialModelValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.model = self.root / "model.md"
        self.questions = self.root / "questions.csv"
        self.values = self.root / "values.json"
        self.write_valid_artifacts()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write_valid_artifacts(self) -> None:
        self.model.write_text(
            "RIFT blocked_edges 7x7 9x9 LOCAL_3X3 TURN_ONLY SHARED_FUEL "
            "AUTO_INTERACT EXPLICIT_INTERACT PROFILE_MINIMAL PROFILE_EXPLORATION "
            "C1 C7 #1080\n",
            encoding="utf-8",
        )
        values = {
            "schema_version": 1,
            "sector_types": ["NORMAL", "BASE", "RESOURCE", "RIFT"],
            "directions": ["N", "E", "S", "W"],
            "object_types": ["BASE_NODE", "RESOURCE_CACHE", "SALVAGE"],
            "invariants": {
                "rift_blocked_edge_min": 1,
                "rift_blocked_edge_max": 3,
                "non_rift_blocked_edges": [],
                "entry_must_be_open": True,
                "selected_exit_must_be_open": True,
                "odd_map_dimensions_only": True,
            },
            "baseline": {
                "width": 9,
                "height": 9,
                "entry_width": 1,
                "obstacle_density": 0.2,
                "observation_mode": "LOCAL_3X3",
                "cost_mode": "TURN_ONLY",
                "interaction_mode": "EXPLICIT_INTERACT",
                "object_profile": "PROFILE_EXPLORATION",
                "rift_knowledge_mode": "KNOWN_DESCRIPTOR",
                "max_srs_turns": 40,
            },
            "comparisons": {
                f"C{index}": {"field": f"field_{index}", "values": [0, 1]}
                for index in range(1, 8)
            },
            "object_profiles": {
                profile: {sector: [] for sector in validator.EXPECTED_SECTOR_TYPES}
                for profile in ("PROFILE_MINIMAL", "PROFILE_EXPLORATION")
            },
            "thresholds": {},
            "persistent_fields": sorted(validator.REQUIRED_PERSISTENT_FIELDS),
        }
        self.values.write_text(json.dumps(values), encoding="utf-8")

        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            for index in range(1, 11):
                writer.writerow(
                    {
                        "question_id": f"Q{index}",
                        "question": "question",
                        "hypothesis": "hypothesis",
                        "comparison_ids": "C1",
                        "automated_metrics": "metric",
                        "manual_scores": "score",
                        "required_sector_types": "NORMAL",
                        "required_fixtures": "fixture",
                        "decision_rule": "rule",
                    }
                )

    def test_validate_all_accepts_valid_artifacts(self) -> None:
        validator.validate_all(self.model, self.questions, self.values)

    def test_even_baseline_dimension_is_rejected(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["baseline"]["width"] = 8
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "odd integer"):
            validator.validate_values(self.values)

    def test_non_rift_blocked_edges_must_be_empty(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["invariants"]["non_rift_blocked_edges"] = ["N"]
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "non-RIFT"):
            validator.validate_values(self.values)

    def test_missing_question_is_rejected(self) -> None:
        rows = list(csv.DictReader(self.questions.open(encoding="utf-8", newline="")))
        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            writer.writerows(rows[:-1])
        values = validator.validate_values(self.values)
        with self.assertRaisesRegex(validator.ValidationError, "Q1..Q10"):
            validator.validate_questions(self.questions, values)

    def test_unknown_comparison_is_rejected(self) -> None:
        rows = list(csv.DictReader(self.questions.open(encoding="utf-8", newline="")))
        rows[0]["comparison_ids"] = "C99"
        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        values = validator.validate_values(self.values)
        with self.assertRaisesRegex(validator.ValidationError, "unknown comparisons"):
            validator.validate_questions(self.questions, values)

    def test_tbd_is_rejected(self) -> None:
        self.model.write_text(self.model.read_text(encoding="utf-8") + "TBD\n", encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "TBD"):
            validator.validate_model(self.model)


if __name__ == "__main__":
    unittest.main()
