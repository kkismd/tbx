from __future__ import annotations

import json
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.log import build_srs_log
from experiments.galactic_exodus.srs.model import Position
from experiments.galactic_exodus.srs.run_fixture import (
    FIXTURES_DIR,
    REPO_ROOT,
    SrsFixtureError,
    fixture_result_to_jsonable,
    load_fixture,
    run_fixture,
)


REQUIRED_FIXTURES = {
    "move_route_basic_9x9.json",
    "move_to_known_9x9.json",
    "resource_cache_single_9x9.json",
    "station_refuel_9x9.json",
    "salvage_placeholder_9x9.json",
    "nebula_observation_3x3_9x9.json",
    "warp_exit_s_9x9.json",
    "warp_exit_rejected_no_flag_9x9.json",
    "rift_blocked_n_9x9.json",
    "turn_only_cost_9x9.json",
    "shared_fuel_cost_9x9.json",
    "revisit_resource_consumed_9x9.json",
    "discovered_cells_restore_9x9.json",
}


class SrsFixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_all_fixture_json_loads(self) -> None:
        for path in sorted(FIXTURES_DIR.glob("*.json")):
            with self.subTest(path=path.name):
                payload = load_fixture(path)
                self.assertIsInstance(payload, dict)

    def test_all_fixture_runs_are_deterministic(self) -> None:
        for path in sorted(FIXTURES_DIR.glob("*_9x9.json")):
            if path.name not in REQUIRED_FIXTURES:
                continue
            with self.subTest(path=path.name):
                first = fixture_result_to_jsonable(run_fixture(path, contracts=self.contracts))
                second = fixture_result_to_jsonable(run_fixture(path, contracts=self.contracts))
                self.assertEqual(first, second)

    def test_fixture_runner_validates_expectations(self) -> None:
        path = FIXTURES_DIR / "resource_cache_single_9x9.json"
        payload = dict(load_fixture(path))
        payload["expect"] = dict(payload["expect"])
        payload["expect"]["fuel"] = 999

        with self.assertRaisesRegex(SrsFixtureError, "expect mismatch for fuel"):
            from experiments.galactic_exodus.srs.run_fixture import run_fixture_data

            run_fixture_data(payload, contracts=self.contracts)

    def test_fixture_runner_rejects_unknown_command_field(self) -> None:
        path = FIXTURES_DIR / "move_route_basic_9x9.json"
        payload = dict(load_fixture(path))
        payload["commands"] = [dict(payload["commands"][0], bad_field=True)]

        with self.assertRaisesRegex(SrsFixtureError, "unknown command field"):
            from experiments.galactic_exodus.srs.run_fixture import run_fixture_data

            run_fixture_data(payload, contracts=self.contracts)

    def test_fixture_runner_outputs_jsonable_result(self) -> None:
        result = run_fixture(FIXTURES_DIR / "resource_cache_single_9x9.json", contracts=self.contracts)
        payload = fixture_result_to_jsonable(result)

        self.assertEqual(payload["fixture_id"], "resource_cache_single_9x9")
        self.assertEqual(payload["final_state"]["fuel"], 7)
        json.dumps(payload)

    def test_resource_cache_fixture_restores_manual_eval_discovered_cells(self) -> None:
        result = run_fixture(FIXTURES_DIR / "resource_cache_single_9x9.json", contracts=self.contracts)

        self.assertEqual(
            result.initial_state.known_state.discovered_cells,
            frozenset(
                {
                    Position(2, 7),
                    Position(2, 6),
                    Position(3, 7),
                    Position(1, 7),
                }
            ),
        )

    def test_revisit_resource_fixture_preserves_manual_eval_discovered_cells(self) -> None:
        result = run_fixture(FIXTURES_DIR / "revisit_resource_consumed_9x9.json", contracts=self.contracts)

        self.assertEqual(
            result.initial_state.known_state.discovered_cells,
            frozenset(
                {
                    Position(2, 7),
                    Position(2, 6),
                    Position(3, 7),
                    Position(1, 7),
                }
            ),
        )

    def test_nebula_fixture_applies_cell_override_before_observation(self) -> None:
        result = run_fixture(FIXTURES_DIR / "nebula_observation_3x3_9x9.json", contracts=self.contracts)

        self.assertEqual(len(result.final_state.known_state.discovered_cells), 9)
        self.assertEqual(result.final_state.actual_map.cell_at(Position(4, 7)).terrain.value, "NEBULA")

    def test_game_log_json_serializable(self) -> None:
        result = run_fixture(FIXTURES_DIR / "move_route_basic_9x9.json", contracts=self.contracts)
        payload = {"events": [dict(event.payload) | {"event_type": event.event_type, "srs_turn": event.srs_turn} for event in build_srs_log(result.log.events).events]}

        json.dumps(payload)

    def test_required_fixture_set_exists(self) -> None:
        existing = {path.name for path in FIXTURES_DIR.glob("*.json")}
        self.assertTrue(REQUIRED_FIXTURES.issubset(existing))
