from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.encounter import (
    spawn_candidate_points,
    spawn_enemies_for_encounter,
)
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsEnemyTier,
)
from experiments.galactic_exodus.srs.run_fixture import FIXTURES_DIR, run_fixture
from experiments.galactic_exodus.srs.test_engine_movement import make_state


REPO_ROOT = Path(__file__).resolve().parents[3]


def chebyshev_distance(a: Position, b: Position) -> int:
    return max(abs(a.x - b.x), abs(a.y - b.y))


def candidate_summary(state) -> dict[str, int]:
    candidates = spawn_candidate_points(state)
    corners = {
        Position(0, 0),
        Position(state.actual_map.width - 1, 0),
        Position(0, state.actual_map.height - 1),
        Position(state.actual_map.width - 1, state.actual_map.height - 1),
    }
    return {
        "count": len(candidates),
        "north": sum(position.y == 0 for position in candidates),
        "south": sum(position.y == state.actual_map.height - 1 for position in candidates),
        "west": sum(position.x == 0 and position not in corners for position in candidates),
        "east": sum(
            position.x == state.actual_map.width - 1 and position not in corners
            for position in candidates
        ),
        "corners": sum(position in corners for position in candidates),
        "min_distance": min(chebyshev_distance(state.player_position, position) for position in candidates),
        "max_distance": max(chebyshev_distance(state.player_position, position) for position in candidates),
    }


class SrsEncounterBalanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_all_floor_center_player_candidate_distribution_after_warp_flag_expansion(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))

        self.assertEqual(
            candidate_summary(state),
            {
                "count": 32,
                "north": 9,
                "south": 9,
                "west": 7,
                "east": 7,
                "corners": 4,
                "min_distance": 4,
                "max_distance": 4,
            },
        )

    def test_edge_near_player_excludes_neighbor_ring_but_keeps_other_edge_candidates(self) -> None:
        state = replace(make_state(), player_position=Position(7, 4))
        candidates = spawn_candidate_points(state)

        self.assertLess(len(candidates), len(spawn_candidate_points(replace(make_state(), player_position=Position(4, 4)))))
        self.assertNotIn(Position(8, 3), candidates)
        self.assertNotIn(Position(8, 4), candidates)
        self.assertNotIn(Position(8, 5), candidates)
        self.assertIn(Position(0, 0), candidates)
        self.assertIn(Position(4, 0), candidates)
        self.assertIn(Position(8, 0), candidates)
        self.assertIn(Position(0, 8), candidates)
        self.assertGreaterEqual(
            min(chebyshev_distance(state.player_position, position) for position in candidates),
            2,
        )

    def test_rift_blocked_edges_remove_blocked_edge_candidates(self) -> None:
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
            candidate_summary(state),
            {
                "count": 15,
                "north": 8,
                "south": 0,
                "west": 0,
                "east": 7,
                "corners": 1,
                "min_distance": 4,
                "max_distance": 4,
            },
        )

    def test_fixed_composition_spawn_positions_keep_initial_distance_pressure(self) -> None:
        state = replace(make_state(), player_position=Position(4, 4))

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
                ("enemy-1", SrsEnemyTier.TIER1, Position(0, 0)),
                ("enemy-2", SrsEnemyTier.TIER1, Position(1, 0)),
                ("enemy-3", SrsEnemyTier.TIER2, Position(2, 0)),
            ],
        )
        self.assertEqual(
            [chebyshev_distance(state.player_position, enemy.position) for enemy in enemies],
            [4, 4, 4],
        )

    def test_combat_encounter_spawn_cap_fixture_documents_current_spawn_positions(self) -> None:
        result = run_fixture(FIXTURES_DIR / "combat_encounter_spawn_cap_9x9.json", contracts=self.contracts)
        player_position = result.final_state.player_position
        enemies = result.final_state.combat_state.enemies

        self.assertEqual(len(enemies), 3)
        self.assertEqual(
            result.summary["combat_enemy_positions"],
            {
                "enemy-1": [1, 0],
                "enemy-2": [2, 0],
                "enemy-3": [3, 0],
            },
        )
        self.assertEqual(
            {
                enemy_id: chebyshev_distance(player_position, enemy.position)
                for enemy_id, enemy in enemies.items()
            },
            {
                "enemy-1": 4,
                "enemy-2": 4,
                "enemy-3": 4,
            },
        )
