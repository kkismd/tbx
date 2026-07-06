from __future__ import annotations

import unittest
from dataclasses import replace

from experiments.galactic_exodus.hud import CompactHudContext, render_compact_hud
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorType,
    SrsCell,
    SrsCombatPhase,
    SrsCombatState,
    SrsEnemyTier,
    SrsObjectType,
    SrsPlayerCombatState,
    SrsTerrainType,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state as make_srs_state
from experiments.galactic_exodus.srs.test_engine_movement import place_object, reveal_positions, replace_cell_terrain
from experiments.galactic_exodus.test_engine import filled_cells, make_actual_map, make_state as make_lrs_state


def _all_positions(state: object) -> list[Position]:
    width = state.actual_map.width
    height = state.actual_map.height
    return [Position(x, y) for y in range(height) for x in range(width)]


def _set_warp_flags(state, position: Position, warp_flags: frozenset[Direction]):
    rows = [list(row) for row in state.actual_map.cells]
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=current.terrain,
        object_id=current.object_id,
        actor_id=current.actor_id,
        warp_flags=warp_flags,
    )
    return replace(
        state,
        actual_map=replace(
            state.actual_map,
            cells=tuple(tuple(row) for row in rows),
        ),
    )


class CompactHudTests(unittest.TestCase):
    def test_minimal_context_shape(self) -> None:
        rendered = render_compact_hud(CompactHudContext())

        self.assertEqual(
            rendered,
            "\n".join(
                [
                    "SECTOR  LRS=-      TYPE=-     SRS=-      SENSOR=-",
                    "TURN    LRS=-      SRS=-      COST=-",
                    "FUEL    -          STATUS=-",
                    "PLAYER  DUR=-      EN=-      TORP=-      SALVAGE=-",
                    "COMBAT  none",
                    "WARP    -",
                    "REWARD  -",
                    "LAST    -",
                ]
            ),
        )
        self.assertEqual(len(rendered.splitlines()), 8)

    def test_lrs_only_hud_uses_lrs_fields(self) -> None:
        cells = filled_cells(".")
        cells[(3, 5)] = "R"
        lrs_state = make_lrs_state(
            actual_map=make_actual_map(cells=cells),
            player_position=(3, 5),
            remaining_fuel=6,
            known_cells={(3, 5): "R", (8, 8): "H"},
            turn_count=18,
        )

        rendered = render_compact_hud(CompactHudContext(lrs_state=lrs_state))

        self.assertIn("SECTOR  LRS=(3,5)  TYPE=RESOURCE", rendered)
        self.assertIn("TURN    LRS=18", rendered)
        self.assertIn("FUEL    6/16", rendered)
        self.assertIn("SRS=-", rendered)
        self.assertIn("STATUS=EXPLORING", rendered)

    def test_srs_display_coordinate_only(self) -> None:
        state = replace(make_srs_state(), player_position=Position(6, 3))
        state = reveal_positions(state, _all_positions(state))

        rendered = render_compact_hud(CompactHudContext(srs_state=state))

        self.assertIn("SRS=(7,4)", rendered)
        self.assertNotIn("Position(x=6, y=3)", rendered)
        self.assertNotIn("internal=", rendered)

    def test_nebula_sensor_range_is_3x3(self) -> None:
        nebula_state = replace(make_srs_state(sector_type=SectorType.NEBULA), player_position=Position(4, 4))
        nebula_state = reveal_positions(nebula_state, _all_positions(nebula_state))
        normal_state = replace(make_srs_state(sector_type=SectorType.NORMAL), player_position=Position(4, 4))
        normal_state = reveal_positions(normal_state, _all_positions(normal_state))

        self.assertIn("SENSOR=3x3", render_compact_hud(CompactHudContext(srs_state=nebula_state)))
        self.assertIn("SENSOR=5x5", render_compact_hud(CompactHudContext(srs_state=normal_state)))

    def test_player_stats_and_combat_target_summary(self) -> None:
        player_state = SrsPlayerCombatState(
            durability=100,
            durability_capacity=100,
            energy=6,
            energy_capacity=6,
            photon_torpedo_ammo=6,
            photon_torpedo_ammo_capacity=6,
            salvage=1,
        )
        enemy_1 = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER2,
            position=Position(4, 4),
        )
        enemy_2 = create_enemy_combat_state(
            enemy_id="enemy-2",
            tier=SrsEnemyTier.TIER1,
            position=Position(3, 3),
        )
        state = replace(
            make_srs_state(
                sector_type=SectorType.RIFT,
                blocked_edges=frozenset({Direction.W}),
                fuel=6,
                max_fuel=9,
            ),
            player_position=Position(6, 3),
            player_state=player_state,
            combat_state=SrsCombatState(
                player=player_state,
                enemies={"enemy-1": enemy_1, "enemy-2": enemy_2},
                phase=SrsCombatPhase.PLAYER_MOVEMENT,
                player_attack_target_id="enemy-1",
            ),
        )
        state = reveal_positions(state, _all_positions(state))

        rendered = render_compact_hud(
            CompactHudContext(
                srs_state=state,
                last_event_summary="MOVE_ACCEPTED route=E,E",
                cost_mode="TURN_ONLY",
            )
        )

        self.assertIn("PLAYER  DUR=100/100  EN=6/6", rendered)
        self.assertIn("TORP=6/6", rendered)
        self.assertIn("SALVAGE=1", rendered)
        self.assertIn(
            "COMBAT  PHASE=PLAYER_MOVEMENT  ENEMY=enemy-1 TIER2 hp=5 at SRS=(5,5)",
            rendered,
        )
        self.assertIn("LAST    MOVE_ACCEPTED route=E,E", rendered)

    def test_warp_summary_uses_player_cell_flags_in_nesw_order(self) -> None:
        state = replace(make_srs_state(), player_position=Position(4, 0))
        state = _set_warp_flags(state, Position(4, 0), frozenset({Direction.S, Direction.N, Direction.E}))
        state = reveal_positions(state, _all_positions(state))

        rendered = render_compact_hud(CompactHudContext(srs_state=state))

        self.assertIn("WARP    N,E,S available at SRS=(5,1)", rendered)

    def test_warp_summary_uses_visible_rift_barrier_when_no_warp(self) -> None:
        state = replace(make_srs_state(), player_position=Position(4, 4))
        state = replace_cell_terrain(state, Position(4, 5), terrain=SrsTerrainType.RIFT_BARRIER)
        state = reveal_positions(state, _all_positions(state))

        rendered = render_compact_hud(CompactHudContext(srs_state=state))

        self.assertIn("WARP    N blocked by RIFT_BARRIER", rendered)

    def test_reward_summary_prefers_nearest_unconsumed_reward(self) -> None:
        state = replace(make_srs_state(), player_position=Position(4, 3))
        state = place_object(state, Position(4, 2), SrsObjectType.SALVAGE, "salvage-a")
        state = place_object(state, Position(5, 3), SrsObjectType.RESOURCE_CACHE, "cache-a")
        state = place_object(state, Position(3, 3), SrsObjectType.STAR, "star-a")
        state = replace(
            state,
            objects={
                **state.objects,
                "cache-a": replace(state.objects["cache-a"], consumed=True),
            },
        )
        state = reveal_positions(state, _all_positions(state))

        rendered = render_compact_hud(CompactHudContext(srs_state=state))

        self.assertIn("REWARD  SALVAGE detected at SRS=(5,3)", rendered)
        self.assertNotIn("CACHE detected", rendered)
        self.assertNotIn("STAR", rendered)

    def test_normal_hud_excludes_debug_strings(self) -> None:
        state = replace(make_srs_state(), player_position=Position(4, 3))
        state = reveal_positions(state, _all_positions(state))

        rendered = render_compact_hud(
            CompactHudContext(
                srs_state=state,
                last_event_summary="MOVE_ACCEPTED route=E,E center=(5,4)",
            )
        )

        for forbidden in ("internal=", "raw", "roll", "consumed_object_ids", "activated_object_ids", "before", "after"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, rendered)


if __name__ == "__main__":
    unittest.main()
