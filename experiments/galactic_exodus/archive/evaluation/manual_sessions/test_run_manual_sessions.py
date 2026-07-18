from __future__ import annotations

import argparse
import csv
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from experiments.galactic_exodus.archive.evaluation.manual_sessions import run_manual_sessions


def make_log(log_path: Path, *, requested_seed: int, effective_seed: int) -> None:
    payload = {
        "schema_version": 3,
        "settings": {},
        "requested_seed": requested_seed,
        "effective_seed": effective_seed,
        "reroll_count": effective_seed - requested_seed,
        "initial_state": {},
        "events": [],
        "final_summary": {
            "outcome": "WON",
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


def complete_answers() -> dict[str, str]:
    answers = {key: "3" for key, _ in run_manual_sessions.SCORE_QUESTIONS}
    for index, _question in enumerate(run_manual_sessions.NOTE_QUESTIONS):
        answers[f"note_{index}"] = f"answer-{index}"
    return answers


class RunManualSessionsTests(unittest.TestCase):
    def test_save_feedback_and_load_feedback_round_trip_partial_answers(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            feedback_path = Path(tmp_dir) / "seed-001-feedback.json"
            answers = {
                "route_decision_score": "4",
                "note_0": "最初の回答",
            }

            run_manual_sessions.save_feedback(feedback_path, 1, answers)
            loaded = run_manual_sessions.load_feedback(feedback_path, 1)

        self.assertEqual(loaded, answers)

    def test_save_feedback_replaces_unpaired_surrogate_for_utf8_safety(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            feedback_path = Path(tmp_dir) / "seed-001-feedback.json"
            answers = {
                "note_0": "broken\ud800text",
            }

            run_manual_sessions.save_feedback(feedback_path, 1, answers)
            payload = json.loads(feedback_path.read_text(encoding="utf-8"))

        self.assertEqual(payload["answers"]["note_0"], "broken\ufffdtext")

    def test_main_reuses_existing_log_and_saved_feedback_without_replaying(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            log_dir = root / "manual"
            log_path = log_dir / "seed-001.json"
            output_path = root / "prototype_manual_sessions.csv"
            feedback_path = run_manual_sessions.feedback_path_for(log_path)
            make_log(log_path, requested_seed=1, effective_seed=1)
            run_manual_sessions.save_feedback(feedback_path, 1, complete_answers())

            args = argparse.Namespace(
                play_script=Path("experiments/galactic_exodus/archive/evaluation/phase1_lrs/play.py"),
                log_dir=log_dir,
                output=output_path,
                player_id="tester",
                seed_start=1,
                seed_end=1,
                redo_seed=[],
                python="python",
            )

            with (
                patch.object(run_manual_sessions, "parse_args", return_value=args),
                patch.object(run_manual_sessions, "confirm", return_value=True),
                patch.object(run_manual_sessions, "run_game") as run_game,
            ):
                exit_code = run_manual_sessions.main()

            with output_path.open(encoding="utf-8", newline="") as file:
                rows = list(csv.DictReader(file))

        self.assertEqual(exit_code, 0)
        run_game.assert_not_called()
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["requested_seed"], "1")
        self.assertEqual(rows[0]["player_id"], "tester")
        self.assertEqual(rows[0]["notes"], " / ".join(f"{question}: answer-{index}" for index, question in enumerate(run_manual_sessions.NOTE_QUESTIONS)))
        self.assertFalse(feedback_path.exists())


if __name__ == "__main__":
    unittest.main()
