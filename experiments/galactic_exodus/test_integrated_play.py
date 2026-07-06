from __future__ import annotations

import io
from pathlib import Path
import subprocess
import unittest

from experiments.galactic_exodus import integrated_play
from experiments.galactic_exodus.srs import model as srs_model


class IntegratedPlayCliTests(unittest.TestCase):
    def run_cli(self, argv: list[str], stdin_text: str) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        exit_code = integrated_play.main(
            argv,
            stdin=io.StringIO(stdin_text),
            stdout=stdout,
            stderr=stderr,
        )
        return exit_code, stdout.getvalue(), stderr.getvalue()

    def test_script_entrypoint_starts_with_seed_42(self) -> None:
        result = subprocess.run(
            ["python", "experiments/galactic_exodus/integrated_play.py", "--seed", "42"],
            cwd=Path(__file__).resolve().parents[2],
            input="Q\n",
            text=True,
            capture_output=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stderr, "")
        self.assertIn("RESULT\n", result.stdout)
        self.assertIn("GAME  started seed=42", result.stdout)
        self.assertIn("LRS\n", result.stdout)
        self.assertIn("SRS\n", result.stdout)
        self.assertIn("HUD\n", result.stdout)
        self.assertTrue(result.stdout.endswith("COMMAND> "))

    def test_missing_seed_prints_help_without_starting_game(self) -> None:
        exit_code, stdout, stderr = self.run_cli([], "")

        self.assertEqual(exit_code, 0)
        self.assertEqual(stdout, "")
        self.assertIn("--seed", stderr)

    def test_invalid_seed_exits_with_argparse_error(self) -> None:
        exit_code, stdout, stderr = self.run_cli(["--seed", "oops"], "")

        self.assertEqual(exit_code, 2)
        self.assertEqual(stdout, "")
        self.assertIn("invalid int value", stderr)

    def test_parser_normalizes_commands(self) -> None:
        self.assertEqual(
            integrated_play.parse_integrated_command(" e "),
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_MOVE, directions=("E",), raw="E"),
        )
        self.assertEqual(
            integrated_play.parse_integrated_command("move e,e,n"),
            integrated_play.IntegratedCommand(
                kind=integrated_play.COMMAND_MOVE,
                directions=("E", "E", "N"),
                raw="MOVE E E N",
            ),
        )
        self.assertEqual(
            integrated_play.parse_integrated_command("move e e n"),
            integrated_play.IntegratedCommand(
                kind=integrated_play.COMMAND_MOVE,
                directions=("E", "E", "N"),
                raw="MOVE E E N",
            ),
        )
        self.assertEqual(
            integrated_play.parse_integrated_command("exit e"),
            integrated_play.IntegratedCommand(
                kind=integrated_play.COMMAND_EXIT,
                directions=("E",),
                raw="EXIT E",
            ),
        )
        self.assertEqual(
            integrated_play.parse_integrated_command("quit"),
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_QUIT, raw="QUIT"),
        )
        self.assertEqual(
            integrated_play.parse_integrated_command("foo"),
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_UNKNOWN, raw="FOO"),
        )

    def test_create_integrated_game_creates_lrs_and_srs(self) -> None:
        state = integrated_play.create_integrated_game(42)

        self.assertIsNotNone(state.lrs_state)
        self.assertIsNotNone(state.srs_state)
        self.assertEqual(state.srs_state.player_position, srs_model.Position(4, 4))
        self.assertEqual(state.last_event_summary, "GAME  started seed=42")

    def test_look_does_not_change_state(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_LOOK),
        )

        self.assertTrue(result.accepted)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)

    def test_status_does_not_change_state(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_STATUS),
        )

        self.assertTrue(result.accepted)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)

    def test_help_does_not_change_state_and_lists_future_commands(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_HELP),
        )

        self.assertTrue(result.accepted)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("N/E/S/W", result.summary_lines[0])
        self.assertIn("EXIT <dir>", result.summary_lines[0])

    def test_unknown_command_rejected_without_crash(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.IntegratedCommand(kind=integrated_play.COMMAND_UNKNOWN, raw="FOO"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_UNKNOWN)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("COMMAND rejected: unknown command", result.summary_lines[0])

    def test_move_currently_rejected_without_changing_positions(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("E"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_MOVE)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("movement is not implemented", result.summary_lines[0])

    def test_exit_currently_rejected_without_changing_positions(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_EXIT)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("exit is not implemented", result.summary_lines[0])

    def test_interact_currently_rejected_without_changing_positions(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("INTERACT"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_INTERACT)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("interaction is not implemented", result.summary_lines[0])

    def test_response_section_order(self) -> None:
        state = integrated_play.create_integrated_game(42)
        result = integrated_play.IntegratedCommandResult(
            accepted=True,
            command_type="INIT",
            summary_lines=("GAME  started seed=42",),
        )

        rendered = integrated_play.render_integrated_response(state, result)

        result_index = rendered.find("RESULT\n")
        lrs_index = rendered.find("\nLRS\n")
        srs_index = rendered.find("\nSRS\n")
        hud_index = rendered.find("\nHUD\n")
        self.assertTrue(result_index < lrs_index < srs_index < hud_index)

    def test_readme_mentions_integrated_cli(self) -> None:
        readme = Path("experiments/galactic_exodus/README.md").read_text(encoding="utf-8")

        self.assertIn("integrated_play.py", readme)
        self.assertIn("python experiments/galactic_exodus/integrated_play.py --seed 42", readme)

    def test_initial_display_snapshot_contains_expected_sections_in_order(self) -> None:
        _, stdout, _ = self.run_cli(["--seed", "42"], "Q\n")

        expected_fragments = [
            "RESULT\n",
            "GAME  started seed=42",
            "LRS\n",
            "  +---+---+---+---+---+---+---+---+\n",
            "SRS\n",
            " 9  ",
            "HUD\n",
            "SECTOR",
            "COMMAND> ",
        ]
        current_index = -1
        for fragment in expected_fragments:
            next_index = stdout.find(fragment, current_index + 1)
            self.assertNotEqual(next_index, -1, fragment)
            self.assertGreater(next_index, current_index, fragment)
            current_index = next_index
