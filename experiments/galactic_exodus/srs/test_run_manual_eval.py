from __future__ import annotations

import unittest
from dataclasses import replace

from experiments.galactic_exodus.srs.model import Direction, Position, SrsObjectType, SrsTerrainType
from experiments.galactic_exodus.srs.run_manual_eval import _player_cell_text, _render_known_map_spaced_for_manual_eval
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object, reveal_positions, replace_cell_terrain


class SrsRunManualEvalTests(unittest.TestCase):
    def test_manual_eval_render_keeps_player_on_floor_cell(self) -> None:
        state = replace(make_state(), player_position=Position(4, 7))
        state = reveal_positions(state, [Position(x, 7) for x in range(9)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[7], ". . . . @ . . . .")

    def test_manual_eval_render_shows_player_beside_warp_symbol(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 8)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[8], "? ? ? ? v@? ? ? ?")

    def test_manual_eval_render_shows_player_beside_salvage_symbol(self) -> None:
        state = place_object(make_state(), Position(4, 6), SrsObjectType.SALVAGE, "salvage-a")
        state = replace(state, player_position=Position(4, 6))
        state = replace(
            state,
            objects={
                **state.objects,
                "salvage-a": replace(state.objects["salvage-a"], consumed=True),
            },
        )
        state = reveal_positions(state, [Position(4, 6)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[6], "? ? ? ? s@? ? ? ?")

    def test_manual_eval_render_uses_left_space_at_right_edge(self) -> None:
        state = reveal_positions(make_state(entry_edge=Direction.E), [Position(8, 4)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[4], "? ? ? ? ? ? ? ?@>")

    def test_player_cell_text_includes_object_and_warp_details(self) -> None:
        state = place_object(make_state(), Position(4, 8), SrsObjectType.SALVAGE, "salvage-a")
        state = replace(
            state,
            objects={
                **state.objects,
                "salvage-a": replace(state.objects["salvage-a"], consumed=True),
            },
        )
        state = replace_cell_terrain(state, Position(4, 8), SrsTerrainType.RIFT_BARRIER)
        state = reveal_positions(state, [Position(4, 8)])

        text = _player_cell_text(state)

        self.assertEqual(
            text,
            "- position=(4,8), terrain=RIFT_BARRIER, warp=S, object=SALVAGE, consumed=true, activated=false",
        )
