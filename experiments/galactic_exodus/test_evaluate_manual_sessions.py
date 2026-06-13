from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus import evaluate_manual_sessions


FIELDNAMES = evaluate_manual_sessions.FIELDNAMES


def make_log(log_path: Path, *, requested_seed: int, effective_seed: int, outcome: str = "WON") -> None:
    payload = {
        "schema_version": 3,
        "settings": {},
        "requested_seed": requested_seed,
        "effective_seed": effective_seed,
        "reroll_count": effective_seed - requested_seed,
        "initial_state": {},
        "events": [],
        "final_summary": {
            "outcome": outcome,
            "turn_count": 12 + requested_seed,
            "remaining_fuel": 20 - requested_seed,
            "max_fuel": 16,
            "used_resource_positions": [],
            "base_visit_count": requested_seed % 2,
            "base_refuel_count": requested_seed % 2,
            "resource_visit_count": (requested_seed + 1) % 2,
            "resource_refuel_count": (requested_seed + 1) % 2,
            "last_supply_source": None,
            "rift_attempts": requested_seed % 3,
            "invalid_or_rejected_actions": 0,
            "path": [],
        },
        "generation_error": None,
    }
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(json.dumps(payload), encoding="utf-8")


def make_row(seed: int, log_path: Path) -> dict[str, str]:
    return {
        "session_id": f"manual-{seed:03d}",
        "player_id": "tester",
        "requested_seed": str(seed),
        "effective_seed": str(seed),
        "outcome": "WON",
        "turn_count": str(12 + seed),
        "remaining_fuel": str(20 - seed),
        "base_visit_count": str(seed % 2),
        "base_refuel_count": str(seed % 2),
        "resource_visit_count": str((seed + 1) % 2),
        "resource_refuel_count": str((seed + 1) % 2),
        "rift_attempts": str(seed % 3),
        "route_decision_score": "3",
        "information_score": "4",
        "fuel_tension_score": "2",
        "supply_choice_score": "3",
        "rift_fairness_score": "4",
        "readability_score": "5",
        "defeat_clarity_score": "5",
        "observation_range_score": "4",
        "resource_reveal_score": "3",
        "rift_asymmetry_score": "2",
        "base_return_value_score": "4",
        "base_loop_risk_score": "5",
        "notes": "迷った局面: なし / B: なし / R: なし / 断層: なし / 表示: なし",
        "log_path": log_path.as_posix(),
    }


class EvaluateManualSessionsTests(unittest.TestCase):
    def write_csv(self, csv_path: Path, rows: list[dict[str, str]]) -> None:
        csv_path.parent.mkdir(parents=True, exist_ok=True)
        with csv_path.open("w", encoding="utf-8", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=FIELDNAMES)
            writer.writeheader()
            writer.writerows(rows)

    def test_validate_manual_sessions_accepts_matching_csv_and_logs(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            csv_path = root / "prototype_manual_sessions.csv"
            rows: list[dict[str, str]] = []
            for seed in (1, 2):
                log_path = root / "manual" / f"seed-{seed:03d}.json"
                make_log(log_path, requested_seed=seed, effective_seed=seed)
                rows.append(make_row(seed, log_path))
            self.write_csv(csv_path, rows)

            exit_code = evaluate_manual_sessions.main(
                ["--csv", str(csv_path), "--seed-start", "1", "--seed-end", "2"]
            )

        self.assertEqual(exit_code, 0)

    def test_validate_manual_sessions_rejects_objective_mismatch(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            csv_path = root / "prototype_manual_sessions.csv"
            log_path = root / "manual" / "seed-001.json"
            make_log(log_path, requested_seed=1, effective_seed=1)
            row = make_row(1, log_path)
            row["turn_count"] = "999"
            self.write_csv(csv_path, [row])

            exit_code = evaluate_manual_sessions.main(
                ["--csv", str(csv_path), "--seed-start", "1", "--seed-end", "1"]
            )

        self.assertEqual(exit_code, 1)

    def test_validate_manual_sessions_rejects_invalid_score(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            csv_path = root / "prototype_manual_sessions.csv"
            log_path = root / "manual" / "seed-001.json"
            make_log(log_path, requested_seed=1, effective_seed=1)
            row = make_row(1, log_path)
            row["information_score"] = "6"
            self.write_csv(csv_path, [row])

            exit_code = evaluate_manual_sessions.main(
                ["--csv", str(csv_path), "--seed-start", "1", "--seed-end", "1"]
            )

        self.assertEqual(exit_code, 1)

    def test_validate_manual_sessions_rejects_replacement_character_in_notes(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            csv_path = root / "prototype_manual_sessions.csv"
            log_path = root / "manual" / "seed-001.json"
            make_log(log_path, requested_seed=1, effective_seed=1)
            row = make_row(1, log_path)
            row["notes"] = "断層�に阻まれた"
            self.write_csv(csv_path, [row])

            exit_code = evaluate_manual_sessions.main(
                ["--csv", str(csv_path), "--seed-start", "1", "--seed-end", "1"]
            )

        self.assertEqual(exit_code, 1)

    def test_validate_manual_sessions_rejects_replacement_character_in_player_id(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            csv_path = root / "prototype_manual_sessions.csv"
            log_path = root / "manual" / "seed-001.json"
            make_log(log_path, requested_seed=1, effective_seed=1)
            row = make_row(1, log_path)
            row["player_id"] = "test�er"
            self.write_csv(csv_path, [row])

            exit_code = evaluate_manual_sessions.main(
                ["--csv", str(csv_path), "--seed-start", "1", "--seed-end", "1"]
            )

        self.assertEqual(exit_code, 1)


if __name__ == "__main__":
    unittest.main()
