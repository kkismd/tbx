from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import (
    SrsContractError,
    load_default_contracts,
    load_initial_values,
    load_srs_elements,
    load_srs_generation,
    load_srs_movement,
)
from experiments.galactic_exodus.srs.model import Direction, ObservationMode


REPO_ROOT = Path(__file__).resolve().parents[3]
SRS_DIR = REPO_ROOT / "experiments" / "galactic_exodus" / "srs"


class SrsContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temp_dir = tempfile.TemporaryDirectory()
        self.temp_dir = Path(self._temp_dir.name)
        self.addCleanup(self._temp_dir.cleanup)

    def write_json(self, filename: str, payload: object) -> Path:
        path = self.temp_dir / filename
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def test_load_movement_contract_accepts_repository_file(self) -> None:
        contract = load_srs_movement(SRS_DIR / "phase2_srs_movement.json")
        self.assertEqual(contract.directions, (Direction.N, Direction.E, Direction.S, Direction.W))

    def test_load_initial_values_accepts_local_movement(self) -> None:
        contract = load_initial_values(SRS_DIR / "phase2_initial_values.json")
        self.assertIs(contract.baseline_observation_mode, ObservationMode.LOCAL_MOVEMENT)

    def test_load_default_contracts_cross_checks_baseline(self) -> None:
        contracts = load_default_contracts(REPO_ROOT)
        self.assertEqual(contracts.initial_values.baseline_cost_mode, contracts.movement.baseline_cost_mode)
        self.assertEqual(
            contracts.initial_values.movement_points_per_turn * 10,
            contracts.movement.movement_cost_budget_raw,
        )

    def test_rejects_legacy_local_3x3_in_movement_contract(self) -> None:
        payload = json.loads((SRS_DIR / "phase2_srs_movement.json").read_text(encoding="utf-8"))
        payload["observation"]["LOCAL_3X3"] = {"default_size": 3}
        with self.assertRaisesRegex(SrsContractError, "LOCAL_3X3 is legacy"):
            load_srs_movement(self.write_json("legacy-movement.json", payload))

    def test_rejects_legacy_local_3x3_in_initial_values(self) -> None:
        payload = json.loads((SRS_DIR / "phase2_initial_values.json").read_text(encoding="utf-8"))
        payload["baseline"]["observation_mode"] = "LOCAL_3X3"
        with self.assertRaisesRegex(SrsContractError, "LOCAL_3X3 is legacy"):
            load_initial_values(self.write_json("legacy-initial.json", payload))

    def test_load_srs_elements_keeps_raw_payload(self) -> None:
        contract = load_srs_elements(SRS_DIR / "phase2_srs_elements.json")
        self.assertEqual(contract.schema_version, 1)
        self.assertEqual(contract.raw["baseline_map_size"], [9, 9])

    def test_load_srs_generation_reads_map_sizes(self) -> None:
        contract = load_srs_generation(SRS_DIR / "phase2_srs_generation.json")
        self.assertEqual(contract.map_sizes, ((9, 9), (11, 11)))

    def test_missing_contract_file_raises_contract_error(self) -> None:
        with self.assertRaisesRegex(SrsContractError, "missing contract file"):
            load_srs_generation(self.temp_dir / "missing.json")

    def test_invalid_json_raises_contract_error(self) -> None:
        path = self.temp_dir / "invalid.json"
        path.write_text("{not-json}", encoding="utf-8")
        with self.assertRaisesRegex(SrsContractError, "invalid JSON"):
            load_srs_elements(path)


if __name__ == "__main__":
    unittest.main()
