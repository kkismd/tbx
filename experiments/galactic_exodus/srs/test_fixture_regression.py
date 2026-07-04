from __future__ import annotations

import unittest

from experiments.galactic_exodus.srs.model import Position
from experiments.galactic_exodus.srs.run_fixture import FIXTURES_DIR, SrsFixtureRunResult, run_fixture


def run_named_fixture(name: str) -> SrsFixtureRunResult:
    return run_fixture(FIXTURES_DIR / f"{name}.json")


def event_types(result: SrsFixtureRunResult) -> list[str]:
    return [event.event_type for event in result.log.events]


def primary_outcome(result: SrsFixtureRunResult) -> object:
    return result.log.events[0].payload.get("outcome")


class SrsFixtureRegressionTests(unittest.TestCase):
    def test_move_route_basic(self) -> None:
        result = run_named_fixture("move_route_basic_9x9")

        self.assertIn("MOVE_ACCEPTED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.srs_turn, 1)
        self.assertEqual(result.final_state.player_position, Position(4, 7))
        self.assertEqual(result.final_state.fuel, 0)

    def test_move_to_known(self) -> None:
        result = run_named_fixture("move_to_known_9x9")

        self.assertIn("MOVE_ACCEPTED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.srs_turn, 1)
        self.assertEqual(result.final_state.player_position, Position(4, 6))
        self.assertEqual(result.final_state.fuel, 0)

    def test_resource_cache_single(self) -> None:
        result = run_named_fixture("resource_cache_single_9x9")

        self.assertIn("INTERACT_ACCEPTED", event_types(result))
        self.assertIn("OBJECT_CONSUMED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.fuel, 7)
        self.assertIn("resource-cache-1", result.final_state.persistent_state.consumed_object_ids)
        self.assertTrue(result.final_state.objects["resource-cache-1"].consumed)
        self.assertIn(Position(2, 7), result.final_state.known_state.discovered_cells)

    def test_station_refuel(self) -> None:
        result = run_named_fixture("station_refuel_9x9")

        self.assertIn("INTERACT_ACCEPTED", event_types(result))
        self.assertIn("STATION_ACTIVATED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.fuel, result.final_state.max_fuel)
        self.assertIn("station-1", result.final_state.persistent_state.activated_object_ids)
        self.assertTrue(result.final_state.objects["station-1"].activated)

    def test_salvage_placeholder(self) -> None:
        result = run_named_fixture("salvage_placeholder_9x9")

        self.assertIn("INTERACT_ACCEPTED", event_types(result))
        self.assertIn("OBJECT_CONSUMED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertIn("salvage-1", result.final_state.persistent_state.consumed_object_ids)
        self.assertTrue(result.final_state.objects["salvage-1"].consumed)

    def test_warp_exit_s(self) -> None:
        result = run_named_fixture("warp_exit_s_9x9")

        self.assertIn("WARP_EXIT_ACCEPTED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.srs_turn, 1)
        self.assertEqual(result.final_state.player_position, Position(4, 8))
        self.assertEqual(result.final_state.fuel, 0)

    def test_rift_blocked_n(self) -> None:
        result = run_named_fixture("rift_blocked_n_9x9")

        self.assertTrue(any("REJECTED" in event_type for event_type in event_types(result)))
        self.assertEqual(primary_outcome(result), "REJECTED_BLOCKED_EDGE")
        self.assertEqual(result.final_state.srs_turn, 0)
        self.assertEqual(result.final_state.player_position, Position(4, 0))
        self.assertEqual(result.final_state.fuel, 0)

    def test_shared_fuel_cost_uses_shared_fuel_without_fixing_exact_delta(self) -> None:
        result = run_named_fixture("shared_fuel_cost_9x9")

        self.assertEqual(result.summary["cost_mode"], "SHARED_FUEL")
        self.assertIn("MOVE_ACCEPTED", event_types(result))
        self.assertLess(result.final_state.fuel, result.initial_state.fuel)

    def test_revisit_resource_consumed(self) -> None:
        result = run_named_fixture("revisit_resource_consumed_9x9")

        self.assertIn("INTERACT_REJECTED", event_types(result))
        self.assertEqual(primary_outcome(result), "REJECTED_ALREADY_CONSUMED")
        self.assertEqual(result.final_state.srs_turn, 0)
        self.assertEqual(result.final_state.fuel, 2)
        self.assertIn("resource-cache-1", result.final_state.persistent_state.consumed_object_ids)
        self.assertTrue(result.final_state.objects["resource-cache-1"].consumed)
        self.assertIn(Position(2, 7), result.final_state.known_state.discovered_cells)

    def test_combat_enemy_movement_tiebreak(self) -> None:
        result = run_named_fixture("combat_enemy_movement_tiebreak_9x9")

        self.assertIn("COMBAT_TRANSITIONED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.combat_state.enemies["enemy-1"].position, Position(2, 3))
        self.assertEqual(
            result.summary["enemy_actions"][0]["target_attackable_position"],
            [4, 3],
        )
