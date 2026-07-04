from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import (
    apply_srs_command,
    bresenham_line,
    enemy_attackable_positions,
    has_clear_line_of_sight,
    is_attackable_position,
    run_srs_commands,
)
from experiments.galactic_exodus.srs.log import COMBAT_TRANSITIONED, WARP_EXIT_REJECTED
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SrsBaseUpgrade,
    SrsCombatPhase,
    SrsCombatState,
    SrsCommand,
    SrsEnemyTier,
    SrsSalvageChoice,
    SrsTerrainType,
    SrsWeaponType,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state, replace_cell_terrain


REPO_ROOT = Path(__file__).resolve().parents[3]


class SrsEngineCombatTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_combat_step_moves_to_attack_phase_when_target_available(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=state.player_position,
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, COMBAT_TRANSITIONED)
        self.assertEqual(result.state.combat_state.phase, SrsCombatPhase.PLAYER_ATTACK)
        self.assertEqual(result.state.combat_state.combat_turn, 0)

    def test_bresenham_line_returns_expected_cells(self) -> None:
        line = bresenham_line(Position(1, 1), Position(4, 3))

        self.assertEqual(
            line,
            (
                Position(1, 1),
                Position(2, 2),
                Position(3, 2),
                Position(4, 3),
            ),
        )

    def test_line_of_sight_is_blocked_by_impassable_intermediate_cell(self) -> None:
        state = replace_cell_terrain(make_state(), Position(2, 4), SrsTerrainType.ASTEROID)

        self.assertFalse(
            has_clear_line_of_sight(
                state,
                attacker=Position(1, 4),
                target=Position(3, 4),
            )
        )

    def test_line_of_sight_ignores_attacker_and_target_cells_for_blocking(self) -> None:
        state = make_state()
        state = replace_cell_terrain(state, Position(1, 4), SrsTerrainType.ASTEROID)
        state = replace_cell_terrain(state, Position(3, 4), SrsTerrainType.ASTEROID)

        self.assertTrue(
            has_clear_line_of_sight(
                state,
                attacker=Position(1, 4),
                target=Position(3, 4),
            )
        )

    def test_attackable_position_uses_fixed_weapon_range(self) -> None:
        state = make_state()

        self.assertTrue(
            is_attackable_position(
                state,
                attacker=Position(1, 4),
                target=Position(4, 4),
                weapon_type=SrsWeaponType.PHOTON_TORPEDO,
            )
        )
        self.assertFalse(
            is_attackable_position(
                state,
                attacker=Position(1, 4),
                target=Position(4, 4),
                weapon_type=SrsWeaponType.PHASER,
            )
        )

    def test_enemy_attackable_positions_enumerates_clear_los_cells(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))

        positions = enemy_attackable_positions(state)

        self.assertIn(Position(2, 4), positions)
        self.assertIn(Position(4, 2), positions)
        self.assertIn(Position(6, 6), positions)
        self.assertNotIn(Position(4, 4), positions)
        self.assertNotIn(Position(1, 4), positions)

    def test_combat_step_skips_attack_phase_without_target(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=state.player_position,
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.phase, SrsCombatPhase.ENEMY_ACTION)

    def test_combat_step_skips_attack_phase_when_line_of_sight_is_blocked(self) -> None:
        state = replace_cell_terrain(make_state(), Position(2, 4), SrsTerrainType.ASTEROID)
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(3, 4),
        )
        state = replace(
            state,
            player_position=Position(1, 4),
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.phase, SrsCombatPhase.ENEMY_ACTION)
        self.assertFalse(result.events[0].payload["target_attackable"])

    def test_combat_step_skips_attack_phase_when_target_is_out_of_range(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(5, 4),
        )
        state = replace(
            state,
            player_position=Position(1, 4),
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.phase, SrsCombatPhase.ENEMY_ACTION)
        self.assertFalse(result.events[0].payload["target_attackable"])

    def test_player_attack_with_torpedo_consumes_ammo_and_removes_destroyed_enemy(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(3, 4),
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.PLAYER_ATTACK,
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="COMBAT_STEP",
                player_attack_action="ATTACK",
                player_attack_weapon="PHOTON_TORPEDO",
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.phase, SrsCombatPhase.ENEMY_ACTION)
        self.assertEqual(result.state.combat_state.player.photon_torpedo_ammo, 5)
        self.assertEqual(result.state.combat_state.enemies, {})
        self.assertIsNone(result.state.combat_state.player_attack_target_id)
        self.assertEqual(
            result.events[0].payload["player_action"],
            {
                "selected_action": "ATTACK",
                "selected_weapon": "PHOTON_TORPEDO",
                "target_enemy_id": "enemy-1",
                "attack_executed": True,
                "damage_applied": 3,
                "resource_cost": 1,
                "resource_type": "PHOTON_TORPEDO_AMMO",
                "target_destroyed": True,
            },
        )

    def test_destroyed_enemy_drop_adds_salvage_and_applies_choice(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        enemy = create_enemy_combat_state(
            enemy_id="enemy-3",
            tier=SrsEnemyTier.TIER3,
            position=Position(3, 4),
            drop_salvage=True,
        )
        enemy = replace(enemy, durability=3)
        state = replace(
            state,
            player_state=replace(state.player_state, energy=3, salvage=1),
            combat_state=SrsCombatState(
                player=replace(state.player_state, energy=3, salvage=1),
                enemies={"enemy-3": enemy},
                phase=SrsCombatPhase.PLAYER_ATTACK,
                player_attack_target_id="enemy-3",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="COMBAT_STEP",
                player_attack_action="ATTACK",
                player_attack_weapon="PHOTON_TORPEDO",
                salvage_choice=SrsSalvageChoice.RECOVER_ENERGY,
            ),
            contracts=self.contracts,
        )

        self.assertFalse(result.state.combat_state.enemy_presence)
        self.assertEqual(result.state.player_state.energy, 6)
        self.assertEqual(result.state.player_state.salvage, 3)
        self.assertEqual(result.events[0].payload["player_action"]["salvage_reward"]["selected_salvage_choice"], "RECOVER_ENERGY")

    def test_destroyed_enemy_without_drop_does_not_change_salvage(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(3, 4),
            drop_salvage=False,
        )
        state = replace(
            state,
            player_state=replace(state.player_state, salvage=2),
            combat_state=SrsCombatState(
                player=replace(state.player_state, salvage=2),
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.PLAYER_ATTACK,
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="COMBAT_STEP",
                player_attack_action="ATTACK",
                player_attack_weapon="PHOTON_TORPEDO",
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_state.salvage, 2)
        self.assertNotIn("salvage_reward", result.events[0].payload["player_action"])

    def test_player_attack_with_phaser_consumes_energy_and_applies_fixed_damage(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER2,
            position=Position(2, 4),
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.PLAYER_ATTACK,
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="COMBAT_STEP",
                player_attack_action="ATTACK",
                player_attack_weapon="PHASER",
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.player.energy, 5)
        self.assertEqual(result.state.combat_state.enemies["enemy-1"].durability, 4)
        self.assertEqual(result.events[0].payload["player_action"]["damage_applied"], 1)

    def test_player_attack_can_skip_without_consuming_resources(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(3, 4),
        )
        combat_state = SrsCombatState(
            enemies={"enemy-1": enemy},
            phase=SrsCombatPhase.PLAYER_ATTACK,
            player_attack_target_id="enemy-1",
        )
        state = replace(state, combat_state=combat_state)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP", player_attack_action="SKIP"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.player.energy, combat_state.player.energy)
        self.assertEqual(
            result.state.combat_state.player.photon_torpedo_ammo,
            combat_state.player.photon_torpedo_ammo,
        )
        self.assertEqual(result.events[0].payload["player_action"]["selected_action"], "SKIP")

    def test_enemy_action_advances_combat_turn_and_recovers_energy(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=state.player_position,
        )
        combat_state = SrsCombatState(
            enemies={"enemy-1": enemy},
            player_attack_target_id="enemy-1",
        )
        combat_state = replace(combat_state, player=replace(combat_state.player, energy=5), phase=SrsCombatPhase.ENEMY_ACTION)
        state = replace(state, combat_state=combat_state)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.combat_state.phase, SrsCombatPhase.PLAYER_MOVEMENT)
        self.assertEqual(result.state.combat_state.combat_turn, 1)
        self.assertEqual(result.state.combat_state.player.energy, 6)

    def test_combat_state_normalizes_enemy_order_by_tier(self) -> None:
        enemy_t4 = create_enemy_combat_state(
            enemy_id="enemy-4",
            tier=SrsEnemyTier.TIER4,
            position=Position(2, 4),
        )
        enemy_t2 = create_enemy_combat_state(
            enemy_id="enemy-2",
            tier=SrsEnemyTier.TIER2,
            position=Position(1, 3),
        )
        enemy_t1 = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(0, 4),
        )

        combat_state = SrsCombatState(
            enemies={
                "enemy-4": enemy_t4,
                "enemy-2": enemy_t2,
                "enemy-1": enemy_t1,
            },
        )

        self.assertEqual(tuple(combat_state.enemies), ("enemy-1", "enemy-2", "enemy-4"))

    def test_multiple_enemy_actions_follow_tier_ascending_order(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        enemy_t4 = create_enemy_combat_state(
            enemy_id="enemy-4",
            tier=SrsEnemyTier.TIER4,
            position=Position(2, 4),
        )
        enemy_t2 = create_enemy_combat_state(
            enemy_id="enemy-2",
            tier=SrsEnemyTier.TIER2,
            position=Position(1, 3),
        )
        enemy_t1 = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(0, 4),
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={
                    "enemy-4": enemy_t4,
                    "enemy-2": enemy_t2,
                    "enemy-1": enemy_t1,
                },
                phase=SrsCombatPhase.ENEMY_ACTION,
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="COMBAT_STEP",
                enemy_reactions={
                    "enemy-1": "COUNTERATTACK",
                    "enemy-2": "COUNTERATTACK",
                    "enemy-4": "COUNTERATTACK",
                },
            ),
            contracts=self.contracts,
        )

        enemy_actions = result.events[0].payload["enemy_actions"]
        self.assertEqual(
            [action["enemy_id"] for action in enemy_actions],
            ["enemy-1", "enemy-2", "enemy-4"],
        )
        self.assertEqual(result.state.combat_state.player.energy, 4)
        self.assertEqual(result.state.combat_state.enemies["enemy-1"].durability, 2)
        self.assertEqual(result.state.combat_state.enemies["enemy-2"].durability, 4)
        self.assertEqual(result.state.combat_state.enemies["enemy-4"].durability, 11)

    def test_three_enemy_pressure_can_exhaust_counterattacks_on_second_turn(self) -> None:
        state = replace(make_state(), player_position=Position(1, 4))
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={
                    "enemy-4": create_enemy_combat_state(
                        enemy_id="enemy-4",
                        tier=SrsEnemyTier.TIER4,
                        position=Position(2, 4),
                    ),
                    "enemy-2": create_enemy_combat_state(
                        enemy_id="enemy-2",
                        tier=SrsEnemyTier.TIER2,
                        position=Position(1, 3),
                    ),
                    "enemy-1": create_enemy_combat_state(
                        enemy_id="enemy-1",
                        tier=SrsEnemyTier.TIER1,
                        position=Position(0, 4),
                    ),
                },
                phase=SrsCombatPhase.PLAYER_ATTACK,
                player_attack_target_id="enemy-4",
            ),
        )

        result = run_srs_commands(
            state,
            (
                SrsCommand(
                    command_type="COMBAT_STEP",
                    player_attack_action="ATTACK",
                    player_attack_weapon="PHASER",
                ),
                SrsCommand(
                    command_type="COMBAT_STEP",
                    enemy_reactions={
                        "enemy-1": "COUNTERATTACK",
                        "enemy-2": "COUNTERATTACK",
                        "enemy-4": "COUNTERATTACK",
                    },
                ),
                SrsCommand(command_type="COMBAT_STEP"),
                SrsCommand(
                    command_type="COMBAT_STEP",
                    player_attack_action="ATTACK",
                    player_attack_weapon="PHASER",
                ),
                SrsCommand(
                    command_type="COMBAT_STEP",
                    enemy_reactions={
                        "enemy-1": "COUNTERATTACK",
                        "enemy-2": "COUNTERATTACK",
                        "enemy-4": "COUNTERATTACK",
                    },
                ),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[1].payload["player_energy_after"], 3)
        self.assertEqual(result.events[4].payload["player_energy_after"], 1)
        self.assertEqual(
            [action["reaction"]["resolved_reaction"] for action in result.events[4].payload["enemy_actions"]],
            ["COUNTERATTACK", "COUNTERATTACK", "DEFEND"],
        )
        self.assertTrue(result.events[4].payload["enemy_actions"][2]["reaction"]["fallback_to_defend"])

    def test_enemy_action_moves_to_lowest_total_movement_cost_attack_cell(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))
        for position in (Position(1, 4), Position(2, 4)):
            state = replace_cell_terrain(state, position, SrsTerrainType.ASTEROID_FIELD)
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(0, 4),
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.ENEMY_ACTION,
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        enemy_action = result.events[0].payload["enemy_actions"][0]
        self.assertEqual(enemy_action["target_attackable_position"], [2, 3])
        self.assertEqual(enemy_action["planned_path"], [[0, 3], [1, 3], [2, 3]])
        self.assertEqual(result.state.combat_state.enemies["enemy-1"].position, Position(2, 3))

    def test_enemy_action_uses_deterministic_cell_order_for_equal_cost_targets(self) -> None:
        state = replace(make_state(), player_position=Position(6, 4))
        for position in (Position(1, 4), Position(2, 4), Position(3, 4), Position(4, 4)):
            state = replace_cell_terrain(state, position, SrsTerrainType.ASTEROID)
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(0, 4),
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.ENEMY_ACTION,
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP"),
            contracts=self.contracts,
        )

        enemy_action = result.events[0].payload["enemy_actions"][0]
        self.assertEqual(enemy_action["target_attackable_position"], [4, 3])
        self.assertEqual(
            enemy_action["planned_path"],
            [[0, 3], [1, 3], [2, 3], [3, 3], [4, 3]],
        )
        self.assertEqual(enemy_action["moved_path"], [[0, 3], [1, 3], [2, 3]])
        self.assertEqual(result.state.combat_state.enemies["enemy-1"].position, Position(2, 3))

    def test_enemy_attack_defend_halves_damage_and_rounds_up(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-2",
            tier=SrsEnemyTier.TIER2,
            position=state.player_position,
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-2": enemy},
                phase=SrsCombatPhase.ENEMY_ACTION,
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP", enemy_reactions={"enemy-2": "DEFEND"}),
            contracts=self.contracts,
        )

        enemy_action = result.events[0].payload["enemy_actions"][0]
        self.assertEqual(enemy_action["reaction"]["resolved_reaction"], "DEFEND")
        self.assertEqual(enemy_action["reaction"]["damage_to_player"], 4)
        self.assertEqual(result.state.combat_state.player.durability, 96)

    def test_enemy_attack_counterattack_uses_phaser_and_full_enemy_damage(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(2, 4),
        )
        state = replace(
            state,
            player_position=Position(1, 4),
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                phase=SrsCombatPhase.ENEMY_ACTION,
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP", enemy_reactions={"enemy-1": "COUNTERATTACK"}),
            contracts=self.contracts,
        )

        enemy_action = result.events[0].payload["enemy_actions"][0]
        self.assertEqual(enemy_action["reaction"]["resolved_reaction"], "COUNTERATTACK")
        self.assertEqual(enemy_action["reaction"]["counterattack_damage"], 1)
        self.assertEqual(enemy_action["reaction"]["damage_to_player"], 6)
        self.assertEqual(result.state.combat_state.player.durability, 94)
        self.assertEqual(result.state.combat_state.player.energy, 6)
        self.assertEqual(result.state.combat_state.enemies["enemy-1"].durability, 2)

    def test_enemy_attack_counterattack_falls_back_to_defend_when_energy_is_insufficient(self) -> None:
        state = make_state()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(2, 4),
        )
        combat_state = SrsCombatState(
            enemies={"enemy-1": enemy},
            phase=SrsCombatPhase.ENEMY_ACTION,
        )
        combat_state = replace(combat_state, player=replace(combat_state.player, energy=0))
        state = replace(
            state,
            player_position=Position(1, 4),
            combat_state=combat_state,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="COMBAT_STEP", enemy_reactions={"enemy-1": "COUNTERATTACK"}),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["enemy_actions"][0]["reaction"]["resolved_reaction"], "DEFEND")
        self.assertEqual(result.state.combat_state.player.energy, 1)
        self.assertEqual(result.state.combat_state.player.durability, 97)

    def test_warp_exit_rejected_while_enemy_presence_true(self) -> None:
        state = make_state(entry_edge=Direction.S)
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=state.player_position,
        )
        state = replace(
            state,
            combat_state=SrsCombatState(
                enemies={"enemy-1": enemy},
                player_attack_target_id="enemy-1",
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="WARP_EXIT", exit_direction=Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, WARP_EXIT_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_ENEMY_PRESENCE")
        self.assertEqual(result.state, state)
