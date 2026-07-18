from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.archive.evaluation.srs import validate_phase2_srs_generation as validator


class Phase2SrsGenerationValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.repo_root = Path(__file__).resolve().parents[3]
        self.tmp_root = self.repo_root / ".tmp"
        self.tmp_root.mkdir(exist_ok=True)
        self.tempdir = tempfile.TemporaryDirectory(dir=self.tmp_root)
        self.path = Path(self.tempdir.name) / "generation.json"
        source = Path(__file__).with_name("phase2_srs_generation.json")
        self.payload = json.loads(source.read_text(encoding="utf-8"))
        self.write()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write(self) -> None:
        self.path.write_text(json.dumps(self.payload), encoding="utf-8")

    def test_valid_contract_is_accepted(self) -> None:
        validator.validate(self.path)

    def test_7x7_is_rejected(self) -> None:
        self.payload["map_sizes"] = [[7, 7], [9, 9]]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "9x9 and 11x11"):
            validator.validate(self.path)

    def test_legacy_name_is_rejected(self) -> None:
        self.payload["terrain_types"].append("WALL")
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "forbidden term WALL"):
            validator.validate(self.path)

    def test_required_terrain_min_zero_is_rejected(self) -> None:
        self.payload["sector_profiles"]["RESOURCE"]["terrain_count_ranges"]["9x9"]["DEBRIS"]["min"] = 0
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "required terrain DEBRIS min must be 1 or more"):
            validator.validate(self.path)

    def test_inverted_range_is_rejected(self) -> None:
        self.payload["sector_profiles"]["NORMAL"]["terrain_count_ranges"]["9x9"]["DEBRIS"] = {
            "min": 3,
            "max": 1,
        }
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "min <= max"):
            validator.validate(self.path)

    def test_special_terrain_limit_violation_is_rejected(self) -> None:
        self.payload["sector_profiles"]["RESOURCE"]["special_terrain_limit"]["9x9"]["max"] = 17
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "special_terrain_limit contract mismatch"):
            validator.validate(self.path)

    def test_planet_count_mismatch_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["celestial_objects"]["PLANET"]["9x9"] = {
            "min": 1,
            "max": 4,
        }
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "PLANET range contract mismatch"):
            validator.validate(self.path)

    def test_warp_distance_constraint_missing_is_rejected(self) -> None:
        del self.payload["constraint_definitions"]["CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2"]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "constraint_definitions"):
            validator.validate(self.path)

    def test_constraint_chebyshev_distance_mismatch_is_rejected(self) -> None:
        self.payload["constraint_definitions"]["CELESTIAL_PAIR_MIN_CHEBYSHEV_2"]["min_distance"] = 1
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "constraint_definitions"):
            validator.validate(self.path)

    def test_station_floor_reservation_radius_mismatch_is_rejected(self) -> None:
        self.payload["constraint_definitions"]["STATION_NEIGHBORHOOD_RESERVED_FLOOR"]["radius"] = 0
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "constraint_definitions"):
            validator.validate(self.path)

    def test_resource_constraint_operator_mismatch_is_rejected(self) -> None:
        self.payload["constraint_definitions"]["RESOURCE_FIELD_IMPASSABLE_BALANCE"]["operator"] = ">"
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "constraint_definitions"):
            validator.validate(self.path)

    def test_asteroid_constraint_divisor_mismatch_is_rejected(self) -> None:
        self.payload["constraint_definitions"]["ASTEROID_CLUSTER_IMPASSABLE_BALANCE"]["right"][
            "divisor"
        ] = 999
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "constraint_definitions"):
            validator.validate(self.path)

    def test_gravity_constraint_min_mismatch_is_rejected(self) -> None:
        self.payload["constraint_definitions"]["GRAVITY_TOTAL_MIN_1"]["min"] = 0
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "constraint_definitions"):
            validator.validate(self.path)

    def test_missing_required_placement_constraint_is_rejected(self) -> None:
        self.payload["sector_profiles"]["RESOURCE"]["placement_constraints"].pop()
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "placement_constraints contract mismatch"):
            validator.validate(self.path)

    def test_warp_point_is_rejected(self) -> None:
        self.payload["object_types"].append("WARP_POINT")
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "forbidden term WARP_POINT"):
            validator.validate(self.path)

    def test_blocked_edge_warp_permission_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["warp"]["flag_generation"]["rift_blocked_edge"] = (
            "ALLOW_AND_REQUIRE"
        )
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "blocked-edge / outer-edge contract mismatch"):
            validator.validate(self.path)

    def test_arrival_tie_break_missing_is_rejected(self) -> None:
        del self.payload["global_generation_contract"]["warp"]["arrival_selection"]["north_south"][
            "tie_break"
        ]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "north/south arrival tie-break"):
            validator.validate(self.path)

    def test_retry_count_not_64_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["retry"]["attempt_count_max"] = 63
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "64-attempt window"):
            validator.validate(self.path)

    def test_seed_encoding_serialization_mismatch_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["seed_encoding"][
            "serialization"
        ] = "PLAIN_STRING"
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "seed_encoding contract mismatch"):
            validator.validate(self.path)

    def test_seed_digest_byte_order_mismatch_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["seed_encoding"][
            "digest_to_integer"
        ]["byte_order"] = "LITTLE_ENDIAN"
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "seed_encoding contract mismatch"):
            validator.validate(self.path)

    def test_attempt_seed_payload_mismatch_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["attempt_seed"][
            "payload_fields"
        ] = ["retry_index"]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "attempt_seed contract mismatch"):
            validator.validate(self.path)

    def test_derived_seed_payload_mismatch_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["derived_seed_encoding"][
            "payload_fields"
        ] = ["phase_label"]
        self.write()
        with self.assertRaisesRegex(
            validator.ValidationError, "derived_seed_encoding contract mismatch"
        ):
            validator.validate(self.path)

    def test_retry_seed_source_mismatch_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["retry"][
            "seed_source"
        ] = "SOMETHING_ELSE"
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "retry.seed_source contract mismatch"):
            validator.validate(self.path)

    def test_fallback_enabled_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["seed_and_retry"]["retry"]["fallback_map"] = True
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "fallback maps must be disabled"):
            validator.validate(self.path)

    def test_movement_rule_reference_missing_is_rejected(self) -> None:
        del self.payload["global_generation_contract"]["reachability"]["movement_rule_reference"]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "movement_rule_reference"):
            validator.validate(self.path)

    def test_generation_report_required_field_missing_is_rejected(self) -> None:
        self.payload["global_generation_contract"]["generation_report_schema"]["required_fields"].remove(
            "validation_results"
        )
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "generation report required_fields"):
            validator.validate(self.path)

    def test_cli_returns_nonzero_and_error_for_invalid_contract(self) -> None:
        self.payload["generation_schema_version"] = 2
        self.write()
        result = subprocess.run(
            [
                "python",
                "experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_generation.py",
                str(self.path),
            ],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("error:", result.stderr)


if __name__ == "__main__":
    unittest.main()
