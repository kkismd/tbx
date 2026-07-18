from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus import replay_phase1_reference as replay
from experiments.galactic_exodus.archive.evaluation.phase1_lrs import validate_phase1_spec


FIXTURE_PATH = Path("experiments/galactic_exodus/fixtures/phase1_reference.json")


class Phase1ReferenceReplayTests(unittest.TestCase):
    def load_payload(self) -> dict[str, object]:
        return replay.load_fixture_file(FIXTURE_PATH)

    def fixture_by_name(self, name: str) -> dict[str, object]:
        payload = self.load_payload()
        fixtures = payload["fixtures"]
        for fixture in fixtures:
            if fixture["name"] == name:
                return copy.deepcopy(fixture)
        raise AssertionError(f"missing fixture {name}")

    def test_all_twelve_fixtures_replay_successfully(self) -> None:
        replay.replay_all(FIXTURE_PATH)

    def test_fixture_schema_mismatch_is_rejected(self) -> None:
        payload = self.load_payload()
        payload["schema_version"] = 999
        with tempfile.TemporaryDirectory() as tempdir:
            path = Path(tempdir) / "fixtures.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaises(validate_phase1_spec.ValidationError):
                replay.replay_all(path)

    def test_invalid_actual_map_is_rejected(self) -> None:
        fixture = self.fixture_by_name("normal_terrain_move")
        fixture["initial_actual_map"]["base_position"] = {"x": 8, "y": 8}
        with self.assertRaisesRegex(ValueError, "normal_terrain_move: .*base_position"):
            replay.replay_fixture(fixture)

    def test_generated_actual_map_mismatch_is_rejected(self) -> None:
        fixture = self.fixture_by_name("no_reroll_initial_board")
        fixture["initial_actual_map"]["cells"][0]["symbol"] = "N"
        with self.assertRaisesRegex(AssertionError, r"no_reroll_initial_board: \$\.initial_actual_map"):
            replay.replay_fixture(fixture)

    def test_expected_turn_mismatch_reports_fixture_name_and_path(self) -> None:
        fixture = self.fixture_by_name("normal_terrain_move")
        fixture["expected_turns"][0]["fuel_after"] = 999
        with self.assertRaisesRegex(AssertionError, r"normal_terrain_move: \$\.expected_turns\[0\]\.fuel_after"):
            replay.replay_fixture(fixture)

    def test_generation_error_fixture_replays_with_injected_generation_path(self) -> None:
        fixture = self.fixture_by_name("generation_error")

        log = replay.replay_fixture(fixture)

        self.assertIsNotNone(log.generation_error)
        self.assertEqual(log.generation_error.reason, "SEED_OVERFLOW")

    def test_fixed_start_goal_zero_fuel_arrival_fixture_wins(self) -> None:
        fixture = self.fixture_by_name("zero_fuel_goal_arrival_wins")

        log = replay.replay_fixture(fixture)

        self.assertEqual(log.final_summary.outcome, "WON")
        self.assertEqual(log.final_summary.remaining_fuel, 0)

    def test_positive_remaining_fuel_without_payable_actual_move_is_lost_fuel(self) -> None:
        fixture = self.fixture_by_name("fuel_depletion_loss")

        log = replay.replay_fixture(fixture)

        self.assertEqual(log.final_summary.outcome, "LOST_FUEL")
        self.assertGreater(log.final_summary.remaining_fuel, 0)


if __name__ == "__main__":
    unittest.main()
