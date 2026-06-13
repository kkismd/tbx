from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus import simulate
from experiments.galactic_exodus import validate_phase1_spec as validator


class Phase1SpecValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.decisions = self.root / "decisions.csv"
        self.spec = self.root / "spec.md"
        self.fixtures = self.root / "fixtures.json"
        self.write_valid_artifacts()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write_valid_artifacts(self) -> None:
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            for index, finding in enumerate(sorted(validator.EXPECTED_FINDINGS), start=1):
                writer.writerow(
                    {
                        "decision_id": f"TEST-{index:03d}",
                        "question": "question",
                        "current_behavior": "current",
                        "decision": "decision",
                        "evidence": "evidence",
                        "source_finding_ids": finding,
                        "affected_issues": "#1050",
                        "tbx_impact": "impact",
                        "status": "DECIDED",
                        "deferred_issue": "-",
                    }
                )
        self.spec.write_text(
            "# spec\n盤面と既知情報\n移動\n燃料と補給\n勝敗とabort\n再抽選とseed\n"
            "Phase 1 UI契約\nGameLog schema v3\nPython/TBX一致契約\nreference fixture\nTEST-001\n",
            encoding="utf-8",
        )
        fixture_template = {
            "purpose": "purpose",
            "mode": "generated",
            "settings": {
                "width": 8,
                "height": 8,
                "start_position": {"x": 1, "y": 1},
                "goal_position": {"x": 8, "y": 8},
                "rift_density": 0.1,
                "initial_fuel": 16,
                "max_fuel": 16,
                "resource_count": 3,
                "resource_supply": 5,
            },
            "requested_seed": 1,
            "effective_seed": 1,
            "reroll_count": 0,
            "initial_actual_map": self.make_actual_map_payload(),
            "commands": [],
            "expected_initial": {},
            "expected_turns": [],
            "expected_final": {"outcome": "ABORTED_NO_POLICY_ACTION"},
            "generation_stub": None,
            "max_turns": 256,
        }
        fixtures = []
        for name in sorted(validator.REQUIRED_FIXTURES):
            fixture = dict(fixture_template)
            fixture["settings"] = dict(fixture_template["settings"])
            fixture["name"] = name
            fixtures.append(fixture)
        self.fixtures.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "game_log_schema_version": 3,
                    "fixtures": fixtures,
                }
            ),
            encoding="utf-8",
        )

    def make_actual_map_payload(self) -> dict[str, object]:
        cells = []
        for y in range(1, simulate.HEIGHT + 1):
            for x in range(1, simulate.WIDTH + 1):
                symbol = "."
                if (x, y) == (1, 1):
                    symbol = "S"
                elif (x, y) == (8, 8):
                    symbol = "H"
                elif (x, y) == (4, 4):
                    symbol = "B"
                cells.append({"position": {"x": x, "y": y}, "symbol": symbol})
        return {
            "cells": cells,
            "rift_edges": [],
            "base_position": {"x": 4, "y": 4},
            "resource_positions": [],
        }

    def test_validate_all_accepts_valid_artifacts(self) -> None:
        validator.validate_all(self.decisions, self.spec, self.fixtures)

    def test_deferred_decision_requires_issue(self) -> None:
        with self.decisions.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows[0]["status"] = "DEFERRED"
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "deferred_issue"):
            validator.validate_decisions(self.decisions)

    def test_missing_finding_is_rejected(self) -> None:
        with self.decisions.open(encoding="utf-8", newline="") as file:
            rows = list(csv.DictReader(file))
        rows[-1]["source_finding_ids"] = rows[-2]["source_finding_ids"]
        with self.decisions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.DECISION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(validator.ValidationError, "findings not processed"):
            validator.validate_decisions(self.decisions)

    def test_duplicate_fixture_name_is_rejected(self) -> None:
        payload = json.loads(self.fixtures.read_text(encoding="utf-8"))
        payload["fixtures"][1]["name"] = payload["fixtures"][0]["name"]
        self.fixtures.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "duplicate fixture name"):
            validator.validate_fixtures(self.fixtures)

    def test_unsorted_rift_edge_is_rejected(self) -> None:
        payload = json.loads(self.fixtures.read_text(encoding="utf-8"))
        fixture = payload["fixtures"][0]
        fixture["mode"] = "injected"
        fixture["initial_actual_map"] = self.make_actual_map_payload()
        fixture["initial_actual_map"]["rift_edges"] = [
            {"from": {"x": 2, "y": 1}, "to": {"x": 1, "y": 1}}
        ]
        self.fixtures.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaisesRegex(validator.ValidationError, "lexicographically sorted"):
            validator.validate_fixtures(self.fixtures)


if __name__ == "__main__":
    unittest.main()
