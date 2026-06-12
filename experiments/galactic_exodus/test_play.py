from __future__ import annotations

import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from experiments.galactic_exodus import engine
from experiments.galactic_exodus import play
from experiments.galactic_exodus import simulate
from experiments.galactic_exodus.test_engine import filled_cells
from experiments.galactic_exodus.test_engine import make_actual_map
from experiments.galactic_exodus.test_engine import make_state


class PlayCliTests(unittest.TestCase):
    def test_script_entrypoint_starts_with_seed_42(self) -> None:
        result = subprocess.run(
            ["python", "experiments/galactic_exodus/play.py", "--seed", "42"],
            cwd=Path(__file__).resolve().parents[2],
            input="Q\n",
            text=True,
            capture_output=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertIn("SEED: requested=42 effective=42 rerolls=0", result.stdout)
        self.assertEqual(result.stderr, "")

    def run_cli(
        self,
        argv: list[str],
        stdin_text: str,
    ) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        exit_code = play.main(
            argv,
            stdin=io.StringIO(stdin_text),
            stdout=stdout,
            stderr=stderr,
        )
        return exit_code, stdout.getvalue(), stderr.getvalue()

    def test_missing_seed_prints_help_without_starting_game(self) -> None:
        with patch.object(engine, "create_game") as create_game:
            exit_code, stdout, stderr = self.run_cli([], "")

        self.assertEqual(exit_code, 0)
        self.assertEqual(stdout, "")
        self.assertIn("--seed", stderr)
        create_game.assert_not_called()

    def test_invalid_seed_does_not_start_game(self) -> None:
        with patch.object(engine, "create_game") as create_game:
            exit_code, stdout, stderr = self.run_cli(["--seed", "oops"], "")

        self.assertEqual(exit_code, 2)
        self.assertEqual(stdout, "")
        self.assertIn("invalid int value", stderr)
        create_game.assert_not_called()

    def test_seed_equals_syntax_is_accepted(self) -> None:
        exit_code, stdout, stderr = self.run_cli(["--seed=42"], "Q\n")

        self.assertEqual(exit_code, 0)
        self.assertIn("SEED: requested=42 effective=42 rerolls=0", stdout)
        self.assertEqual(stderr, "")

    def test_quit_command_exits_without_changing_state(self) -> None:
        exit_code, stdout, _ = self.run_cli(["--seed", "42"], "Q\n")

        self.assertEqual(exit_code, 0)
        self.assertEqual(stdout.count("MAP:\n"), 1)
        self.assertIn("TURN: 0\n", stdout)
        self.assertTrue(stdout.endswith("COMMAND> "))

    def test_eof_exits_without_changing_state(self) -> None:
        exit_code, stdout, _ = self.run_cli(["--seed", "42"], "")

        self.assertEqual(exit_code, 0)
        self.assertEqual(stdout.count("MAP:\n"), 1)
        self.assertIn("STATUS: IN PROGRESS\n", stdout)
        self.assertTrue(stdout.endswith("COMMAND> "))

    def test_lowercase_and_surrounding_whitespace_are_accepted(self) -> None:
        exit_code, stdout, _ = self.run_cli(["--seed", "42"], "  e  \nQ\n")

        self.assertEqual(exit_code, 0)
        self.assertIn("MOVED TO (2,1), COST ", stdout)
        self.assertIn("TURN: 1\n", stdout)

    def test_invalid_input_leaves_state_unchanged(self) -> None:
        exit_code, stdout, _ = self.run_cli(["--seed", "42"], "X\nQ\n")

        self.assertEqual(exit_code, 0)
        self.assertIn("INVALID COMMAND\n", stdout)
        self.assertIn("POSITION: (1,1)\n", stdout)
        self.assertIn("TURN: 0\n", stdout)

    def test_renderer_hides_unknown_cells_bases_resources_and_terrain(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "N"
        actual_map = make_actual_map(cells=cells, base_position=(3, 1), resource_positions=((4, 1),))
        state = make_state(actual_map=actual_map)

        rendered = play.board_lines(state)

        self.assertEqual(rendered[-1], "y=1 P ? ? ? ? ? ? ?")
        self.assertIn("y=8 ? ? ? ? ? ? ? H", rendered)

    def test_goal_is_visible_from_start_and_player_has_priority(self) -> None:
        cells = filled_cells(".")
        actual_map = make_actual_map(cells=cells)
        start_state = make_state(actual_map=actual_map)
        goal_state = make_state(
            actual_map=actual_map,
            player_position=simulate.SPECIAL_H,
            visited_cells={simulate.SPECIAL_H},
            path=[simulate.SPECIAL_H],
        )

        self.assertEqual(play.board_lines(start_state)[0], "y=8 ? ? ? ? ? ? ? H")
        self.assertEqual(play.board_lines(goal_state)[0], "y=8 ? ? ? ? ? ? ? P")

    def test_blocked_line_uses_known_adjacent_rifts(self) -> None:
        blocked_edge = simulate.normalize_edge((1, 1), (1, 2))
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(blocked_edge,)),
            known_routes={blocked_edge: engine.ROUTE_RIFT},
        )

        self.assertEqual(play.format_blocked_directions(state), "N")

    def test_turn_messages_cover_outcomes(self) -> None:
        blocked_edge = simulate.normalize_edge((1, 1), (1, 2))
        rift_state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), rift_edges=(blocked_edge,)),
            remaining_fuel=2,
        )
        blocked_event = engine.apply_command(rift_state, "N")
        known_event = engine.apply_command(rift_state, "N")

        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        insufficient_state = make_state(actual_map=make_actual_map(cells=cells), remaining_fuel=2)
        insufficient_event = engine.apply_command(insufficient_state, "E")

        self.assertEqual(play.format_event_messages(rift_state, blocked_event), ["RIFT BLOCKED N, COST 1"])
        self.assertEqual(play.format_event_messages(rift_state, known_event), ["KNOWN RIFT N"])
        self.assertEqual(
            play.format_event_messages(insufficient_state, insufficient_event),
            ["INSUFFICIENT FUEL: NEED 3, HAVE 2"],
        )

    def test_turn_messages_cover_supply_results(self) -> None:
        base_state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), base_position=(2, 1)),
            settings=engine.GameSettings(initial_fuel=3, max_fuel=5, resource_count=0),
            remaining_fuel=3,
        )
        resource_state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((2, 1),)),
            settings=engine.GameSettings(initial_fuel=3, max_fuel=16, resource_supply=5),
            remaining_fuel=3,
        )

        base_event = engine.apply_command(base_state, "E")
        resource_event = engine.apply_command(resource_state, "E")

        self.assertIn("REFUELED AT B(2,1): 2 -> 5", play.format_event_messages(base_state, base_event))
        self.assertIn(
            "REFUELED AT R(2,1): +5 (2 -> 7)",
            play.format_event_messages(resource_state, resource_event),
        )

    def test_generation_error_is_reported_separately(self) -> None:
        with patch.object(
            engine,
            "create_game",
            side_effect=engine.GenerationError(
                requested_seed=99,
                attempts=100,
                last_candidate_seed=198,
                reason="NO_REACHABLE_MAP",
                message="no map",
            ),
        ):
            exit_code, stdout, stderr = self.run_cli(["--seed", "99"], "")

        self.assertEqual(exit_code, 1)
        self.assertEqual(stderr, "")
        self.assertIn(
            "GENERATION ERROR: requested=99 attempts=100 last_candidate_seed=198 reason=NO_REACHABLE_MAP message=no map",
            stdout,
        )

    def test_same_seed_and_input_produce_same_output(self) -> None:
        first = self.run_cli(["--seed", "42"], "E\nN\nQ\n")
        second = self.run_cli(["--seed", "42"], "E\nN\nQ\n")

        self.assertEqual(first, second)

    def test_json_log_writes_schema_version_3(self) -> None:
        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            log_path = Path(tmp_dir) / "play-log.json"

            exit_code, stdout, stderr = self.run_cli(
                ["--seed", "42", "--json-log", str(log_path)],
                "Q\n",
            )

            payload = json.loads(log_path.read_text(encoding="utf-8"))

        self.assertEqual(exit_code, 0)
        self.assertEqual(stderr, "")
        self.assertTrue(stdout.endswith("COMMAND> "))
        self.assertEqual(payload["schema_version"], 3)
        self.assertEqual(payload["final_summary"]["outcome"], engine.FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION)
        self.assertIsNone(payload["generation_error"])

    def test_state_panel_uses_fixed_supply_fields(self) -> None:
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((3, 2), (2, 3))),
            remaining_fuel=7,
            used_resource_positions={(2, 3), (3, 2)},
            last_supply_source=engine.SupplySource(kind="R", position=(3, 2)),
        )

        stdout = io.StringIO()
        play.render_state(state, stdout)
        rendered = stdout.getvalue()

        self.assertIn("FUEL: 7/16\n", rendered)
        self.assertIn("LAST SUPPLY: R(3,2)\n", rendered)
        self.assertIn("USED R: (2,3),(3,2)\n", rendered)


if __name__ == "__main__":
    unittest.main()
