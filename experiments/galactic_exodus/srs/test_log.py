from __future__ import annotations

import unittest

from experiments.galactic_exodus.srs.log import (
    MOVE_ACCEPTED,
    MOVE_REJECTED,
    OBSERVATION_UPDATED,
    build_srs_log,
    make_turn_event,
)


class SrsLogTests(unittest.TestCase):
    def test_make_turn_event_freezes_payload(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=MOVE_ACCEPTED,
            payload={"value": 1},
        )

        with self.assertRaises(TypeError):
            event.payload["value"] = 2

    def test_build_srs_log_preserves_event_order(self) -> None:
        first = make_turn_event(srs_turn=1, event_type=MOVE_ACCEPTED, payload={})
        second = make_turn_event(srs_turn=1, event_type=OBSERVATION_UPDATED, payload={})

        log = build_srs_log([first, second])

        self.assertEqual(log.events, (first, second))

    def test_move_accepted_payload_has_required_fields(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=MOVE_ACCEPTED,
            payload={
                "command_type": "MOVE_ROUTE",
                "movement_rule": "MOVEMENT_POINTS",
                "cost_mode": "TURN_ONLY",
                "start_position": [4, 8],
                "end_position": [4, 7],
                "entered_cells": [[4, 7]],
                "blocked_position": None,
                "movement_raw_cost": 10,
                "fuel_delta": 0,
                "observation_updates": [[4, 7]],
                "outcome": "ACCEPTED",
            },
        )

        self.assertEqual(
            set(event.payload),
            {
                "command_type",
                "movement_rule",
                "cost_mode",
                "start_position",
                "end_position",
                "entered_cells",
                "blocked_position",
                "movement_raw_cost",
                "fuel_delta",
                "observation_updates",
                "outcome",
            },
        )

    def test_move_rejected_payload_has_required_fields(self) -> None:
        event = make_turn_event(
            srs_turn=0,
            event_type=MOVE_REJECTED,
            payload={
                "command_type": "MOVE_TO",
                "movement_rule": "MOVEMENT_POINTS",
                "cost_mode": "TURN_ONLY",
                "start_position": [4, 8],
                "end_position": [4, 8],
                "entered_cells": [],
                "blocked_position": None,
                "movement_raw_cost": 0,
                "fuel_delta": 0,
                "observation_updates": [],
                "outcome": "REJECTED_MOVE_TO_UNIMPLEMENTED",
            },
        )

        self.assertEqual(event.payload["movement_raw_cost"], 0)
        self.assertEqual(event.payload["observation_updates"], [])

    def test_observation_updated_payload_has_required_fields(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=OBSERVATION_UPDATED,
            payload={
                "center": [4, 7],
                "newly_discovered_count": 25,
                "total_discovered_count": 25,
            },
        )

        self.assertEqual(
            set(event.payload),
            {"center", "newly_discovered_count", "total_discovered_count"},
        )


if __name__ == "__main__":
    unittest.main()
