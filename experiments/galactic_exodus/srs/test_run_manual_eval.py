from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.model import Direction, Position, SrsObjectType, SrsTerrainType
from experiments.galactic_exodus.srs.run_fixture import run_fixture
from experiments.galactic_exodus.srs.run_manual_eval import (
    _event_summary_lines,
    _player_cell_text,
    _render_known_map_spaced_for_manual_eval,
)
from experiments.galactic_exodus.srs.render import render_row_for_internal_y
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object, reveal_positions, replace_cell_terrain


class SrsRunManualEvalTests(unittest.TestCase):
    def _row_for_internal_y(self, *, height: int, y: int) -> int:
        return render_row_for_internal_y(height=height, y=y)

    def _summary_lines_for_fixture(self, fixture_name: str) -> list[str]:
        result = run_fixture(Path(__file__).resolve().parent / "fixtures" / fixture_name)
        return _event_summary_lines(result)

    def test_manual_eval_render_keeps_player_on_floor_cell(self) -> None:
        state = replace(make_state(), player_position=Position(4, 1))
        state = reveal_positions(state, [Position(x, 1) for x in range(9)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=1)], " 2  . . . . @ . . . .")
        self.assertEqual(rendered.splitlines()[9], "")
        self.assertEqual(rendered.splitlines()[10], "    1 2 3 4 5 6 7 8 9")

    def test_manual_eval_render_shows_player_beside_warp_symbol(self) -> None:
        state = reveal_positions(make_state(), [Position(4, 0)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=0)], " 1  ? ? ? ? v@? ? ? ?")

    def test_manual_eval_render_shows_player_beside_salvage_symbol(self) -> None:
        state = place_object(make_state(), Position(4, 2), SrsObjectType.SALVAGE, "salvage-a")
        state = replace(state, player_position=Position(4, 2))
        state = replace(
            state,
            objects={
                **state.objects,
                "salvage-a": replace(state.objects["salvage-a"], consumed=True),
            },
        )
        state = reveal_positions(state, [Position(4, 2)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=2)], " 3  ? ? ? ? s@? ? ? ?")

    def test_manual_eval_render_uses_left_space_at_right_edge(self) -> None:
        state = reveal_positions(make_state(entry_edge=Direction.E), [Position(8, 4)])

        rendered = _render_known_map_spaced_for_manual_eval(state)

        self.assertEqual(rendered.splitlines()[self._row_for_internal_y(height=9, y=4)], " 5  ? ? ? ? ? ? ? ?@>")

    def test_player_cell_text_includes_object_and_warp_details(self) -> None:
        state = place_object(make_state(), Position(4, 0), SrsObjectType.SALVAGE, "salvage-a")
        state = replace(
            state,
            objects={
                **state.objects,
                "salvage-a": replace(state.objects["salvage-a"], consumed=True),
            },
        )
        state = replace_cell_terrain(state, Position(4, 0), SrsTerrainType.RIFT_BARRIER)
        state = reveal_positions(state, [Position(4, 0)])

        text = _player_cell_text(state)

        self.assertEqual(
            text,
            "- position=(5,1), terrain=RIFT_BARRIER, warp=S, object=SALVAGE, consumed=true, activated=false",
        )

    def test_event_summary_positions_use_display_coordinates(self) -> None:
        lines = self._summary_lines_for_fixture("move_to_known_9x9.json")

        self.assertIn("turn 1: MOVE_ACCEPTED MOVE_TO (5,1) -> (5,3) fuel 0->0", lines)
        self.assertIn("turn 1: OBSERVATION_UPDATED center=(5,2) new=0 total=81", lines)
        self.assertIn("turn 1: OBSERVATION_UPDATED center=(5,3) new=0 total=81", lines)

    def test_event_summary_resource_cache_includes_fuel_and_consumed_state(self) -> None:
        lines = self._summary_lines_for_fixture("resource_cache_single_9x9.json")

        self.assertEqual(
            lines[-2:],
            [
                "turn 1: INTERACT_ACCEPTED RESOURCE_CACHE resource-cache-1 outcome=ACCEPTED fuel 2->5 restore=3 consumed=true",
                "turn 1: OBJECT_CONSUMED RESOURCE_CACHE resource-cache-1",
            ],
        )

    def test_manual_eval_resource_cache_fixture_keeps_target_area_visible(self) -> None:
        result = run_fixture(Path(__file__).resolve().parent / "fixtures" / "resource_cache_single_9x9.json")

        self.assertEqual(
            _player_cell_text(result.final_state),
            "- position=(3,8), terrain=FLOOR, object=RESOURCE_CACHE, consumed=true, activated=false",
        )
        self.assertEqual(
            _render_known_map_spaced_for_manual_eval(result.final_state).splitlines()[self._row_for_internal_y(height=9, y=7)],
            " 8  ? . r@. ? ? ? ? ?",
        )

    def test_event_summary_station_includes_refuel_and_activation(self) -> None:
        lines = self._summary_lines_for_fixture("station_refuel_9x9.json")

        self.assertEqual(
            lines[-2:],
            [
                "turn 1: INTERACT_ACCEPTED STATION station-1 outcome=ACCEPTED fuel 2->9 refuel_to_max=true activated=true",
                "turn 1: STATION_ACTIVATED STATION station-1",
            ],
        )

    def test_event_summary_salvage_marks_placeholder_and_consumed(self) -> None:
        lines = self._summary_lines_for_fixture("salvage_placeholder_9x9.json")

        self.assertEqual(
            lines[-2:],
            [
                "turn 1: INTERACT_ACCEPTED SALVAGE salvage-1 outcome=ACCEPTED fuel 2->2 placeholder=true consumed=true",
                "turn 1: OBJECT_CONSUMED SALVAGE salvage-1",
            ],
        )

    def test_event_summary_revisit_resource_reject_mentions_consumed_object(self) -> None:
        lines = self._summary_lines_for_fixture("revisit_resource_consumed_9x9.json")

        self.assertIn(
            "turn 0: INTERACT_REJECTED RESOURCE_CACHE resource-cache-1 outcome=REJECTED_ALREADY_CONSUMED fuel 2->2 consumed=true",
            lines,
        )

    def test_manual_eval_revisit_resource_fixture_shows_consumed_player_cell(self) -> None:
        result = run_fixture(Path(__file__).resolve().parent / "fixtures" / "revisit_resource_consumed_9x9.json")

        self.assertEqual(
            _player_cell_text(result.final_state),
            "- position=(3,8), terrain=FLOOR, object=RESOURCE_CACHE, consumed=true, activated=false",
        )
