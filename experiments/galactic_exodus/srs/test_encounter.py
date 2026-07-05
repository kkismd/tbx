from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.encounter import (
    BASE_ENCOUNTER_CHANCE_PER_SRS_TURN,
    ENCOUNTERS_PER_LRS_STEP,
    EXPECTED_SRS_TURNS,
    EncounterRollDisposition,
    actual_encounter_chance,
    encounter_roll_disposition,
    encounter_composition_options,
    encounter_group_budget_range,
    enemy_group_cost,
    spawn_candidate_points,
    spawn_enemies_for_encounter,
    terrain_encounter_modifier,
)
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsCombatState,
    SrsEnemyTier,
    SrsTerrainType,
)
from experiments.galactic_exodus.srs.run_fixture import FIXTURES_DIR, run_fixture
from experiments.galactic_exodus.srs.test_engine_movement import make_state, replace_cell_terrain


REPO_ROOT = Path(__file__).resolve().parents[3]


class SrsEncounterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_group_costs_and_budget_ranges_use_issue_1201_fixed_values(self) -> None:
        self.assertEqual(enemy_group_cost(SrsEnemyTier.TIER1), 1)
        self.assertEqual(enemy_group_cost(SrsEnemyTier.TIER2), 2)
        self.assertEqual(enemy_group_cost(SrsEnemyTier.TIER3), 3)
        self.assertEqual(enemy_group_cost(SrsEnemyTier.TIER4), 5)
        self.assertEqual(encounter_group_budget_range(0), (1, 1))
        self.assertEqual(encounter_group_budget_range(1), (1, 2))
        self.assertEqual(encounter_group_budget_range(2), (2, 3))
        self.assertEqual(encounter_group_budget_range(3), (3, 4))
        self.assertEqual(encounter_group_budget_range(4), (4, 5))

    def test_issue_1202_fixed_encounter_values_and_nebula_modifier_are_used(self) -> None:
        nebula_state = replace(make_state(), player_position=Position(4, 4))
        nebula_state = replace_cell_terrain(nebula_state, Position(4, 4), SrsTerrainType.NEBULA)

        self.assertEqual(EXPECTED_SRS_TURNS, 4)
        self.assertEqual(ENCOUNTERS_PER_LRS_STEP, 0.75)
        self.assertEqual(BASE_ENCOUNTER_CHANCE_PER_SRS_TURN, 0.18)
        self.assertEqual(terrain_encounter_modifier(SrsTerrainType.NEBULA), 0.7)
        self.assertAlmostEqual(actual_encounter_chance(nebula_state), 0.126)

    def test_composition_table_uses_issue_1201_fixed_values(self) -> None:
        danger4 = encounter_composition_options(4)

        self.assertEqual(
            [(option.weight_percent, tuple(option.tiers)) for option in danger4],
            [
                (40, (SrsEnemyTier.TIER3, SrsEnemyTier.TIER1)),
                (25, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER2)),
                (20, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER1, SrsEnemyTier.TIER1)),
                (10, (SrsEnemyTier.TIER3, SrsEnemyTier.TIER2)),
                (5, (SrsEnemyTier.TIER4,)),
            ],
        )

    def test_spawn_candidates_use_passable_warp_points_outside_player_neighbor_ring(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))

        self.assertEqual(
            spawn_candidate_points(state),
            (
                Position(4, 0),
                Position(0, 4),
                Position(8, 4),
                Position(4, 8),
            ),
        )

    def test_spawn_candidates_exclude_player_cell_and_blocked_warp_points(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="rift-9201",
            sector_type=SectorType.RIFT,
            sector_seed=9201,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N, Direction.W}),
        )
        state = make_state(
            sector_type=descriptor.sector_type,
            sector_seed=descriptor.sector_seed,
            entry_edge=descriptor.entry_edge,
            blocked_edges=descriptor.blocked_edges,
        )
        state = replace(state, descriptor=descriptor, player_position=Position(4, 4))

        self.assertEqual(
            spawn_candidate_points(state),
            (
                Position(4, 0),
                Position(8, 4),
            ),
        )

    def test_spawn_cap_keeps_strongest_enemies_then_sorts_result_ascending(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="rift-9202",
            sector_type=SectorType.RIFT,
            sector_seed=9202,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N, Direction.W}),
        )
        state = make_state(
            sector_type=descriptor.sector_type,
            sector_seed=descriptor.sector_seed,
            entry_edge=descriptor.entry_edge,
            blocked_edges=descriptor.blocked_edges,
        )
        state = replace(state, descriptor=descriptor, player_position=Position(4, 4))

        enemies = spawn_enemies_for_encounter(
            state,
            danger_score=4,
            composition=(
                SrsEnemyTier.TIER2,
                SrsEnemyTier.TIER1,
                SrsEnemyTier.TIER1,
            ),
        )

        self.assertEqual(
            [(enemy.enemy_id, enemy.tier, enemy.position) for enemy in enemies],
            [
                ("enemy-1", SrsEnemyTier.TIER1, Position(4, 0)),
                ("enemy-2", SrsEnemyTier.TIER2, Position(8, 4)),
            ],
        )

    def test_fixture_accepts_fixed_encounter_composition_input(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_spawn_cap_9x9.json", contracts=self.contracts)

        self.assertEqual(tuple(result.final_state.combat_state.enemies), ("enemy-1", "enemy-2"))
        self.assertEqual(
            result.summary["combat_enemy_positions"],
            {
                "enemy-1": [4, 0],
                "enemy-2": [8, 4],
            },
        )

    def test_encounter_roll_is_suppressed_while_enemy_presence_is_active(self) -> None:
        enemy = spawn_enemies_for_encounter(
            replace(make_state(), player_position=Position(4, 4)),
            danger_score=0,
            composition=(SrsEnemyTier.TIER1,),
        )[0]
        previous_state = replace(
            make_state(),
            combat_state=SrsCombatState(enemies={enemy.enemy_id: enemy}),
        )
        next_state = replace(previous_state, srs_turn=1)

        disposition = encounter_roll_disposition(previous_state, command_type="WAIT", next_state=next_state)

        self.assertEqual(disposition, EncounterRollDisposition.SKIPPED_ENEMY_PRESENCE)

    def test_movement_turn_without_enemies_requires_encounter_roll(self) -> None:
        previous_state = make_state()
        next_state = replace(previous_state, srs_turn=1, player_position=Position(4, 1))

        disposition = encounter_roll_disposition(previous_state, command_type="MOVE_ROUTE", next_state=next_state)

        self.assertEqual(disposition, EncounterRollDisposition.REQUIRED)

    def test_wait_fixture_can_trigger_nebula_modified_encounter(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_wait_nebula_9x9.json", contracts=self.contracts)

        self.assertEqual(result.final_state.srs_turn, 1)
        self.assertTrue(result.final_state.combat_state.enemy_presence)
        self.assertEqual(result.log.events[0].event_type, "WAIT_ACCEPTED")
        self.assertEqual(result.log.events[1].event_type, "ENCOUNTER_ROLLED")
        self.assertAlmostEqual(result.log.events[1].payload["actual_encounter_chance"], 0.126)

    def test_wait_fixture_suppresses_encounter_when_base_docked(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_wait_base_docked_9x9.json", contracts=self.contracts)

        self.assertEqual([event.event_type for event in result.log.events], ["WAIT_ACCEPTED"])
        self.assertIsNone(result.final_state.combat_state)
