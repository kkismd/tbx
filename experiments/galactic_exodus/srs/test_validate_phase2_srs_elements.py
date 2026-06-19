from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs import validate_phase2_srs_elements as validator


class Phase2SrsElementsValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tempdir.name) / "elements.json"
        source = Path(__file__).with_name("phase2_srs_elements.json")
        self.payload = json.loads(source.read_text(encoding="utf-8"))
        self.write()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write(self) -> None:
        self.path.write_text(json.dumps(self.payload), encoding="utf-8")

    def test_valid_contract_is_accepted(self) -> None:
        validator.validate(self.path)

    def test_7x7_is_rejected(self) -> None:
        self.payload["map_sizes"] = [[7, 7], [9, 9]]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "9x9 and 11x11"):
            validator.validate(self.path)

    def test_nebula_observation_must_be_3(self) -> None:
        self.payload["terrain_types"]["NEBULA"]["observation_size"] = 5
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "NEBULA"):
            validator.validate(self.path)

    def test_only_floor_hosts_warp_flag(self) -> None:
        self.payload["terrain_types"]["DEBRIS"]["can_host_feature"] = True
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "DEBRIS must not host features"):
            validator.validate(self.path)

    def test_gravity_axis_contract_is_enforced(self) -> None:
        self.payload["terrain_types"]["GRAVITY_FIELD_VERTICAL"][
            "double_cost_when_axis_changes"
        ] = "Y"
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "vertical gravity"):
            validator.validate(self.path)

    def test_impassable_collision_must_not_consume_cost(self) -> None:
        self.payload["object_types"]["PLANET"]["movement_cost_consumed_on_collision"] = True
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "PLANET collision"):
            validator.validate(self.path)

    def test_station_must_be_floor_only(self) -> None:
        self.payload["terrain_object_matrix"]["NEBULA"].append("STATION")
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "STATION must be FLOOR-only"):
            validator.validate(self.path)

    def test_impassable_terrain_cannot_host_objects(self) -> None:
        self.payload["terrain_object_matrix"]["ASTEROID"] = ["SALVAGE"]
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "impassable terrain"):
            validator.validate(self.path)

    def test_gravity_sector_requires_at_least_one_field(self) -> None:
        self.payload["sector_terrain_matrix"]["GRAVITY"]["invariants"][
            "gravity_field_total_min"
        ] = 0
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "at least one gravity field"):
            validator.validate(self.path)

    def test_removed_wall_must_not_return(self) -> None:
        self.payload["terrain_types"]["WALL"] = dict(
            self.payload["terrain_types"]["ASTEROID"]
        )
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "exact expected set"):
            validator.validate(self.path)


if __name__ == "__main__":
    unittest.main()
