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
            "NEBULA ASTEROID GRAVITY RIFT blocked_edges SrsTerrainType SrsFeatureType "
            "SrsObjectType SrsActorType WARP_POINT STAR PLANET STOP_BEFORE VECTOR_COMMAND "
            "MOVEMENT_POINTS DIRECTIONAL_THRUST 7x7 9x9 LOCAL_3X3 TURN_ONLY SHARED_FUEL "
            "AUTO_INTERACT EXPLICIT_INTERACT PROFILE_MINIMAL PROFILE_EXPLORATION "
            "C1 C8 Q1..Q16 #1080\n",
            encoding="utf-8",
        )
        values = {
            "schema_version": 2,
            "sector_types": sorted(validator.EXPECTED_SECTOR_TYPES),
            "directions": sorted(validator.EXPECTED_DIRECTIONS),
            "terrain_types": sorted(validator.EXPECTED_TERRAIN_TYPES),
            "feature_types": sorted(validator.EXPECTED_FEATURE_TYPES),
            "object_types": sorted(validator.EXPECTED_OBJECT_TYPES),
            "actor_types": sorted(validator.EXPECTED_ACTOR_TYPES),
            "movement_rules": sorted(validator.EXPECTED_MOVEMENT_RULES),
            "path_input_modes": sorted(validator.EXPECTED_PATH_INPUT_MODES),
            "invariants": {
                "rift_blocked_edge_min": 1,
                "rift_blocked_edge_max": 3,
                "non_rift_blocked_edges": [],
                "warp_point_must_be_open": True,
                "selected_warp_direction_must_be_open": True,
                "odd_map_dimensions_only": True,
                "star_count": 1,
                "planet_count_min": 1,
                "warp_point_at_edge_midpoint": True,
                "warp_point_object_overlap_allowed": False,
                "all_warp_points_connected": True,
            },
            "baseline": {
                "width": 9,
                "height": 9,
                "warp_point_width": 1,
                "warp_clearance_depth": 1,
                "obstacle_density": 0.2,
                "observation_mode": "LOCAL_3X3",
                "cost_mode": "TURN_ONLY",
                "interaction_mode": "EXPLICIT_INTERACT",
                "object_profile": "PROFILE_EXPLORATION",
                "rift_knowledge_mode": "KNOWN_DESCRIPTOR",
                "movement_rule": "MOVEMENT_POINTS",
                "movement_points_per_turn": 4,
                "path_input_mode": "ROUTE_PREVIEW",
                "collision_behavior": "STOP_BEFORE",
                "max_srs_turns": 40,
            },
            "celestial_body_profiles": {
                "7x7": {
                    "STAR": {"count": 1},
                    "PLANET": {"count_min": 1, "count_max": 3},
                },
                "9x9": {
                    "STAR": {"count": 1},
                    "PLANET": {"count_min": 2, "count_max": 5},
                },
            },
            "element_definitions": {
                object_type: {
                    "category": "CELESTIAL_BODY",
                    "passable": False,
                    "blocks_line_travel": True,
                    "persistent_after_revisit": True,
                    "collision_behavior": "STOP_BEFORE",
                    "allowed_sector_types": sorted(validator.EXPECTED_SECTOR_TYPES),
                }
                for object_type in ("STAR", "PLANET")
            }
            | {
                "WARP_POINT": {
                    "passable": True,
                    "placement": "EDGE_MIDPOINT",
                    "can_host_object": False,
                }
            },
            "terrain_profiles": {
                sector: ["FLOOR"] for sector in validator.EXPECTED_SECTOR_TYPES
            },
            "comparisons": {
                **{
                    f"C{index}": {"field": f"field_{index}", "values": [0, 1]}
                    for index in range(1, 8)
                },
                "C8": {
                    "field": "movement_rule",
                    "values": sorted(validator.EXPECTED_MOVEMENT_RULES),
                },
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

    def test_star_count_must_be_one(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["celestial_body_profiles"]["9x9"]["STAR"]["count"] = 2
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "STAR count"):
            validator.validate_values(self.values)

    def test_planet_must_be_impassable(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["element_definitions"]["PLANET"]["passable"] = True
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "PLANET must be impassable"):
            validator.validate_values(self.values)

    def test_warp_point_cannot_host_object(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["element_definitions"]["WARP_POINT"]["can_host_object"] = True
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "must not host objects"):
            validator.validate_values(self.values)

    def test_c8_must_compare_all_movement_rules(self) -> None:
        payload = json.loads(self.values.read_text(encoding="utf-8"))
        payload["comparisons"]["C8"]["values"] = ["MOVEMENT_POINTS", "VECTOR_COMMAND"]
        self.values.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "all movement rules"):
            validator.validate_values(self.values)

    def test_missing_question_is_rejected(self) -> None:
        rows = list(csv.DictReader(self.questions.open(encoding="utf-8", newline="")))
        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            writer.writerows(rows[:-1])
        values = validator.validate_values(self.values)
        with self.assertRaisesRegex(validator.ValidationError, "Q1..Q16"):
            validator.validate_questions(self.questions, values)

    def test_movement_question_requires_c8(self) -> None:
        rows = list(csv.DictReader(self.questions.open(encoding="utf-8", newline="")))
        rows[10]["comparison_ids"] = "C1"
        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        values = validator.validate_values(self.values)
        with self.assertRaisesRegex(validator.ValidationError, "Q11 must include C8"):
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
