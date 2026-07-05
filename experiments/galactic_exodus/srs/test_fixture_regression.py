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
        self.assertEqual(result.final_state.player_position, Position(5, 8))
        self.assertEqual(result.final_state.fuel, 0)

    def test_move_to_known(self) -> None:
        result = run_named_fixture("move_to_known_9x9")

        self.assertIn("MOVE_ACCEPTED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.srs_turn, 1)
        self.assertEqual(result.final_state.player_position, Position(5, 7))
        self.assertEqual(result.final_state.fuel, 0)

    def test_resource_cache_single(self) -> None:
        result = run_named_fixture("resource_cache_single_9x9")

        self.assertIn("INTERACT_ACCEPTED", event_types(result))
        self.assertIn("OBJECT_CONSUMED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.fuel, 5)
        self.assertIn("resource-cache-1", result.final_state.persistent_state.consumed_object_ids)
        self.assertTrue(result.final_state.objects["resource-cache-1"].consumed)
        self.assertIn(Position(3, 8), result.final_state.known_state.discovered_cells)

    def test_station_refuel(self) -> None:
        result = run_named_fixture("station_refuel_9x9")

        self.assertIn("INTERACT_ACCEPTED", event_types(result))
        self.assertIn("STATION_ACTIVATED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.fuel, result.final_state.max_fuel)
        self.assertEqual(result.final_state.player_state.durability, 100)
        self.assertEqual(result.final_state.player_state.energy, 6)
        self.assertEqual(result.final_state.player_state.photon_torpedo_ammo, 6)
        self.assertIn("station-1", result.final_state.persistent_state.activated_object_ids)
        self.assertTrue(result.final_state.objects["station-1"].activated)

    def test_salvage_placeholder(self) -> None:
        result = run_named_fixture("salvage_placeholder_9x9")

        self.assertIn("INTERACT_ACCEPTED", event_types(result))
        self.assertIn("OBJECT_CONSUMED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertIn("salvage-1", result.final_state.persistent_state.consumed_object_ids)
        self.assertTrue(result.final_state.objects["salvage-1"].consumed)
        self.assertEqual(result.final_state.player_state.salvage, 1)

    def test_salvage_recover_durability(self) -> None:
        result = run_named_fixture("salvage_recover_durability_9x9")

        self.assertEqual(result.final_state.player_state.durability, 100)
        self.assertEqual(result.final_state.player_state.salvage, 1)

    def test_base_upgrade_defense(self) -> None:
        result = run_named_fixture("base_upgrade_defense_9x9")

        self.assertEqual(result.final_state.player_state.salvage, 0)
        self.assertEqual(result.final_state.player_state.defense, 2)

    def test_warp_exit_s(self) -> None:
        result = run_named_fixture("warp_exit_s_9x9")

        self.assertIn("WARP_EXIT_ACCEPTED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.srs_turn, 1)
        self.assertEqual(result.final_state.player_position, Position(5, 9))
        self.assertEqual(result.final_state.fuel, 0)

    def test_rift_blocked_n(self) -> None:
        result = run_named_fixture("rift_blocked_n_9x9")

        self.assertTrue(any("REJECTED" in event_type for event_type in event_types(result)))
        self.assertEqual(primary_outcome(result), "REJECTED_BLOCKED_EDGE")
        self.assertEqual(result.final_state.srs_turn, 0)
        self.assertEqual(result.final_state.player_position, Position(5, 1))
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
        self.assertIn(Position(3, 8), result.final_state.known_state.discovered_cells)

    def test_combat_enemy_movement_tiebreak(self) -> None:
        result = run_named_fixture("combat_enemy_movement_tiebreak_9x9")

        self.assertIn("COMBAT_TRANSITIONED", event_types(result))
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertEqual(result.final_state.combat_state.enemies["enemy-1"].position, Position(3, 4))
        self.assertEqual(
            result.summary["enemy_actions"][0]["target_attackable_position"],
            [5, 4],
        )
        self.assertIsNone(result.summary["enemy_actions"][0]["reaction"])

    def test_combat_torpedo_destroy_no_counterattack(self) -> None:
        result = run_named_fixture("combat_torpedo_destroy_no_counterattack_9x9")

        self.assertEqual(result.final_state.combat_state.player.photon_torpedo_ammo, 5)
        self.assertFalse(result.final_state.combat_state.enemy_presence)
        self.assertEqual(result.summary["enemy_actions"], [])

    def test_combat_phaser_attack_damage(self) -> None:
        result = run_named_fixture("combat_phaser_attack_damage_9x9")

        self.assertEqual(result.final_state.combat_state.player.energy, 5)
        self.assertEqual(result.final_state.combat_state.enemies["enemy-1"].durability, 4)

    def test_combat_salvage_drop_tier3_energy(self) -> None:
        result = run_named_fixture("combat_salvage_drop_tier3_energy_9x9")

        self.assertEqual(result.final_state.player_state.energy, 6)
        self.assertEqual(result.final_state.player_state.salvage, 3)
        self.assertEqual(
            result.log.events[0].payload["player_action"]["salvage_reward"]["selected_salvage_choice"],
            "RECOVER_ENERGY",
        )

    def test_combat_salvage_no_drop_tier1(self) -> None:
        result = run_named_fixture("combat_salvage_no_drop_tier1_9x9")

        self.assertEqual(result.final_state.player_state.salvage, 2)

    def test_combat_enemy_defend(self) -> None:
        result = run_named_fixture("combat_enemy_defend_9x9")

        self.assertEqual(result.final_state.combat_state.player.durability, 96)
        self.assertEqual(result.summary["enemy_actions"][0]["reaction"]["resolved_reaction"], "DEFEND")

    def test_combat_enemy_counterattack(self) -> None:
        result = run_named_fixture("combat_enemy_counterattack_9x9")

        self.assertEqual(result.final_state.combat_state.player.durability, 94)
        self.assertEqual(result.final_state.combat_state.enemies["enemy-1"].durability, 2)
        self.assertEqual(result.summary["enemy_actions"][0]["reaction"]["resolved_reaction"], "COUNTERATTACK")

    def test_combat_enemy_counterattack_fallback_energy(self) -> None:
        result = run_named_fixture("combat_enemy_counterattack_fallback_energy_9x9")

        self.assertEqual(result.final_state.combat_state.player.durability, 97)
        self.assertEqual(result.final_state.combat_state.player.energy, 1)
        self.assertTrue(result.summary["enemy_actions"][0]["reaction"]["fallback_to_defend"])

    def test_combat_energy_pressure_danger3(self) -> None:
        result = run_named_fixture("combat_energy_pressure_danger3_9x9")

        self.assertEqual(result.final_state.combat_state.player.energy, 1)
        self.assertEqual(result.final_state.combat_state.player.durability, 59)
        self.assertEqual(
            [action["enemy_id"] for action in result.log.events[1].payload["enemy_actions"]],
            ["enemy-1", "enemy-2", "enemy-4"],
        )
        self.assertEqual(result.log.events[1].payload["player_energy_after"], 3)
        self.assertEqual(result.log.events[4].payload["player_energy_after"], 1)
        self.assertEqual(
            [action["reaction"]["resolved_reaction"] for action in result.log.events[4].payload["enemy_actions"]],
            ["COUNTERATTACK", "COUNTERATTACK", "DEFEND"],
        )

    def test_combat_energy_pressure_danger4(self) -> None:
        result = run_named_fixture("combat_energy_pressure_danger4_9x9")

        self.assertEqual(result.final_state.combat_state.player.energy, 1)
        self.assertEqual(result.final_state.combat_state.player.durability, 50)
        self.assertEqual(
            [action["enemy_id"] for action in result.log.events[1].payload["enemy_actions"]],
            ["enemy-1", "enemy-2", "enemy-3", "enemy-4"],
        )
        self.assertEqual(result.log.events[1].payload["player_energy_after"], 2)
        self.assertEqual(result.log.events[4].payload["player_energy_after"], 1)
        self.assertEqual(
            [action["reaction"]["resolved_reaction"] for action in result.log.events[4].payload["enemy_actions"]],
            ["COUNTERATTACK", "DEFEND", "DEFEND", "DEFEND"],
        )

    def test_combat_encounter_spawn_cap(self) -> None:
        result = run_named_fixture("combat_encounter_spawn_cap_9x9")

        self.assertEqual(result.log.events, ())
        self.assertEqual(
            result.summary["combat_enemy_positions"],
            {
                "enemy-1": [9, 5],
                "enemy-2": [5, 9],
            },
        )
        self.assertEqual(
            result.summary["combat_enemy_durabilities"],
            {
                "enemy-1": 3,
                "enemy-2": 5,
            },
        )

    def test_combat_encounter_wait_nebula(self) -> None:
        result = run_named_fixture("combat_encounter_wait_nebula_9x9")

        self.assertEqual(event_types(result), ["WAIT_ACCEPTED", "ENCOUNTER_ROLLED"])
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertTrue(result.final_state.combat_state.enemy_presence)
        self.assertAlmostEqual(result.log.events[1].payload["actual_encounter_chance"], 0.126)

    def test_combat_encounter_wait_base_docked(self) -> None:
        result = run_named_fixture("combat_encounter_wait_base_docked_9x9")

        self.assertEqual(event_types(result), ["WAIT_ACCEPTED"])
        self.assertEqual(primary_outcome(result), "ACCEPTED")
        self.assertIsNone(result.final_state.combat_state)
