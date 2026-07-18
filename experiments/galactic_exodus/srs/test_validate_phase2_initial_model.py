from __future__ import annotations

import csv
import json
import shutil
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path

from experiments.galactic_exodus.archive.evaluation.srs import validate_phase2_initial_model as validator


class Phase2InitialModelValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixtures = Path("experiments/galactic_exodus/srs")
        self.archive_docs = Path("experiments/galactic_exodus/docs/archive")
        self.root = Path(".tmp/phase2_initial_model_tests") / self._testMethodName
        self.root.mkdir(parents=True, exist_ok=True)
        self.model = self.copy_archive_doc("phase2_initial_model.md")
        self.questions = self.copy_fixture("phase2_questions.csv")
        self.values = self.copy_fixture("phase2_initial_values.json")
        self.elements = self.copy_fixture("phase2_srs_elements.json")
        self.generation = self.copy_fixture("phase2_srs_generation.json")

    def tearDown(self) -> None:
        shutil.rmtree(self.root, ignore_errors=True)

    def copy_fixture(self, name: str) -> Path:
        src = self.fixtures / name
        dst = self.root / name
        dst.write_text(src.read_text(encoding="utf-8"), encoding="utf-8")
        return dst

    def copy_archive_doc(self, name: str) -> Path:
        src = self.archive_docs / name
        dst = self.root / name
        dst.write_text(src.read_text(encoding="utf-8"), encoding="utf-8")
        return dst

    def validate_all(self) -> None:
        validator.validate_all(
            self.model,
            self.questions,
            self.values,
            self.elements,
            self.generation,
        )

    def run_main(self, *args: str) -> tuple[int, str, str]:
        stdout = StringIO()
        stderr = StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = validator.main(list(args))
        return exit_code, stdout.getvalue(), stderr.getvalue()

    def read_json(self, path: Path) -> dict[str, object]:
        return json.loads(path.read_text(encoding="utf-8"))

    def write_json(self, path: Path, payload: dict[str, object]) -> None:
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    def read_question_rows(self) -> list[dict[str, str]]:
        with self.questions.open(encoding="utf-8", newline="") as file:
            return list(csv.DictReader(file))

    def write_question_rows(self, rows: list[dict[str, str]]) -> None:
        with self.questions.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=validator.QUESTION_FIELDS)
            writer.writeheader()
            writer.writerows(rows)

    def mutate_question(self, question_id: str, field: str, value: str) -> None:
        rows = self.read_question_rows()
        for row in rows:
            if row["question_id"] == question_id:
                row[field] = value
                break
        self.write_question_rows(rows)

    def assert_invalid(self, pattern: str) -> None:
        with self.assertRaisesRegex(validator.ValidationError, pattern):
            self.validate_all()

    def test_validate_all_accepts_repository_artifacts(self) -> None:
        validator.validate_all(
            self.archive_docs / "phase2_initial_model.md",
            self.fixtures / "phase2_questions.csv",
            self.fixtures / "phase2_initial_values.json",
            self.fixtures / "phase2_srs_elements.json",
            self.fixtures / "phase2_srs_generation.json",
        )

    def test_cli_reports_cross_file_ok(self) -> None:
        exit_code, stdout, stderr = self.run_main(
            "--model",
            str(self.model),
            "--questions",
            str(self.questions),
            "--values",
            str(self.values),
            "--elements",
            str(self.elements),
            "--generation",
            str(self.generation),
        )
        self.assertEqual(exit_code, 0)
        self.assertEqual(stderr, "")
        self.assertIn("cross-file: OK", stdout)

    def test_cli_requires_elements_argument(self) -> None:
        with redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit) as cm:
                validator.main(
                    [
                        "--model",
                        str(self.model),
                        "--questions",
                        str(self.questions),
                        "--values",
                        str(self.values),
                        "--generation",
                        str(self.generation),
                    ]
                )
        self.assertNotEqual(cm.exception.code, 0)

    def test_cli_requires_generation_argument(self) -> None:
        with redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit) as cm:
                validator.main(
                    [
                        "--model",
                        str(self.model),
                        "--questions",
                        str(self.questions),
                        "--values",
                        str(self.values),
                        "--elements",
                        str(self.elements),
                    ]
                )
        self.assertNotEqual(cm.exception.code, 0)

    def test_cli_reports_error_for_broken_elements_json(self) -> None:
        self.elements.write_text("{broken", encoding="utf-8")
        exit_code, stdout, stderr = self.run_main(
            "--model",
            str(self.model),
            "--questions",
            str(self.questions),
            "--values",
            str(self.values),
            "--elements",
            str(self.elements),
            "--generation",
            str(self.generation),
        )
        self.assertEqual(exit_code, 1)
        self.assertEqual(stdout, "")
        self.assertIn("error:", stderr)

    def test_values_generation_schema_version_must_match_generation(self) -> None:
        payload = self.read_json(self.values)
        payload["generation_schema_version"] = 2
        self.write_json(self.values, payload)
        self.assert_invalid("generation_schema_version must be 1")

    def test_elements_schema_version_must_be_one(self) -> None:
        payload = self.read_json(self.elements)
        payload["schema_version"] = 2
        self.write_json(self.elements, payload)
        self.assert_invalid("schema_version must be 1")

    def test_sector_types_must_include_rift(self) -> None:
        payload = self.read_json(self.values)
        payload["sector_types"].remove("RIFT")
        self.write_json(self.values, payload)
        self.assert_invalid("sector_types must be")

    def test_terrain_types_must_include_rift_barrier(self) -> None:
        payload = self.read_json(self.values)
        payload["terrain_types"].remove("RIFT_BARRIER")
        self.write_json(self.values, payload)
        self.assert_invalid("terrain_types must be")

    def test_object_types_must_include_station(self) -> None:
        payload = self.read_json(self.values)
        payload["object_types"].remove("STATION")
        self.write_json(self.values, payload)
        self.assert_invalid("object_types must be")

    def test_elements_map_sizes_must_match_contract(self) -> None:
        payload = self.read_json(self.elements)
        payload["map_sizes"] = [[9, 9]]
        self.write_json(self.elements, payload)
        self.assert_invalid("map_sizes must be 9x9 and 11x11")

    def test_generation_map_sizes_must_match_contract(self) -> None:
        payload = self.read_json(self.generation)
        payload["map_sizes"] = [[9, 9]]
        self.write_json(self.generation, payload)
        self.assert_invalid("map_sizes must be 9x9 and 11x11")

    def test_contract_references_elements_basename_must_match(self) -> None:
        payload = self.read_json(self.values)
        payload["contract_references"]["elements"] = "wrong.json"
        self.write_json(self.values, payload)
        self.assert_invalid("contract_references must match")

    def test_contract_references_generation_basename_must_match(self) -> None:
        payload = self.read_json(self.values)
        payload["contract_references"]["generation"] = "wrong.json"
        self.write_json(self.values, payload)
        self.assert_invalid("contract_references must match")

    def test_generation_sector_profiles_must_include_gravity(self) -> None:
        payload = self.read_json(self.generation)
        del payload["sector_profiles"]["GRAVITY"]
        self.write_json(self.generation, payload)
        self.assert_invalid("sector_profiles must define all seven sector types")

    def test_elements_sector_terrain_matrix_must_include_resource(self) -> None:
        payload = self.read_json(self.elements)
        del payload["sector_terrain_matrix"]["RESOURCE"]
        self.write_json(self.elements, payload)
        self.assert_invalid("sector_terrain_matrix must match values.sector_types")

    def test_elements_terrain_object_matrix_must_include_floor(self) -> None:
        payload = self.read_json(self.elements)
        del payload["terrain_object_matrix"]["FLOOR"]
        self.write_json(self.elements, payload)
        self.assert_invalid("terrain_object_matrix must match values.terrain_types")

    def test_elements_terrain_object_matrix_rejects_unknown_object(self) -> None:
        payload = self.read_json(self.elements)
        payload["terrain_object_matrix"]["FLOOR"].append("UNKNOWN_OBJECT")
        self.write_json(self.elements, payload)
        self.assert_invalid("terrain_object_matrix.FLOOR must stay within values.object_types")

    def test_model_rejects_warp_point(self) -> None:
        self.model.write_text(self.model.read_text(encoding="utf-8") + "\nWARP_POINT\n", encoding="utf-8")
        self.assert_invalid("forbidden legacy token remains: WARP_POINT")

    def test_values_reject_obstacle_density_key(self) -> None:
        payload = self.read_json(self.values)
        payload["obstacle_density"] = 1
        self.write_json(self.values, payload)
        self.assert_invalid("forbidden legacy token remains: obstacle_density")

    def test_questions_reject_seven_by_seven_fixture(self) -> None:
        self.mutate_question("Q17", "required_fixtures", "seven_by_seven_crossing")
        self.assert_invalid("Q17.required_fixtures must match the Phase 2A1c contract")

    def test_elements_reject_base_node_object_type(self) -> None:
        payload = self.read_json(self.elements)
        payload["object_types"]["BASE_NODE"] = {"label": "legacy"}
        self.write_json(self.elements, payload)
        self.assert_invalid("forbidden legacy token remains: BASE_NODE")

    def test_generation_allows_legacy_contracts_removed_record(self) -> None:
        payload = self.read_json(self.generation)
        payload["legacy_contracts_removed"]["WARP_POINT"] = "removed"
        self.write_json(self.generation, payload)
        self.validate_all()

    def test_generation_rejects_legacy_token_outside_removed_contracts(self) -> None:
        payload = self.read_json(self.generation)
        payload["notes"] = "WARP_POINT"
        self.write_json(self.generation, payload)
        self.assert_invalid(r"root\.notes: forbidden term WARP_POINT must not appear")

    def test_gravity_field_vertical_is_allowed(self) -> None:
        self.model.write_text(
            self.model.read_text(encoding="utf-8") + "\nGRAVITY_FIELD_VERTICAL\n",
            encoding="utf-8",
        )
        self.validate_all()

    def test_gravity_field_horizontal_is_allowed(self) -> None:
        self.model.write_text(
            self.model.read_text(encoding="utf-8") + "\nGRAVITY_FIELD_HORIZONTAL\n",
            encoding="utf-8",
        )
        self.validate_all()

    def test_gravity_field_token_is_rejected(self) -> None:
        self.model.write_text(self.model.read_text(encoding="utf-8") + "\nGRAVITY_FIELD\n", encoding="utf-8")
        self.assert_invalid("forbidden legacy token remains: GRAVITY_FIELD")

    def test_q17_requires_terrain_count_by_type(self) -> None:
        self.mutate_question(
            "Q17",
            "automated_metrics",
            "special_terrain_ratio;isolated_cell_ratio;reachable_cell_ratio",
        )
        self.assert_invalid("Q17.automated_metrics must match")

    def test_q18_requires_object_reachability_rate(self) -> None:
        self.mutate_question(
            "Q18",
            "automated_metrics",
            "celestial_spacing;object_count_by_type;object_detour_cost",
        )
        self.assert_invalid("Q18.automated_metrics must match")

    def test_q19_requires_retry_index_p95(self) -> None:
        self.mutate_question(
            "Q19",
            "automated_metrics",
            "generation_failure_rate;retry_index_p50;max_retry_index",
        )
        self.assert_invalid("Q19.automated_metrics must match")

    def test_q19_requires_all_sector_types(self) -> None:
        self.mutate_question(
            "Q19",
            "required_sector_types",
            "NORMAL;BASE;RESOURCE;NEBULA;ASTEROID;RIFT",
        )
        self.assert_invalid("Q19.required_sector_types must match")

    def test_q19_requires_retry_index_p95_decision_rule(self) -> None:
        self.mutate_question(
            "Q19",
            "decision_rule",
            "generation_failure_rate=0、max_retry_index<=63",
        )
        self.assert_invalid("Q19.decision_rule must contain retry_index_p95<64")

    def test_q20_requires_deterministic_map_match_rate(self) -> None:
        self.mutate_question(
            "Q20",
            "automated_metrics",
            "generation_report_match_rate;seed_collision_count",
        )
        self.assert_invalid("Q20.automated_metrics must match")

    def test_q20_requires_all_sector_types(self) -> None:
        self.mutate_question(
            "Q20",
            "required_sector_types",
            "NORMAL;BASE;RESOURCE;NEBULA;ASTEROID;GRAVITY",
        )
        self.assert_invalid("Q20.required_sector_types must match")

    def test_q20_requires_generation_report_match_rate_decision_rule(self) -> None:
        self.mutate_question(
            "Q20",
            "decision_rule",
            "deterministic_map_match_rate=1.0、seed_collision_count=0",
        )
        self.assert_invalid("Q20.decision_rule must contain generation_report_match_rate=1.0")


if __name__ == "__main__":
    unittest.main()
