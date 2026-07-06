from __future__ import annotations

import unittest

from experiments.galactic_exodus import engine
from experiments.galactic_exodus import event_format
from experiments.galactic_exodus.test_engine import filled_cells, make_actual_map, make_state


class LrsEventFormatTests(unittest.TestCase):
    def test_moved_summary(self) -> None:
        state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))
        event = engine.apply_command(state, "E")

        self.assertEqual(
            event_format.format_lrs_event_summary(event),
            "MOVE  accepted to LRS=(2,1)",
        )

    def test_unknown_rift_summary(self) -> None:
        blocked_edge = ((1, 1), (1, 2))
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(blocked_edge,)),
        )
        event = engine.apply_command(state, "N")

        self.assertEqual(
            event_format.format_lrs_event_summary(event),
            "RIFT  discovered: LRS edge (1,1)-N is blocked",
        )

    def test_known_rift_rejection_summary(self) -> None:
        blocked_edge = ((1, 1), (1, 2))
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(blocked_edge,)),
        )
        engine.apply_command(state, "N")
        event = engine.apply_command(state, "N")

        self.assertEqual(
            event_format.format_lrs_event_summary(event),
            "MOVE  rejected: known RIFT blocks N",
        )

    def test_insufficient_fuel_summary(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        state = make_state(actual_map=make_actual_map(cells=cells), remaining_fuel=2)
        event = engine.apply_command(state, "E")

        self.assertEqual(
            event_format.format_lrs_event_summary(event),
            "MOVE  rejected: insufficient fuel",
        )

    def test_invalid_and_out_of_bounds_summaries(self) -> None:
        invalid_state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))
        invalid_event = engine.apply_command(invalid_state, "X")
        bounds_state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))
        bounds_event = engine.apply_command(bounds_state, "S")

        self.assertEqual(
            event_format.format_lrs_event_summary(invalid_event),
            "MOVE  rejected: invalid command",
        )
        self.assertEqual(
            event_format.format_lrs_event_summary(bounds_event),
            "MOVE  rejected: out of bounds",
        )

    def test_debug_summary_includes_event_type_and_payload(self) -> None:
        state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))
        event = engine.apply_command(state, "E")

        rendered = event_format.format_lrs_debug_event(event)

        self.assertIn("MOVED", rendered)
        self.assertIn("from=(1,1)", rendered)
        self.assertIn("to=(2,1)", rendered)
        self.assertIn("fuel=", rendered)

    def test_unknown_event_fallback_does_not_raise(self) -> None:
        event = engine.TurnEvent(
            turn=0,
            command="Q",
            outcome="UNKNOWN_EVENT",
            from_position=(1, 1),
            attempted_position=None,
            to_position=(1, 1),
            fuel_before=0,
            fuel_spent=0,
            fuel_after=0,
            required_fuel=None,
            discovered_cells=(),
            discovered_rift=False,
            supply_result=engine.SUPPLY_RESULT_NONE,
            supply_source=None,
            fuel_before_supply=None,
            fuel_after_supply=None,
            supply_amount=0,
            status_after=engine.GAME_STATUS_IN_PROGRESS,
        )

        self.assertEqual(event_format.format_lrs_event_summary(event), "EVENT UNKNOWN_EVENT")
        self.assertIn("UNKNOWN_EVENT", event_format.format_lrs_debug_event(event))

    def test_lrs_event_wording_snapshot(self) -> None:
        moved_state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))
        blocked_edge = ((1, 1), (1, 2))
        discovered_rift_state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(blocked_edge,)),
        )
        known_rift_state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(blocked_edge,)),
        )
        engine.apply_command(known_rift_state, "N")
        low_fuel_cells = filled_cells(".")
        low_fuel_cells[(2, 1)] = "A"
        low_fuel_state = make_state(
            actual_map=make_actual_map(cells=low_fuel_cells),
            remaining_fuel=2,
        )
        invalid_state = make_state(actual_map=make_actual_map(cells=filled_cells(".")))
        home_state = make_state(
            actual_map=make_actual_map(cells=filled_cells(".")),
            player_position=(8, 7),
            known_cells={(8, 7): ".", (8, 8): "H"},
            visited_cells={(8, 7)},
        )

        rendered = "\n".join(
            [
                event_format.format_lrs_event_summary(engine.apply_command(moved_state, "E")),
                event_format.format_lrs_event_summary(engine.apply_command(discovered_rift_state, "N")),
                event_format.format_lrs_event_summary(engine.apply_command(known_rift_state, "N")),
                event_format.format_lrs_event_summary(engine.apply_command(low_fuel_state, "E")),
                event_format.format_lrs_event_summary(engine.apply_command(invalid_state, "X")),
                event_format.format_lrs_event_summary(engine.apply_command(home_state, "N")),
            ]
        )

        self.assertEqual(
            rendered,
            "\n".join(
                [
                    "MOVE  accepted to LRS=(2,1)",
                    "RIFT  discovered: LRS edge (1,1)-N is blocked",
                    "MOVE  rejected: known RIFT blocks N",
                    "MOVE  rejected: insufficient fuel",
                    "MOVE  rejected: invalid command",
                    "MOVE  accepted to LRS=(8,8)",
                ]
            ),
        )


if __name__ == "__main__":
    unittest.main()
