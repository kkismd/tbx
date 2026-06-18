from __future__ import annotations

import csv
import json
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

from experiments.galactic_exodus.srs import validate_phase2_initial_model as validator


class Phase2InitialModelValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(".tmp/phase2_initial_model_tests")
        self.root.mkdir(parents=True, exist_ok=True)
        self.model = self.root / "model.md"
        self.questions = self.root / "questions.csv"
        self.values = self.root / "values.json"
        self.write_valid_artifacts()

    def tearDown(self) -> None:
        for path in (self.model, self.questions, self.values):
            path.unlink(missing_ok=True)
        self.root.rmdir()

    def write_valid_artifacts(self) -> None:
        self.model.write_text(
            "SectorType SrsTerrainType SrsObjectType SrsActorType warp_flags blocked_edges "
            "generation_schema_version generation_profile_ref GRAVITY_FIELD_VERTICAL "
            "GRAVITY_FIELD_HORIZONTAL STATION STAR PLANET RESOURCE_CACHE SALVAGE 9x9 11x11 "
            "LOCAL_3X3 TURN_ONLY EXPLICIT_INTERACT VALUE_OBJECT_DETOUR KNOWN_DESCRIPTOR "
            "MOVEMENT_POINTS VECTOR_COMMAND DIRECTIONAL_THRUST STOP_BEFORE C1..C8 Q1..Q16 #1080\n",
            encoding="utf-8",
        )
        values = {
            "schema_version": 3,
            "generation_schema_version": 1,
            "contract_references": dict(validator.EXPECTED_CONTRACT_REFERENCES),
            "sector_types": sorted(validator.EXPECTED_SECTOR_TYPES),
            "directions": sorted(validator.EXPECTED_DIRECTIONS),
            "terrain_types": sorted(validator.EXPECTED_TERRAIN_TYPES),
            "object_types": sorted(validator.EXPECTED_OBJECT_TYPES),
            "actor_types": sorted(validator.EXPECTED_ACTOR_TYPES),
            "movement_rules": sorted(validator.EXPECTED_MOVEMENT_RULES),
            "path_input_modes": sorted(validator.EXPECTED_PATH_INPUT_MODES),
            "baseline": {
                "width": 9,
                "height": 9,
                "generation_profile": "phase2_srs_generation.json",
                "generation_schema_version": 1,
                "observation_mode": "LOCAL_3X3",
                "cost_mode": "TURN_ONLY",
                "interaction_mode": "EXPLICIT_INTERACT",
                "sector_value_route": "VALUE_OBJECT_DETOUR",
                "rift_knowledge_mode": "KNOWN_DESCRIPTOR",
                "movement_rule": "MOVEMENT_POINTS",
                "movement_points_per_turn": 4,
                "path_input_mode": "ROUTE_PREVIEW",
                "collision_behavior": "STOP_BEFORE",
                "max_srs_turns": 40,
            },
            "comparisons": {
                "C1": {"field": "map_size", "values": [[9, 9], [11, 11]]},
                "C2": {"field": "observation_mode", "values": ["FULL", "LOCAL_3X3"]},
                "C3": {"field": "cost_mode", "values": ["TURN_ONLY", "SHARED_FUEL"]},
                "C4": {"field": "interaction_mode", "values": ["AUTO_INTERACT", "EXPLICIT_INTERACT"]},
                "C5": {
                    "field": "sector_value_route",
                    "values": ["DIRECT_EXIT", "VALUE_OBJECT_DETOUR"],
                },
                "C6": {"field": "rift_knowledge_mode", "values": ["KNOWN_DESCRIPTOR", "LOCAL_DISCOVERY"]},
                "C7": {
                    "field": "sector_type",
                    "values": ["NORMAL", "BASE", "RESOURCE", "NEBULA", "ASTEROID", "GRAVITY", "RIFT"],
                },
                "C8": {
                    "field": "movement_rule",
                    "values": ["VECTOR_COMMAND", "MOVEMENT_POINTS", "DIRECTIONAL_THRUST"],
                },
            },
            "thresholds": {},
            "persistent_fields": sorted(validator.REQUIRED_PERSISTENT_FIELDS),
        }
        self.values.write_text(json.dumps(values), encoding="utf-8")

        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            for index in range(1, 21):
                writer.writerow(
                    {
                        "question_id": f"Q{index}",
                        "question": "question",
                        "hypothesis": "hypothesis",
                        "comparison_ids": "C8" if index >= 11 else "C1",
                        "automated_metrics": "metric",
                        "manual_scores": "score",
                        "required_sector_types": "NORMAL",
                        "required_fixtures": "fixture",
                        "decision_rule": "rule",
                    }
                )

    def test_validate_all_accepts_valid_artifacts(self) -> None:
        validator.validate_all(self.model, self.questions, self.values)

    def test_cli_reports_twenty_questions(self) -> None:
        stdout = StringIO()
        with redirect_stdout(stdout):
            exit_code = validator.main(
                ["--model", str(self.model), "--questions", str(self.questions), "--values", str(self.values)]
            )
        self.assertEqual(exit_code, 0)
        self.assertIn("questions: 20", stdout.getvalue())

    def test_missing_q20_is_rejected(self) -> None:
        rows = self.read_question_rows()
        self.write_question_rows(rows[:-1])
        with self.assertRaisesRegex(validator.ValidationError, "Q1..Q20 exactly once"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_extra_q21_is_rejected(self) -> None:
        rows = self.read_question_rows()
        extra = dict(rows[-1])
        extra["question_id"] = "Q21"
        rows.append(extra)
        self.write_question_rows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "Q1..Q20 exactly once"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_duplicate_question_id_is_rejected(self) -> None:
        rows = self.read_question_rows()
        rows[-1]["question_id"] = "Q19"
        self.write_question_rows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "Q1..Q20 exactly once"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_unknown_comparison_id_is_rejected(self) -> None:
        rows = self.read_question_rows()
        rows[0]["comparison_ids"] = "C9"
        self.write_question_rows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "unknown comparisons"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_unknown_sector_type_is_rejected(self) -> None:
        rows = self.read_question_rows()
        rows[0]["required_sector_types"] = "VOID"
        self.write_question_rows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "invalid required_sector_types"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_blank_field_is_rejected(self) -> None:
        rows = self.read_question_rows()
        rows[0]["decision_rule"] = ""
        self.write_question_rows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "must not be blank"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_q11_to_q16_must_include_c8(self) -> None:
        rows = self.read_question_rows()
        rows[10]["comparison_ids"] = "C1"
        self.write_question_rows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "Q11 must include C8"):
            validator.validate_questions(self.questions, validator.validate_values(self.values))

    def test_schema_version_must_be_three(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["schema_version"] = 2
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "schema_version must be 3"):
            validator.validate_values(self.values)

    def test_generation_schema_version_must_be_one(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["generation_schema_version"] = 2
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "generation_schema_version must be 1"):
            validator.validate_values(self.values)

    def test_feature_types_field_is_rejected(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["feature_types"] = ["WARP_POINT"]
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "feature_types must not exist"):
            validator.validate_values(self.values)

    def test_baseline_must_be_9x9(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["baseline"]["width"] = 11
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "baseline must be 9x9"):
            validator.validate_values(self.values)

    def test_c1_must_compare_9x9_and_11x11(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["comparisons"]["C1"]["values"] = [[9, 9], [13, 13]]
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "C1 must compare 9x9 and 11x11"):
            validator.validate_values(self.values)

    def test_c5_must_compare_sector_value_route(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["comparisons"]["C5"] = {"field": "object_profile", "values": ["A", "B"]}
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "C5 must compare sector_value_route"):
            validator.validate_values(self.values)

    def test_c7_must_include_all_sector_types(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["comparisons"]["C7"]["values"] = ["NORMAL", "BASE"]
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "C7 must compare all sector types"):
            validator.validate_values(self.values)

    def test_c8_must_compare_all_movement_rules(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["comparisons"]["C8"]["values"] = ["MOVEMENT_POINTS", "VECTOR_COMMAND"]
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "C8 must compare all movement rules"):
            validator.validate_values(self.values)

    def test_persistent_fields_must_include_schema_three_set(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["persistent_fields"].remove("warp_flags")
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "persistent_fields must be"):
            validator.validate_values(self.values)

    def read_question_rows(self) -> list[dict[str, str]]:
        with self.questions.open(encoding="utf-8", newline="") as file:
            return list(csv.DictReader(file))

    def write_question_rows(self, rows: list[dict[str, str]]) -> None:
        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)


if __name__ == "__main__":
    unittest.main()
