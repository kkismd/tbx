from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import SrsInteractionError, apply_srs_command
from experiments.galactic_exodus.srs.log import (
    INTERACT_ACCEPTED,
    INTERACT_REJECTED,
    OBJECT_CONSUMED,
    STATION_ACTIVATED,
)
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SrsCommand,
    SrsObjectType,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object


REPO_ROOT = Path(__file__).resolve().parents[3]


def place_object_with_metadata(
    object_type: SrsObjectType,
    *,
    position: Position,
    object_id: str,
    fuel_restore: int | None = None,
):
    state = place_object(make_state(fuel=2, max_fuel=9), position, object_type, object_id)
    if fuel_restore is None:
        return state
    objects = dict(state.objects)
    objects[object_id] = replace(objects[object_id], metadata={"fuel_restore": fuel_restore})
    return replace(state, objects=objects)


class SrsEngineInteractionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_resource_cache_refuels_and_consumes(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=5,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 7)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertTrue(result.state.objects["resource-cache-1"].consumed)
        self.assertIn("resource-cache-1", result.state.persistent_state.consumed_object_ids)
        self.assertEqual([event.event_type for event in result.events], [INTERACT_ACCEPTED, OBJECT_CONSUMED])

    def test_resource_cache_reads_fuel_restore_metadata(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=3,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["fuel_delta"], 3)
        self.assertEqual(result.events[1].payload["fuel_restore"], 3)

    def test_resource_cache_does_not_recompute_restore_from_cache_count(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=4,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 6)
        self.assertEqual(result.events[1].payload["fuel_restore"], 4)

    def test_resource_cache_missing_fuel_restore_raises(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 8), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")

        with self.assertRaisesRegex(SrsInteractionError, "missing fuel_restore"):
            apply_srs_command(
                state,
                SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
                contracts=self.contracts,
            )

    def test_resource_cache_invalid_fuel_restore_raises(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=0,
        )

        with self.assertRaisesRegex(SrsInteractionError, "positive integer"):
            apply_srs_command(
                state,
                SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
                contracts=self.contracts,
            )

    def test_resource_cache_full_fuel_does_not_consume(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=5,
        )
        state = replace(state, fuel=9, max_fuel=9)

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, INTERACT_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_NO_EFFECT")
        self.assertEqual(result.state, state)

    def test_resource_cache_revisit_remains_consumed(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=5,
        )
        consumed_state = replace(
            state,
            objects={
                **state.objects,
                "resource-cache-1": replace(state.objects["resource-cache-1"], consumed=True),
            },
            persistent_state=replace(
                state.persistent_state,
                consumed_object_ids=frozenset({"resource-cache-1"}),
            ),
        )

        result = apply_srs_command(
            consumed_state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_ALREADY_CONSUMED")
        self.assertEqual(result.state, consumed_state)

    def test_station_refuels_to_max(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 9)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertTrue(result.state.objects["station-1"].activated)
        self.assertIn("station-1", result.state.persistent_state.activated_object_ids)
        self.assertEqual([event.event_type for event in result.events], [INTERACT_ACCEPTED, STATION_ACTIVATED])

    def test_station_reusable(self) -> None:
        state = place_object(make_state(fuel=3, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")
        state = replace(
            state,
            persistent_state=replace(
                state.persistent_state,
                activated_object_ids=frozenset({"station-1"}),
            ),
            objects={
                **state.objects,
                "station-1": replace(state.objects["station-1"], activated=True),
            },
            fuel=4,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "ACCEPTED")
        self.assertEqual(result.state.fuel, 9)

    def test_station_requires_adjacent(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 6), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_WRONG_RANGE")
        self.assertEqual(result.state, state)

    def test_station_full_fuel_rejected_no_effect(self) -> None:
        state = place_object(make_state(fuel=9, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_NO_EFFECT")
        self.assertEqual(result.state, state)

    def test_salvage_consumed_placeholder(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(4, 8), SrsObjectType.SALVAGE, "salvage-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="salvage-1"),
            contracts=self.contracts,
        )

        self.assertTrue(result.state.objects["salvage-1"].consumed)
        self.assertIn("salvage-1", result.state.persistent_state.consumed_object_ids)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertEqual(result.events[0].payload["fuel_delta"], 0)

    def test_interact_rejected_does_not_consume_turn(self) -> None:
        state = place_object(make_state(fuel=9, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 0)
        self.assertEqual(result.state, state)

    def test_interact_accepted_consumes_one_turn(self) -> None:
        state = place_object(make_state(fuel=1, max_fuel=9), Position(4, 7), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 1)

    def test_interaction_log_fields(self) -> None:
        state = place_object_with_metadata(
            SrsObjectType.RESOURCE_CACHE,
            position=Position(4, 8),
            object_id="resource-cache-1",
            fuel_restore=5,
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        payload = result.events[0].payload
        self.assertEqual(
            set(payload),
            {
                "command_type",
                "object_id",
                "object_type",
                "interaction_range",
                "effect",
                "position",
                "fuel_before",
                "fuel_after",
                "fuel_delta",
                "outcome",
            },
        )
        self.assertEqual(payload["object_id"], "resource-cache-1")
        self.assertEqual(payload["object_type"], "RESOURCE_CACHE")
        self.assertEqual(payload["interaction_range"], "SAME_CELL")
        self.assertEqual(payload["effect"], "REFUEL_PARTIAL")


if __name__ == "__main__":
    unittest.main()
