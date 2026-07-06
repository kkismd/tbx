from __future__ import annotations

import unittest
from dataclasses import replace

from experiments.galactic_exodus.display_reference import expected_srs_display_snapshot, make_srs_display_snapshot_state
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SrsCell,
    SrsCombatState,
    SrsEnemyTier,
    SrsObjectType,
    SrsTerrainType,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.render import (
    from_display_position,
    render_display_map,
    render_known_map,
    render_known_map_spaced,
    render_row_for_internal_y,
    to_display_position,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object, reveal_positions, replace_cell_terrain


class SrsRenderTests(unittest.TestCase):
    def _row_for_internal_y(self, *, height: int, y: int) -> int:
        return render_row_for_internal_y(height=height, y=y)

    def _replace_cell(
        self,
        state,
        position: Position,
        *,
        terrain: SrsTerrainType | None = None,
        object_id: str | None = None,
        warp_flags: frozenset[Direction] | None = None,
    ):
        rows = [list(row) for row in state.actual_map.cells]
        current = state.actual_map.cell_at(position)
        rows[position.y][position.x] = SrsCell(
            terrain=current.terrain if terrain is None else terrain,
            object_id=current.object_id if object_id is None else object_id,
            actor_id=current.actor_id,
            warp_flags=current.warp_flags if warp_flags is None else warp_flags,
        )
        return replace(
            state,
            actual_map=replace(
                state.actual_map,
                cells=tuple(tuple(row) for row in rows),
            ),
        )

    def _display_row(self, rendered: str, display_y: int) -> str:
        return rendered.splitlines()[9 - display_y]

    def _display_symbols(self, rendered: str, display_y: int) -> list[str]:
        return self._display_row(rendered, display_y).split()[1:]

    def _display_cell(self, rendered: str, *, display_x: int, display_y: int) -> str:
        return self._display_symbols(rendered, display_y)[display_x - 1]

    def _with_enemy(self, state, *, enemy_id: str, position: Position):
        enemy = create_enemy_combat_state(
            enemy_id=enemy_id,
            tier=SrsEnemyTier.TIER2,
            position=position,
        )
        return replace(
            state,
            combat_state=SrsCombatState(
                enemies={enemy_id: enemy},
                player_attack_target_id=enemy_id,
            ),
        )

    def _build_baseline_snapshot_state(self):
        return make_srs_display_snapshot_state()

    def test_known_render_unknown_cells_are_question_marks(self) -> None:
        rendered = render_known_map(make_state())

        self.assertEqual(rendered.splitlines(), ["?" * 9] * 9)

    def test_known_render_does_not_leak_actual_terrain(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 4), SrsTerrainType.NEBULA)

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=4)][4], "?")
        self.assertNotIn("~", rendered)

    def test_known_render_player_overrides_cell_symbol(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 0)])

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=0)][4], "@")

    def test_known_render_object_symbols(self) -> None:
        state = reveal_positions(
            place_object(make_state(), Position(1, 1), SrsObjectType.STAR, "star-a"),
            [Position(1, 1), Position(2, 1), Position(3, 1), Position(4, 1)],
        )
        state = place_object(state, Position(2, 1), SrsObjectType.PLANET, "planet-a")
        state = place_object(state, Position(3, 1), SrsObjectType.STATION, "station-a")
        state = place_object(state, Position(4, 1), SrsObjectType.RESOURCE_CACHE, "resource-a")
        state = reveal_positions(state, [Position(1, 1), Position(2, 1), Position(3, 1), Position(4, 1)])

        rendered = render_known_map(state)

        row = rendered.splitlines()[self._row_for_internal_y(height=9, y=1)]
        self.assertEqual(row[1], "*")
        self.assertEqual(row[2], "o")
        self.assertEqual(row[3], "S")
        self.assertEqual(row[4], "R")

    def test_known_render_consumed_resource_cache_lowercase(self) -> None:
        state = place_object(make_state(), Position(2, 2), SrsObjectType.RESOURCE_CACHE, "resource-a")
        state = replace(
            state,
            objects={
                **state.objects,
                "resource-a": replace(state.objects["resource-a"], consumed=True),
            },
        )
        state = reveal_positions(state, [Position(2, 2)])

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=2)][2], "r")

    def test_known_render_consumed_salvage_lowercase(self) -> None:
        state = place_object(make_state(), Position(2, 2), SrsObjectType.SALVAGE, "salvage-a")
        state = replace(
            state,
            objects={
                **state.objects,
                "salvage-a": replace(state.objects["salvage-a"], consumed=True),
            },
        )
        state = reveal_positions(state, [Position(2, 2)])

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=2)][2], "s")

    def test_known_render_warp_symbols(self) -> None:
        state = self._replace_cell(make_state(), Position(1, 1), warp_flags=frozenset({Direction.N, Direction.E}))
        state = reveal_positions(state, [Position(1, 1)])

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=1)][1], "+")

    def test_render_top_row_is_internal_north_row(self) -> None:
        state = reveal_positions(make_state(), [Position(3, 8)])

        rendered = render_known_map(state)
        lines = rendered.splitlines()

        self.assertEqual(lines[0][3], ".")
        self.assertEqual(lines[8][3], "?")

    def test_render_bottom_row_is_internal_south_row(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 0)])

        rendered = render_known_map(state)
        lines = rendered.splitlines()

        self.assertEqual(lines[0][4], "?")
        self.assertEqual(lines[8][4], "@")

    def test_display_position_converts_internal_zero_origin_to_display_one_origin(self) -> None:
        self.assertEqual(to_display_position(Position(0, 0)), (1, 1))
        self.assertEqual(to_display_position(Position(8, 8)), (9, 9))

    def test_from_display_position_converts_display_one_origin_to_internal_zero_origin(self) -> None:
        self.assertEqual(from_display_position(1, 1), Position(0, 0))
        self.assertEqual(from_display_position(9, 9), Position(8, 8))

    def test_warp_points_have_expected_display_coordinates(self) -> None:
        self.assertEqual(to_display_position(Position(4, 8)), (5, 9))
        self.assertEqual(to_display_position(Position(8, 4)), (9, 5))
        self.assertEqual(to_display_position(Position(4, 0)), (5, 1))
        self.assertEqual(to_display_position(Position(0, 4)), (1, 5))

    def test_known_render_row_widths_are_stable(self) -> None:
        state = reveal_positions(make_state(), [Position(x, y) for y in range(9) for x in range(9)])

        rendered = render_known_map(state)

        self.assertEqual([len(row) for row in rendered.splitlines()], [9] * 9)

    def test_spaced_known_render_inserts_single_spaces_between_cells(self) -> None:
        state = reveal_positions(make_state(), [Position(x, 0) for x in range(9)])

        rendered = render_known_map_spaced(state)

        lines = rendered.splitlines()
        self.assertEqual(lines[0], "? ? ? ? ? ? ? ? ?")
        self.assertEqual(lines[8], ". . . . @ . . . .")
        self.assertEqual([len(row) for row in lines], [17] * 9)

    def test_display_map_shape_uses_axis_labels_and_blank_separator(self) -> None:
        rendered = render_display_map(make_state())
        lines = rendered.splitlines()

        self.assertEqual(len(lines), 11)
        self.assertEqual([line.split()[0] for line in lines[:9]], [str(y) for y in range(9, 0, -1)])
        self.assertEqual(lines[9], "")
        self.assertEqual(lines[10], "    1 2 3 4 5 6 7 8 9")

    def test_display_map_uses_display_coordinate_row_order(self) -> None:
        state = reveal_positions(make_state(), [Position(3, 8), Position(4, 0)])

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=4, display_y=9), ".")
        self.assertEqual(self._display_cell(rendered, display_x=5, display_y=1), "@")
        self.assertEqual(to_display_position(Position(3, 8)), (4, 9))
        self.assertEqual(from_display_position(5, 1), Position(4, 0))

    def test_display_map_player_uses_at_mark_at_display_coordinate(self) -> None:
        state = replace(make_state(), player_position=Position(6, 3))
        state = reveal_positions(state, [Position(6, 3)])

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=7, display_y=4), "@")

    def test_display_map_enemy_overlay_uses_e_for_known_cell(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 4)])
        state = self._with_enemy(state, enemy_id="enemy-1", position=Position(4, 4))

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=5, display_y=5), "e")

    def test_display_map_hides_enemy_on_unknown_cell(self) -> None:
        state = self._with_enemy(make_state(), enemy_id="enemy-1", position=Position(4, 4))

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=5, display_y=5), "?")

    def test_display_map_overlay_priority_player_over_enemy(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 4)])
        state = replace(state, player_position=Position(4, 4))
        state = self._with_enemy(state, enemy_id="enemy-1", position=Position(4, 4))

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=5, display_y=5), "@")

    def test_display_map_overlay_priority_enemy_over_object_warp_and_terrain(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 4), SrsTerrainType.ASTEROID)
        state = place_object(state, Position(4, 4), SrsObjectType.SALVAGE, "salvage-a")
        state = self._replace_cell(state, Position(4, 4), warp_flags=frozenset({Direction.N}))
        state = reveal_positions(state, [Position(4, 4)])
        state = self._with_enemy(state, enemy_id="enemy-1", position=Position(4, 4))

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=5, display_y=5), "e")

    def test_display_map_object_symbols_follow_contract(self) -> None:
        positions = [
            (Position(0, 4), SrsObjectType.SALVAGE, "$", False),
            (Position(1, 4), SrsObjectType.SALVAGE, "s", True),
            (Position(2, 4), SrsObjectType.RESOURCE_CACHE, "R", False),
            (Position(3, 4), SrsObjectType.RESOURCE_CACHE, "r", True),
            (Position(4, 4), SrsObjectType.STATION, "S", False),
            (Position(5, 4), SrsObjectType.STAR, "*", False),
            (Position(6, 4), SrsObjectType.PLANET, "o", False),
        ]
        state = make_state()
        for index, (position, object_type, _, consumed) in enumerate(positions, start=1):
            object_id = f"object-{index}"
            state = place_object(state, position, object_type, object_id)
            if consumed:
                state = replace(
                    state,
                    objects={
                        **state.objects,
                        object_id: replace(state.objects[object_id], consumed=True),
                    },
                )
        state = reveal_positions(state, [position for position, *_ in positions])

        rendered = render_display_map(state)

        self.assertEqual(
            self._display_symbols(rendered, 5)[:7],
            ["$", "s", "R", "r", "S", "*", "o"],
        )

    def test_display_map_warp_symbols_follow_contract(self) -> None:
        state = make_state()
        cases = [
            (Position(0, 4), frozenset({Direction.N}), "^"),
            (Position(1, 4), frozenset({Direction.E}), ">"),
            (Position(2, 4), frozenset({Direction.S}), "v"),
            (Position(3, 4), frozenset({Direction.W}), "<"),
            (Position(4, 4), frozenset({Direction.N, Direction.E}), "+"),
        ]
        for position, warp_flags, _ in cases:
            state = self._replace_cell(state, position, warp_flags=warp_flags)
        state = reveal_positions(state, [position for position, *_ in cases])

        rendered = render_display_map(state)

        self.assertEqual(
            self._display_symbols(rendered, 5)[:5],
            ["^", ">", "v", "<", "+"],
        )

    def test_display_map_impassable_terrain_uses_hash(self) -> None:
        state = self._replace_cell(make_state(), Position(4, 4), terrain=SrsTerrainType.ASTEROID, warp_flags=frozenset())
        state = self._replace_cell(state, Position(5, 4), terrain=SrsTerrainType.RIFT_BARRIER, warp_flags=frozenset())
        state = reveal_positions(state, [Position(4, 4), Position(5, 4)])

        rendered = render_display_map(state)

        self.assertEqual(self._display_symbols(rendered, 5)[4:6], ["#", "#"])

    def test_display_map_keeps_unknown_cell_secrecy_for_terrain_object_enemy_and_warp(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 4), SrsTerrainType.ASTEROID)
        state = place_object(state, Position(4, 4), SrsObjectType.STAR, "star-a")
        state = self._replace_cell(state, Position(4, 4), warp_flags=frozenset({Direction.N}))
        state = self._with_enemy(state, enemy_id="enemy-1", position=Position(4, 4))

        rendered = render_display_map(state)

        self.assertEqual(self._display_cell(rendered, display_x=5, display_y=5), "?")
        self.assertNotIn("*", rendered)
        self.assertNotIn("^", rendered)
        self.assertNotIn("e", rendered)

    def test_display_map_snapshot_matches_issue_baseline_shape(self) -> None:
        rendered = render_display_map(self._build_baseline_snapshot_state())

        self.assertEqual(rendered, expected_srs_display_snapshot())
