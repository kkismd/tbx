from __future__ import annotations

import json
import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import apply_srs_command
from experiments.galactic_exodus.srs.log import build_srs_log
from experiments.galactic_exodus.srs.model import Direction, Position, SrsCombatPhase, SrsCombatState, SrsCommand, SrsEnemyTier, create_enemy_combat_state
from experiments.galactic_exodus.srs.run_fixture import (
    FIXTURES_DIR,
    REPO_ROOT,
    SrsFixtureError,
    fixture_result_to_jsonable,
    load_fixture,
    run_fixture,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state


REQUIRED_FIXTURES = {
    "move_route_basic_9x9.json",
    "move_to_known_9x9.json",
    "resource_cache_single_9x9.json",
    "station_refuel_9x9.json",
    "salvage_placeholder_9x9.json",
    "salvage_reject_recover_durability_9x9.json",
    "salvage_recover_energy_9x9.json",
    "salvage_recover_photon_torpedo_ammo_9x9.json",
    "base_upgrade_defense_9x9.json",
    "nebula_observation_3x3_9x9.json",
    "warp_exit_s_9x9.json",
    "warp_exit_rejected_no_flag_9x9.json",
    "rift_blocked_n_9x9.json",
    "turn_only_cost_9x9.json",
    "shared_fuel_cost_9x9.json",
    "revisit_resource_consumed_9x9.json",
    "discovered_cells_restore_9x9.json",
    "combat_core_state_9x9.json",
    "combat_attack_clear_los_9x9.json",
    "combat_attack_blocked_los_9x9.json",
    "combat_attack_out_of_range_9x9.json",
    "combat_enemy_movement_tiebreak_9x9.json",
    "combat_torpedo_destroy_no_counterattack_9x9.json",
    "combat_phaser_attack_damage_9x9.json",
    "combat_salvage_drop_tier3_energy_9x9.json",
    "combat_salvage_no_drop_tier1_9x9.json",
    "combat_enemy_defend_9x9.json",
    "combat_enemy_counterattack_9x9.json",
    "combat_enemy_counterattack_fallback_energy_9x9.json",
    "combat_energy_pressure_danger3_9x9.json",
    "combat_energy_pressure_danger4_9x9.json",
    "combat_encounter_spawn_cap_9x9.json",
    "combat_encounter_wait_nebula_9x9.json",
    "combat_encounter_wait_base_docked_9x9.json",
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
        self.assertEqual(payload["final_state"]["fuel"], 5)
        json.dumps(payload)

    def test_fixture_runner_validates_render_not_contains(self) -> None:
        path = FIXTURES_DIR / "move_route_basic_9x9.json"
        payload = dict(load_fixture(path))
        payload["expect"] = dict(payload["expect"])
        payload["expect"]["render_not_contains"] = "."

        with self.assertRaisesRegex(SrsFixtureError, "expect mismatch for render_not_contains"):
            from experiments.galactic_exodus.srs.run_fixture import run_fixture_data

            run_fixture_data(payload, contracts=self.contracts)

    def test_resource_cache_fixture_restores_manual_eval_discovered_cells(self) -> None:
        result = run_fixture(FIXTURES_DIR / "resource_cache_single_9x9.json", contracts=self.contracts)

        self.assertEqual(
            result.initial_state.known_state.discovered_cells,
            frozenset(
                {
                    Position(2, 6),
                    Position(2, 5),
                    Position(3, 6),
                    Position(1, 6),
                }
            ),
        )

    def test_revisit_resource_fixture_preserves_manual_eval_discovered_cells(self) -> None:
        result = run_fixture(FIXTURES_DIR / "revisit_resource_consumed_9x9.json", contracts=self.contracts)

        self.assertEqual(
            result.initial_state.known_state.discovered_cells,
            frozenset(
                {
                    Position(2, 6),
                    Position(2, 5),
                    Position(3, 6),
                    Position(1, 6),
                }
            ),
        )

    def test_nebula_fixture_applies_cell_override_before_observation(self) -> None:
        result = run_fixture(FIXTURES_DIR / "nebula_observation_3x3_9x9.json", contracts=self.contracts)

        self.assertEqual(len(result.final_state.known_state.discovered_cells), 9)
        self.assertEqual(result.final_state.actual_map.cell_at(Position(4, 1)).terrain.value, "NEBULA")

    def test_game_log_json_serializable(self) -> None:
        result = run_fixture(FIXTURES_DIR / "move_route_basic_9x9.json", contracts=self.contracts)
        payload = {"events": [dict(event.payload) | {"event_type": event.event_type, "srs_turn": event.srs_turn} for event in build_srs_log(result.log.events).events]}

        json.dumps(payload)

    def test_combat_fixture_blocks_warp_and_advances_phase_deterministically(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_core_state_9x9.json", contracts=self.contracts)

        self.assertEqual(result.final_state.combat_state.phase.value, "ENEMY_ACTION")
        self.assertEqual(result.final_state.combat_state.combat_turn, 1)
        self.assertTrue(result.final_state.combat_state.enemy_presence)
        self.assertEqual(result.final_state.combat_state.player.energy, 6)

    def test_combat_attackability_fixtures_cover_allow_and_reject_cases(self) -> None:
        clear = run_fixture(FIXTURES_DIR / "combat_attack_clear_los_9x9.json", contracts=self.contracts)
        blocked = run_fixture(FIXTURES_DIR / "combat_attack_blocked_los_9x9.json", contracts=self.contracts)
        out_of_range = run_fixture(FIXTURES_DIR / "combat_attack_out_of_range_9x9.json", contracts=self.contracts)

        self.assertEqual(clear.final_state.combat_state.phase.value, "PLAYER_ATTACK")
        self.assertEqual(blocked.final_state.combat_state.phase.value, "ENEMY_ACTION")
        self.assertEqual(out_of_range.final_state.combat_state.phase.value, "ENEMY_ACTION")

    def test_enemy_movement_fixture_keeps_target_path_and_final_position_deterministic(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_enemy_movement_tiebreak_9x9.json", contracts=self.contracts)

        self.assertEqual(result.final_state.combat_state.phase.value, "PLAYER_MOVEMENT")
        self.assertEqual(result.final_state.combat_state.enemies["enemy-1"].position, Position(2, 5))
        self.assertEqual(
            result.summary["enemy_actions"],
            [
                {
                    "enemy_id": "enemy-1",
                    "start_position": [0, 4],
                    "target_attackable_position": [4, 5],
                    "planned_path": [[0, 5], [1, 5], [2, 5], [3, 5], [4, 5]],
                    "moved_path": [[0, 5], [1, 5], [2, 5]],
                    "final_position": [2, 5],
                    "movement_power": 3,
                    "movement_cost": 50,
                    "attacked_player": False,
                    "can_attack_before_move": False,
                    "can_attack_after_move": False,
                    "reaction": None,
                }
            ],
        )

    def test_combat_resolution_fixtures_cover_attack_and_reaction_outcomes(self) -> None:
        torpedo = run_fixture(FIXTURES_DIR / "combat_torpedo_destroy_no_counterattack_9x9.json", contracts=self.contracts)
        phaser = run_fixture(FIXTURES_DIR / "combat_phaser_attack_damage_9x9.json", contracts=self.contracts)
        defend = run_fixture(FIXTURES_DIR / "combat_enemy_defend_9x9.json", contracts=self.contracts)
        counterattack = run_fixture(FIXTURES_DIR / "combat_enemy_counterattack_9x9.json", contracts=self.contracts)
        fallback = run_fixture(
            FIXTURES_DIR / "combat_enemy_counterattack_fallback_energy_9x9.json",
            contracts=self.contracts,
        )

        self.assertFalse(torpedo.final_state.combat_state.enemy_presence)
        self.assertEqual(torpedo.final_state.combat_state.player.photon_torpedo_ammo, 5)
        self.assertEqual(phaser.final_state.combat_state.enemies["enemy-1"].durability, 4)
        self.assertEqual(phaser.final_state.combat_state.player.energy, 5)
        self.assertEqual(defend.final_state.combat_state.player.durability, 96)
        self.assertEqual(
            defend.summary["enemy_actions"][0]["reaction"]["resolved_reaction"],
            "DEFEND",
        )
        self.assertEqual(counterattack.final_state.combat_state.player.durability, 94)
        self.assertEqual(counterattack.final_state.combat_state.enemies["enemy-1"].durability, 2)
        self.assertEqual(
            counterattack.summary["enemy_actions"][0]["reaction"]["resolved_reaction"],
            "COUNTERATTACK",
        )
        self.assertTrue(fallback.summary["enemy_actions"][0]["reaction"]["fallback_to_defend"])
        self.assertEqual(fallback.final_state.combat_state.player.energy, 1)

    def test_multi_enemy_pressure_fixtures_preserve_tier_order_and_energy_pressure(self) -> None:
        danger3 = run_fixture(FIXTURES_DIR / "combat_energy_pressure_danger3_9x9.json", contracts=self.contracts)
        danger4 = run_fixture(FIXTURES_DIR / "combat_energy_pressure_danger4_9x9.json", contracts=self.contracts)

        self.assertEqual(
            [action["enemy_id"] for action in danger3.log.events[1].payload["enemy_actions"]],
            ["enemy-1", "enemy-2", "enemy-4"],
        )
        self.assertEqual(danger3.log.events[1].payload["player_energy_after"], 3)
        self.assertEqual(danger3.log.events[4].payload["player_energy_after"], 1)
        self.assertTrue(danger3.log.events[4].payload["enemy_actions"][2]["reaction"]["fallback_to_defend"])
        self.assertEqual(
            [action["enemy_id"] for action in danger4.log.events[1].payload["enemy_actions"]],
            ["enemy-1", "enemy-2", "enemy-3", "enemy-4"],
        )
        self.assertEqual(danger4.log.events[1].payload["player_energy_after"], 2)
        self.assertEqual(danger4.log.events[4].payload["player_energy_after"], 1)
        self.assertTrue(danger4.log.events[4].payload["enemy_actions"][1]["reaction"]["fallback_to_defend"])

    def test_encounter_spawn_fixture_accepts_fixed_composition_and_spawn_cap(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_spawn_cap_9x9.json", contracts=self.contracts)

        self.assertEqual(
            result.summary["combat_enemy_positions"],
            {
                "enemy-1": [1, 0],
                "enemy-2": [2, 0],
                "enemy-3": [3, 0],
            },
        )
        self.assertEqual(
            result.summary["combat_enemy_durabilities"],
            {
                "enemy-1": 3,
                "enemy-2": 3,
                "enemy-3": 5,
            },
        )

    def test_custom_orthogonal_raw_cost_is_used_by_movement_and_enemy_pathfinding(self) -> None:
        custom_contracts = replace(
            self.contracts,
            movement=replace(
                self.contracts.movement,
                orthogonal_raw_cost=7,
                movement_cost_budget_raw=28,
            ),
        )

        movement_state = make_state()
        movement_result = apply_srs_command(
            movement_state,
            SrsCommand(command_type="MOVE_ROUTE", route=(Direction.N,)),
            contracts=custom_contracts,
        )
        self.assertEqual(movement_result.events[0].payload["movement_raw_cost"], 7)

        enemy_state = replace(make_state(), player_position=Position(4, 4))
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(0, 4),
        )
        enemy_state = replace(
            enemy_state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.ENEMY_ACTION,
            ),
        )
        enemy_result = apply_srs_command(
            enemy_state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=custom_contracts,
        )

        self.assertEqual(enemy_result.events[0].payload["enemy_actions"][0]["movement_cost"], 14)

    def test_required_fixture_set_exists(self) -> None:
        existing = {path.name for path in FIXTURES_DIR.glob("*.json")}
        self.assertTrue(REQUIRED_FIXTURES.issubset(existing))
