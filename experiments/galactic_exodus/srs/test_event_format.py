from __future__ import annotations

import unittest

from experiments.galactic_exodus.srs import event_format, log
from experiments.galactic_exodus.srs.log import make_turn_event


class SrsEventFormatTests(unittest.TestCase):
    def test_move_accepted_uses_display_coordinate(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.MOVE_ACCEPTED,
            payload={
                "resolved_route": ["E", "E"],
                "end_position": [6, 3],
            },
        )

        rendered = event_format.format_srs_event_summary(event)

        self.assertEqual(rendered, "MOVE  accepted route=E,E to SRS=(7,4)")
        self.assertNotIn("internal=", rendered)

    def test_stopped_before_impassable(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.STOPPED_BEFORE_IMPASSABLE,
            payload={"terrain": "RIFT_BARRIER", "blocked_position": [8, 3]},
        )

        self.assertEqual(
            event_format.format_srs_event_summary(event),
            "STOP  blocked by RIFT_BARRIER at SRS=(9,4)",
        )

    def test_observation_updated(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.OBSERVATION_UPDATED,
            payload={"newly_discovered_count": 6, "total_discovered_count": 34},
        )

        self.assertEqual(
            event_format.format_srs_event_summary(event),
            "SCAN  5x5 update: +6 known cells, total=34",
        )

    def test_nebula_observation_returns_two_lines(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.OBSERVATION_UPDATED,
            payload={
                "nebula_interference": True,
                "sensor_range": 3,
                "newly_discovered_count": 4,
                "total_discovered_count": 18,
            },
        )

        self.assertEqual(
            event_format.format_srs_event_summary_lines(event),
            [
                "SCAN  NEBULA interference: sensor range reduced to 3x3",
                "SCAN  3x3 update: +4 known cells, total=18",
            ],
        )

    def test_resource_cache_consumed(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.OBJECT_CONSUMED,
            payload={"object_type": "RESOURCE_CACHE", "fuel_delta": 3, "fuel_after": 6, "max_fuel": 9},
        )

        self.assertEqual(
            event_format.format_srs_event_summary(event),
            "CACHE acquired: fuel +3 -> 6/9",
        )

    def test_salvage_consumed(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.OBJECT_CONSUMED,
            payload={
                "object_type": "SALVAGE",
                "salvage_after": 1,
                "durability_delta": 8,
                "durability_after": 100,
                "durability_capacity": 100,
            },
        )

        self.assertEqual(
            event_format.format_srs_event_summary(event),
            "SALVAGE acquired: +1 inventory, durability +8 -> 100/100",
        )

    def test_station_activated_and_upgrade(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.STATION_ACTIVATED,
            payload={
                "applied_upgrade": "DEFENSE",
                "salvage_before": 4,
                "salvage_after": 0,
            },
        )

        self.assertEqual(
            event_format.format_srs_event_summary_lines(event),
            [
                "BASE station activated: full recovery complete",
                "UPGRADE defense +1, salvage 4 -> 0",
            ],
        )

    def test_warp_accepted_and_rejected(self) -> None:
        accepted = make_turn_event(
            srs_turn=1,
            event_type=log.WARP_EXIT_ACCEPTED,
            payload={"exit_direction": "S", "start_position": [4, 0]},
        )
        rejected = make_turn_event(
            srs_turn=0,
            event_type=log.WARP_EXIT_REJECTED,
            payload={"exit_direction": "E", "outcome": "REJECTED_BLOCKED_EDGE"},
        )

        self.assertEqual(
            event_format.format_srs_event_summary(accepted),
            "WARP  S accepted from SRS=(5,1)",
        )
        self.assertEqual(
            event_format.format_srs_event_summary(rejected),
            "WARP  rejected: E edge is blocked by RIFT_BARRIER",
        )

    def test_combat_transitioned_and_rejected(self) -> None:
        transitioned = make_turn_event(
            srs_turn=1,
            event_type=log.COMBAT_TRANSITIONED,
            payload={
                "phase_to": "PLAYER_ATTACK",
                "player_action": {"target_enemy_id": "enemy-1"},
                "target_position": [4, 4],
            },
        )
        rejected = make_turn_event(
            srs_turn=1,
            event_type=log.COMBAT_REJECTED,
            payload={"reason": "enemy out of range"},
        )

        self.assertEqual(
            event_format.format_srs_event_summary(transitioned),
            "COMBAT phase=PLAYER_ATTACK target=enemy-1 at SRS=(5,5)",
        )
        self.assertEqual(
            event_format.format_srs_event_summary(rejected),
            "COMBAT  rejected: enemy out of range",
        )

    def test_encounter_spawned_and_none(self) -> None:
        spawned = make_turn_event(
            srs_turn=1,
            event_type=log.ENCOUNTER_ROLLED,
            payload={
                "roll": 0.12,
                "threshold": 0.18,
                "enemy_id": "enemy-1",
                "enemy_tier": "TIER2",
                "spawn_position": [4, 4],
            },
        )
        none = make_turn_event(
            srs_turn=1,
            event_type=log.ENCOUNTER_ROLLED,
            payload={"roll": 0.42, "threshold": 0.18},
        )

        self.assertEqual(
            event_format.format_srs_event_summary(spawned),
            "ENCOUNTER roll=0.12 threshold=0.18 -> spawned enemy-1 TIER2 at SRS=(5,5)",
        )
        self.assertEqual(
            event_format.format_srs_event_summary(none),
            "ENCOUNTER roll=0.42 threshold=0.18 -> none",
        )

    def test_debug_event_includes_internal_display_pair(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type=log.MOVE_ACCEPTED,
            payload={"start_position": [4, 2], "end_position": [6, 3]},
        )

        rendered = event_format.format_srs_debug_event(event)

        self.assertIn("MOVE_ACCEPTED", rendered)
        self.assertIn("start_internal=(4,2)", rendered)
        self.assertIn("end_display=(7,4)", rendered)

    def test_unknown_event_fallback_does_not_raise(self) -> None:
        event = make_turn_event(
            srs_turn=1,
            event_type="UNKNOWN_EVENT",
            payload={"payload": "value"},
        )

        self.assertEqual(event_format.format_srs_event_summary(event), "EVENT UNKNOWN_EVENT")
        self.assertIn("UNKNOWN_EVENT", event_format.format_srs_debug_event(event))


if __name__ == "__main__":
    unittest.main()
