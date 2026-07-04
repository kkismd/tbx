from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import apply_srs_command
from experiments.galactic_exodus.srs.log import COMBAT_TRANSITIONED, WARP_EXIT_REJECTED
from experiments.galactic_exodus.srs.model import (
    Direction,
    SrsCombatPhase,
    SrsCombatState,
    SrsCommand,
    SrsEnemyTier,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state


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
