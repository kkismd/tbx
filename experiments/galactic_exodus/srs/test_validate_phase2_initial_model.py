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
            for index in range(1, 17):
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


if __name__ == "__main__":
    unittest.main()
