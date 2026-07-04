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
)
from experiments.galactic_exodus.srs.log import COMBAT_TRANSITIONED, WARP_EXIT_REJECTED
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SrsCombatPhase,
    SrsCombatState,
    SrsCommand,
    SrsEnemyTier,
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
