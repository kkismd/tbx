from __future__ import annotations

import unittest
from dataclasses import replace

from experiments.galactic_exodus.srs.model import Direction, Position, SrsCell, SrsObjectType, SrsTerrainType
from experiments.galactic_exodus.srs.render import (
    from_display_position,
    render_known_map,
    render_known_map_spaced,
    render_row_for_internal_y,
    to_display_position,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object, reveal_positions, replace_cell_terrain


class SrsRenderTests(unittest.TestCase):
    def _row_for_internal_y(self, *, height: int, y: int) -> int:
        return render_row_for_internal_y(height=height, y=y)

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
        state = make_state()
        rows = [list(row) for row in state.actual_map.cells]
        cell = rows[1][1]
        rows[1][1] = SrsCell(
            terrain=cell.terrain,
            object_id=cell.object_id,
            actor_id=cell.actor_id,
            warp_flags=frozenset({Direction.N, Direction.E}),
        )
        state = replace(
            state,
            actual_map=replace(
                state.actual_map,
                cells=tuple(tuple(row) for row in rows),
            ),
        )
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
