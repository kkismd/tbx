from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs import validate_phase2_results as validator


class Phase2ReferenceValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.repo_root = Path(__file__).resolve().parents[3]
        self.tmp_root = self.repo_root / ".tmp"
        self.tmp_root.mkdir(exist_ok=True)
        self.tempdir = tempfile.TemporaryDirectory(dir=self.tmp_root)
        self.path = Path(self.tempdir.name) / "phase2_reference.json"
        source = Path(__file__).parent / "fixtures" / "phase2_reference.json"
        self.source_dir = source.parent
        self.payload = json.loads(source.read_text(encoding="utf-8"))
        for case in self.payload["cases"]:
            case["fixture_path"] = str((self.source_dir / case["fixture_path"]).resolve())
        self.write()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write(self) -> None:
        self.path.write_text(json.dumps(self.payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    def assert_invalid(self, pattern: str) -> None:
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, pattern):
            validator.validate(self.path)

    def test_valid_reference_is_accepted(self) -> None:
        summary = validator.validate(self.path)
        self.assertEqual(summary["case_count"], 19)

    def test_missing_required_case_is_rejected(self) -> None:
        self.payload["cases"] = [case for case in self.payload["cases"] if case["case_id"] != "shared_fuel_route"]
        self.assert_invalid("missing required cases")

    def test_missing_fixture_file_is_rejected(self) -> None:
        self.payload["cases"][0]["fixture_path"] = "fixtures/does_not_exist.json"
        self.assert_invalid("fixture file not found")

    def test_final_expect_unknown_field_is_rejected(self) -> None:
        self.payload["cases"][0]["final_expect"]["bad_field"] = True
        self.assert_invalid("unknown field")

    def test_nebula_discovered_count_mismatch_is_rejected(self) -> None:
        for case in self.payload["cases"]:
            if case["case_id"] == "nebula_3x3_observation":
                case["final_expect"]["discovered_count"] = 10
                break
        self.assert_invalid("discovered_count expected 10")

    def test_turn_only_vs_shared_fuel_delta_mismatch_is_rejected(self) -> None:
        self.payload["comparisons"]["turn_only_vs_shared_fuel"]["expected_fuel_delta"] = 3
        self.assert_invalid("fuel delta expected 3")

    def test_cli_reports_ok_for_valid_reference(self) -> None:
        result = subprocess.run(
            [
                "python",
                "experiments/galactic_exodus/srs/validate_phase2_results.py",
                "experiments/galactic_exodus/srs/fixtures/phase2_reference.json",
            ],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("Phase 2 SRS reference fixture: OK", result.stdout)

    def test_cli_returns_nonzero_for_invalid_reference(self) -> None:
        self.payload["reference_schema_version"] = 2
        self.write()
        result = subprocess.run(
            [
                "python",
                "experiments/galactic_exodus/srs/validate_phase2_results.py",
                str(self.path),
            ],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("error:", result.stderr)


if __name__ == "__main__":
    unittest.main()
