from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.encounter import (
    BASE_ENCOUNTER_CHANCE_PER_SRS_TURN,
    ENCOUNTERS_PER_LRS_STEP,
    ENEMY_SALVAGE_DROP_CHANCES,
    EXPECTED_SRS_TURNS,
    EncounterRollDisposition,
    actual_encounter_chance,
    encounter_roll_disposition,
    encounter_composition_options,
    encounter_group_budget_range,
    enemy_salvage_drop_chance,
    enemy_group_cost,
    resolve_enemy_salvage_drop,
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

    def test_enemy_salvage_drop_chance_is_tier_monotonic(self) -> None:
        ordered_tiers = (
            SrsEnemyTier.TIER1,
            SrsEnemyTier.TIER2,
            SrsEnemyTier.TIER3,
            SrsEnemyTier.TIER4,
        )

        self.assertEqual(
            [enemy_salvage_drop_chance(tier) for tier in ordered_tiers],
            [ENEMY_SALVAGE_DROP_CHANCES[tier] for tier in ordered_tiers],
        )
        self.assertLessEqual(enemy_salvage_drop_chance(SrsEnemyTier.TIER1), enemy_salvage_drop_chance(SrsEnemyTier.TIER2))
        self.assertLessEqual(enemy_salvage_drop_chance(SrsEnemyTier.TIER2), enemy_salvage_drop_chance(SrsEnemyTier.TIER3))
        self.assertLessEqual(enemy_salvage_drop_chance(SrsEnemyTier.TIER3), enemy_salvage_drop_chance(SrsEnemyTier.TIER4))

    def test_enemy_salvage_drop_roll_success(self) -> None:
        resolved, payload = resolve_enemy_salvage_drop(tier=SrsEnemyTier.TIER2, roll=0.34)

        self.assertTrue(resolved)
        self.assertEqual(payload["enemy_tier"], "TIER2")
        self.assertEqual(payload["salvage_drop_chance"], 0.35)
        self.assertEqual(payload["salvage_drop_roll"], 0.34)
        self.assertTrue(payload["drop_salvage"])

    def test_enemy_salvage_drop_roll_failure(self) -> None:
        resolved, payload = resolve_enemy_salvage_drop(tier=SrsEnemyTier.TIER2, roll=0.36)

        self.assertFalse(resolved)
        self.assertEqual(payload["salvage_drop_chance"], 0.35)
        self.assertEqual(payload["salvage_drop_roll"], 0.36)
        self.assertFalse(payload["drop_salvage"])

    def test_enemy_salvage_drop_roll_boundary_equal_chance_is_failure(self) -> None:
        resolved, payload = resolve_enemy_salvage_drop(
            tier=SrsEnemyTier.TIER3,
            roll=enemy_salvage_drop_chance(SrsEnemyTier.TIER3),
        )

        self.assertFalse(resolved)
        self.assertFalse(payload["drop_salvage"])

    def test_spawn_candidates_use_all_passable_warp_points_outside_player_neighbor_ring(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))

        self.assertEqual(
            spawn_candidate_points(state),
            (
                Position(0, 0),
                Position(1, 0),
                Position(2, 0),
                Position(3, 0),
                Position(4, 0),
                Position(5, 0),
                Position(6, 0),
                Position(7, 0),
                Position(8, 0),
                Position(0, 1),
                Position(8, 1),
                Position(0, 2),
                Position(8, 2),
                Position(0, 3),
                Position(8, 3),
                Position(0, 4),
                Position(8, 4),
                Position(0, 5),
                Position(8, 5),
                Position(0, 6),
                Position(8, 6),
                Position(0, 7),
                Position(8, 7),
                Position(0, 8),
                Position(1, 8),
                Position(2, 8),
                Position(3, 8),
                Position(4, 8),
                Position(5, 8),
                Position(6, 8),
                Position(7, 8),
                Position(8, 8),
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
                Position(1, 0),
                Position(2, 0),
                Position(3, 0),
                Position(4, 0),
                Position(5, 0),
                Position(6, 0),
                Position(7, 0),
                Position(8, 0),
                Position(8, 1),
                Position(8, 2),
                Position(8, 3),
                Position(8, 4),
                Position(8, 5),
                Position(8, 6),
                Position(8, 7),
            ),
        )

    def test_spawn_keeps_sorted_enemy_tiers_when_candidates_are_sufficient(self) -> None:
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
                ("enemy-1", SrsEnemyTier.TIER1, Position(1, 0)),
                ("enemy-2", SrsEnemyTier.TIER1, Position(2, 0)),
                ("enemy-3", SrsEnemyTier.TIER2, Position(3, 0)),
            ],
        )

    def test_fixture_accepts_fixed_encounter_composition_input(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_spawn_cap_9x9.json", contracts=self.contracts)

        self.assertEqual(tuple(result.final_state.combat_state.enemies), ("enemy-1", "enemy-2", "enemy-3"))
        self.assertEqual(
            result.summary["combat_enemy_positions"],
            {
                "enemy-1": [1, 0],
                "enemy-2": [2, 0],
                "enemy-3": [3, 0],
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
        spawned_enemy = result.log.events[1].payload["spawned_enemies"][0]
        self.assertEqual(spawned_enemy["enemy_id"], "enemy-1")
        self.assertEqual(spawned_enemy["enemy_tier"], "TIER2")
        self.assertEqual(spawned_enemy["salvage_drop_chance"], 0.35)
        self.assertEqual(spawned_enemy["salvage_drop_roll"], 0.4346763120373218)
        self.assertFalse(spawned_enemy["drop_salvage"])
        self.assertEqual(
            result.summary["combat_enemy_salvage_drops"],
            {
                "enemy-1": {
                    "enemy_tier": "TIER2",
                    "salvage_drop_chance": 0.35,
                    "salvage_drop_roll": 0.4346763120373218,
                    "drop_salvage": False,
                }
            },
        )

    def test_wait_fixture_suppresses_encounter_when_base_docked(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_wait_base_docked_9x9.json", contracts=self.contracts)

        self.assertEqual([event.event_type for event in result.log.events], ["WAIT_ACCEPTED"])
        self.assertIsNone(result.final_state.combat_state)
