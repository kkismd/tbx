from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.engine import apply_srs_command
from experiments.galactic_exodus.srs.log import (
    INTERACT_ACCEPTED,
    INTERACT_REJECTED,
    OBJECT_CONSUMED,
    STATION_ACTIVATED,
)
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SrsBaseUpgrade,
    SrsCommand,
    SrsObjectType,
    SrsSalvageChoice,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state, place_object


REPO_ROOT = Path(__file__).resolve().parents[3]

class SrsEngineInteractionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_resource_cache_refuels_and_consumes(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 9), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 5)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertTrue(result.state.objects["resource-cache-1"].consumed)
        self.assertIn("resource-cache-1", result.state.persistent_state.consumed_object_ids)
        self.assertEqual([event.event_type for event in result.events], [INTERACT_ACCEPTED, OBJECT_CONSUMED])

    def test_resource_cache_uses_issue_fixed_restore_value(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 9), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["fuel_delta"], 3)
        self.assertEqual(result.events[1].payload["fuel_restore"], 3)

    def test_resource_cache_caps_at_max_fuel(self) -> None:
        state = place_object(make_state(fuel=8, max_fuel=9), Position(5, 9), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 9)
        self.assertEqual(result.events[0].payload["fuel_delta"], 1)

    def test_resource_cache_full_fuel_does_not_consume(self) -> None:
        state = place_object(make_state(fuel=9, max_fuel=9), Position(5, 9), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="resource-cache-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].event_type, INTERACT_REJECTED)
        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_NO_EFFECT")
        self.assertEqual(result.state, state)

    def test_resource_cache_revisit_remains_consumed(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 9), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")
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

    def test_station_recovers_all_resources_to_max(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
        state = replace(
            state,
            player_state=replace(
                state.player_state,
                durability=91,
                energy=2,
                photon_torpedo_ammo=1,
            ),
        )

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.fuel, 9)
        self.assertEqual(result.state.player_state.durability, 100)
        self.assertEqual(result.state.player_state.energy, 6)
        self.assertEqual(result.state.player_state.photon_torpedo_ammo, 6)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertTrue(result.state.objects["station-1"].activated)
        self.assertIn("station-1", result.state.persistent_state.activated_object_ids)
        self.assertEqual([event.event_type for event in result.events], [INTERACT_ACCEPTED, STATION_ACTIVATED])

    def test_station_reusable(self) -> None:
        state = place_object(make_state(fuel=3, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
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
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 7), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "REJECTED_WRONG_RANGE")
        self.assertEqual(result.state, state)

    def test_station_can_buy_base_upgrade(self) -> None:
        state = place_object(make_state(fuel=9, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")
        state = replace(
            state,
            player_state=replace(state.player_state, salvage=4, defense=1),
        )

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="INTERACT",
                target_object_id="station-1",
                base_upgrade_choice=SrsBaseUpgrade.DEFENSE,
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.events[0].payload["outcome"], "ACCEPTED")
        self.assertEqual(result.state.player_state.salvage, 0)
        self.assertEqual(result.state.player_state.defense, 2)
        self.assertEqual(result.events[0].payload["selected_upgrade"], "DEFENSE")

    def test_salvage_store_only_adds_inventory(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 9), SrsObjectType.SALVAGE, "salvage-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="salvage-1"),
            contracts=self.contracts,
        )

        self.assertTrue(result.state.objects["salvage-1"].consumed)
        self.assertIn("salvage-1", result.state.persistent_state.consumed_object_ids)
        self.assertEqual(result.state.srs_turn, 1)
        self.assertEqual(result.events[0].payload["fuel_delta"], 0)
        self.assertEqual(result.state.player_state.salvage, 1)
        self.assertEqual(result.events[0].payload["selected_salvage_choice"], "STORE_ONLY")

    def test_salvage_recover_energy_adds_inventory_and_caps_no_overflow(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 9), SrsObjectType.SALVAGE, "salvage-1")
        state = replace(state, player_state=replace(state.player_state, energy=5))

        result = apply_srs_command(
            state,
            SrsCommand(
                command_type="INTERACT",
                target_object_id="salvage-1",
                salvage_choice=SrsSalvageChoice.RECOVER_ENERGY,
            ),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.player_state.energy, 6)
        self.assertEqual(result.state.player_state.salvage, 1)
        self.assertEqual(result.events[0].payload["energy_delta"], 1)

    def test_interact_rejected_does_not_consume_turn(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 7), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 0)
        self.assertEqual(result.state, state)

    def test_interact_accepted_consumes_one_turn(self) -> None:
        state = place_object(make_state(fuel=1, max_fuel=9), Position(5, 8), SrsObjectType.STATION, "station-1")

        result = apply_srs_command(
            state,
            SrsCommand(command_type="INTERACT", target_object_id="station-1"),
            contracts=self.contracts,
        )

        self.assertEqual(result.state.srs_turn, 1)

    def test_interaction_log_fields(self) -> None:
        state = place_object(make_state(fuel=2, max_fuel=9), Position(5, 9), SrsObjectType.RESOURCE_CACHE, "resource-cache-1")

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
