from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs import validate_phase2_srs_movement as validator


class Phase2SrsMovementValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.repo_root = Path(__file__).resolve().parents[3]
        self.tmp_root = self.repo_root / ".tmp"
        self.tmp_root.mkdir(exist_ok=True)
        self.tempdir = tempfile.TemporaryDirectory(dir=self.tmp_root)
        self.path = Path(self.tempdir.name) / "movement.json"
        source = Path(__file__).with_name("phase2_srs_movement.json")
        self.payload = json.loads(source.read_text(encoding="utf-8"))
        self.write()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write(self) -> None:
        self.path.write_text(json.dumps(self.payload, ensure_ascii=False), encoding="utf-8")

    def assert_invalid(self, pattern: str) -> None:
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, pattern):
            validator.validate(self.path)

    def test_valid_contract_is_accepted(self) -> None:
        validator.validate(self.path)

    def test_schema_version_mismatch_is_rejected(self) -> None:
        self.payload["movement_schema_version"] = 2
        self.assert_invalid("movement_schema_version must be 1")

    def test_legacy_wall_is_rejected(self) -> None:
        self.payload["movement_rules"]["MOVEMENT_POINTS"]["legacy"] = "WALL"
        self.assert_invalid("forbidden term WALL")

    def test_legacy_local_3x3_is_rejected(self) -> None:
        self.payload["baseline"]["observation_mode"] = "LOCAL_3X3"
        self.assert_invalid("forbidden term LOCAL_3X3")

    def test_baseline_cost_mode_mismatch_is_rejected(self) -> None:
        self.payload["baseline"]["cost_mode"] = "SHARED_FUEL"
        self.assert_invalid("baseline.cost_mode")

    def test_turn_only_fuel_consumption_is_rejected(self) -> None:
        self.payload["cost_units"]["turn_only_consumes_lrs_fuel"] = True
        self.assert_invalid("TURN_ONLY must not consume LRS fuel")

    def test_raw_budget_mismatch_is_rejected(self) -> None:
        self.payload["cost_units"]["movement_cost_budget_raw"] = 30
        self.assert_invalid("movement_cost_budget_raw must be 40")

    def test_rejected_command_turn_consumption_is_rejected(self) -> None:
        self.payload["command_turn_rules"]["invalid_or_rejected_command_consumes_turn"] = True
        self.assert_invalid("rejected command must not consume turn")

    def test_missing_movement_rule_is_rejected(self) -> None:
        del self.payload["movement_rules"]["VECTOR_COMMAND"]
        self.assert_invalid("three comparison rules")

    def test_movement_points_diagonal_allowed_is_rejected(self) -> None:
        self.payload["movement_rules"]["MOVEMENT_POINTS"]["diagonal_allowed"] = True
        self.assert_invalid("4-directional")

    def test_movement_points_carryover_is_rejected(self) -> None:
        self.payload["movement_rules"]["MOVEMENT_POINTS"]["unused_budget_carryover"] = True
        self.assert_invalid("must not carry over")

    def test_move_to_actual_map_access_is_rejected(self) -> None:
        self.payload["movement_rules"]["MOVEMENT_POINTS"]["move_to_pathfinding"]["known_state_only"] = False
        self.assert_invalid("known state only")

    def test_vector_angle_origin_mismatch_is_rejected(self) -> None:
        self.payload["movement_rules"]["VECTOR_COMMAND"]["angle_degrees"]["origin"] = "E"
        self.assert_invalid("angle origin")

    def test_vector_path_resolution_mismatch_is_rejected(self) -> None:
        self.payload["movement_rules"]["VECTOR_COMMAND"]["path_resolution"] = "BRESENHAM"
        self.assert_invalid("SUPERCOVER_LINE")

    def test_thrust_direction_turning_is_rejected(self) -> None:
        self.payload["movement_rules"]["DIRECTIONAL_THRUST"]["direction_changes_within_command"] = "FREE"
        self.assert_invalid("THRUST must forbid turns")

    def test_stop_before_first_blocked_no_turn_is_rejected(self) -> None:
        self.payload["collision"]["first_blocked_cell"]["movement_command_consumes_srs_turn"] = False
        self.assert_invalid("first blocked movement must consume turn")

    def test_collision_cell_cost_consumption_is_rejected(self) -> None:
        self.payload["collision"]["movement_cost_consumed_on_collision_cell"] = True
        self.assert_invalid("collision cell cost")

    def test_nebula_observation_size_mismatch_is_rejected(self) -> None:
        self.payload["observation"]["LOCAL_MOVEMENT"]["nebula_size"] = 5
        self.assert_invalid("nebula_size must be 3")

    def test_resource_total_refuel_mismatch_is_rejected(self) -> None:
        self.payload["interaction"]["RESOURCE_CACHE"]["sector_total_refuel_amount"] = 6
        self.assert_invalid("RESOURCE_CACHE sector total")

    def test_resource_zero_refuel_consumption_is_rejected(self) -> None:
        self.payload["interaction"]["RESOURCE_CACHE"]["consume_when_refuel_amount_is_zero"] = True
        self.assert_invalid("zero-refuel RESOURCE_CACHE")

    def test_station_not_reusable_is_rejected(self) -> None:
        self.payload["interaction"]["STATION"]["reusable"] = False
        self.assert_invalid("STATION must be reusable")

    def test_salvage_effect_not_deferred_is_rejected(self) -> None:
        self.payload["interaction"]["SALVAGE"]["effect"] = "REPAIR"
        self.assert_invalid("SALVAGE must be deferred placeholder")

    def test_warp_exit_no_turn_is_rejected(self) -> None:
        self.payload["warp_exit"]["accepted_warp_or_exit_consumes_srs_turn"] = False
        self.assert_invalid("accepted warp/exit must consume turn")

    def test_missing_log_event_is_rejected(self) -> None:
        self.payload["game_log"]["required_events"].remove("OBSERVATION_UPDATED")
        self.assert_invalid("required events mismatch")

    def test_missing_metric_is_rejected(self) -> None:
        self.payload["evaluation_metrics"].remove("resource_refuel_amount")
        self.assert_invalid("evaluation_metrics mismatch")

    def test_cli_returns_nonzero_and_error_for_invalid_contract(self) -> None:
        self.payload["movement_schema_version"] = 2
        self.write()
        result = subprocess.run(
            [
                "python",
                "experiments/galactic_exodus/srs/validate_phase2_srs_movement.py",
                str(self.path),
            ],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("error:", result.stderr)

    def test_cli_reports_ok_for_valid_contract(self) -> None:
        result = subprocess.run(
            [
                "python",
                "experiments/galactic_exodus/srs/validate_phase2_srs_movement.py",
                "experiments/galactic_exodus/srs/phase2_srs_movement.json",
            ],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("Phase 2 SRS movement contract: OK", result.stdout)


if __name__ == "__main__":
    unittest.main()
