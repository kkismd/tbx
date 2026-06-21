from __future__ import annotations

import unittest
from dataclasses import replace

from experiments.galactic_exodus.srs.model import Direction, Position, SrsCell, SrsObjectType, SrsTerrainType
from experiments.galactic_exodus.srs.render import render_known_map
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object, reveal_positions, replace_cell_terrain


class SrsRenderTests(unittest.TestCase):
    def test_known_render_unknown_cells_are_question_marks(self) -> None:
        rendered = render_known_map(make_state())

        self.assertEqual(rendered.splitlines(), ["?" * 9] * 9)

    def test_known_render_does_not_leak_actual_terrain(self) -> None:
        state = replace_cell_terrain(make_state(), Position(4, 4), SrsTerrainType.NEBULA)

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[4][4], "?")
        self.assertNotIn("~", rendered)

    def test_known_render_player_overrides_cell_symbol(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 8)])

        rendered = render_known_map(state)

        self.assertEqual(rendered.splitlines()[8][4], "@")

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

        row = rendered.splitlines()[1]
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

        self.assertEqual(rendered.splitlines()[2][2], "r")

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

        self.assertEqual(rendered.splitlines()[2][2], "s")

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

        self.assertEqual(rendered.splitlines()[1][1], "+")

    def test_known_render_row_widths_are_stable(self) -> None:
        state = reveal_positions(make_state(), [Position(x, y) for y in range(9) for x in range(9)])

        rendered = render_known_map(state)

        self.assertEqual([len(row) for row in rendered.splitlines()], [9] * 9)
