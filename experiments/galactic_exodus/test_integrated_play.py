from __future__ import annotations

import io
from dataclasses import replace
from pathlib import Path
import subprocess
import unittest

from experiments.galactic_exodus import engine as lrs_engine
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
            integrated_play.parse_integrated_command("exit x"),
            integrated_play.IntegratedCommand(
                kind=integrated_play.COMMAND_EXIT,
                directions=("X",),
                raw="EXIT X",
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

    def test_direction_command_moves_only_srs(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("E"),
        )

        self.assertTrue(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_MOVE)
        self.assertFalse(result.changed_lrs_position)
        self.assertTrue(result.changed_srs_position)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, srs_model.Position(old_srs.x + 1, old_srs.y))
        self.assertTrue(result.summary_lines[0].startswith("MOVE  accepted"))

    def test_repeated_direction_commands_never_move_lrs(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position

        first = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("E"),
        )
        second = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("E"),
        )

        self.assertTrue(first.accepted)
        self.assertTrue(second.accepted)
        self.assertEqual(state.lrs_state.player_position, old_lrs)

    def test_move_route_comma_syntax_moves_only_srs(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("MOVE E,E,N"),
        )

        self.assertTrue(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_MOVE)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, srs_model.Position(6, 5))
        self.assertTrue(result.summary_lines[0].startswith("MOVE  accepted"))

    def test_move_route_whitespace_syntax_moves_only_srs(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("MOVE E E N"),
        )

        self.assertTrue(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_MOVE)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, srs_model.Position(6, 5))
        self.assertTrue(result.summary_lines[0].startswith("MOVE  accepted"))

    def test_move_invalid_direction_rejected_without_changing_positions(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("MOVE X"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_MOVE)
        self.assertFalse(result.changed_lrs_position)
        self.assertFalse(result.changed_srs_position)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("invalid direction", result.summary_lines[0])

    def test_hud_last_updates_after_move(self) -> None:
        state = integrated_play.create_integrated_game(42)

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("E"),
        )
        rendered = integrated_play.render_integrated_response(state, result)

        self.assertIn("LAST    MOVE  accepted", rendered)

    def test_exit_rejected_without_matching_warp_flag(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(4, 4))

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_EXIT)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, srs_model.Position(4, 4))
        self.assertEqual(old_srs, srs_model.Position(4, 4))
        self.assertIn("no E warp point", result.summary_lines[0])

    def test_exit_accepted_moves_lrs_east_and_enters_new_srs_from_west(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.lrs_state.player_position = (1, 1)
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(8, 4))

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )

        self.assertTrue(result.accepted)
        self.assertTrue(result.changed_lrs_position)
        self.assertTrue(result.changed_srs_position)
        self.assertEqual(state.lrs_state.player_position, (2, 1))
        self.assertEqual(state.srs_state.player_position, srs_model.Position(0, 4))
        self.assertIn("EXIT  E accepted from SRS=(9,5)", result.summary_lines[0])
        self.assertIn("LRS   moved E: LRS=(1,1) -> LRS=(2,1)", result.summary_lines[1])
        self.assertIn("SRS   entered sector TYPE=", result.summary_lines[2])
        self.assertIn("at SRS=(1,5)", result.summary_lines[2])

    def test_exit_out_of_bounds_rejected(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.lrs_state.player_position = (8, 8)
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(8, 4))

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(state.lrs_state.player_position, (8, 8))
        self.assertEqual(state.srs_state.player_position, srs_model.Position(8, 4))
        self.assertIn("would leave LRS map", result.summary_lines[0])

    def test_exit_known_rift_rejected(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.lrs_state.player_position = (1, 1)
        blocked_edge = lrs_engine.simulate.normalize_edge((1, 1), (2, 1))
        state.lrs_state.known_routes[blocked_edge] = lrs_engine.ROUTE_RIFT
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(8, 4))

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(state.lrs_state.player_position, (1, 1))
        self.assertEqual(state.srs_state.player_position, srs_model.Position(8, 4))
        self.assertIn("edge is blocked by RIFT", result.summary_lines[0])

    def test_exit_invalid_direction_rejected_without_changing_positions(self) -> None:
        state = integrated_play.create_integrated_game(42)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT X"),
        )

        self.assertFalse(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_EXIT)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("invalid direction", result.summary_lines[0])

    def test_hud_last_updates_after_exit(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.lrs_state.player_position = (1, 1)
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(8, 4))

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )
        rendered = integrated_play.render_integrated_response(state, result)

        self.assertIn("LAST    SRS   entered sector TYPE=", rendered)

    def test_initial_srs_has_all_four_exit_warp_flags(self) -> None:
        state = integrated_play.create_integrated_game(42)

        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(0, 4)).warp_flags,
            frozenset({srs_model.Direction.W}),
        )
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(8, 4)).warp_flags,
            frozenset({srs_model.Direction.E}),
        )
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(4, 0)).warp_flags,
            frozenset({srs_model.Direction.S}),
        )
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(4, 8)).warp_flags,
            frozenset({srs_model.Direction.N}),
        )

    def test_new_srs_after_exit_keeps_all_four_exit_warp_flags(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.lrs_state.player_position = (1, 1)
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(8, 4))

        integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("EXIT E"),
        )

        self.assertEqual(state.srs_state.player_position, srs_model.Position(0, 4))
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(0, 4)).warp_flags,
            frozenset({srs_model.Direction.W}),
        )
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(8, 4)).warp_flags,
            frozenset({srs_model.Direction.E}),
        )
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(4, 0)).warp_flags,
            frozenset({srs_model.Direction.S}),
        )
        self.assertEqual(
            state.srs_state.actual_map.cell_at(srs_model.Position(4, 8)).warp_flags,
            frozenset({srs_model.Direction.N}),
        )

    def test_resource_sector_places_resource_cache_at_fixed_position(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.srs_state = integrated_play._create_minimal_srs_for_sector(
            seed=state.lrs_state.effective_seed,
            lrs_position=state.lrs_state.player_position,
            sector_symbol="R",
        )

        cache_cell = state.srs_state.actual_map.cell_at(srs_model.Position(4, 5))

        self.assertEqual(cache_cell.object_id, "resource-cache-1")
        self.assertEqual(
            state.srs_state.objects["resource-cache-1"].object_type,
            srs_model.SrsObjectType.RESOURCE_CACHE,
        )

    def test_base_sector_places_station_at_fixed_position(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.srs_state = integrated_play._create_minimal_srs_for_sector(
            seed=state.lrs_state.effective_seed,
            lrs_position=state.lrs_state.player_position,
            sector_symbol="B",
        )

        station_cell = state.srs_state.actual_map.cell_at(srs_model.Position(4, 5))

        self.assertEqual(station_cell.object_id, "station-1")
        self.assertEqual(
            state.srs_state.objects["station-1"].object_type,
            srs_model.SrsObjectType.STATION,
        )

    def test_interact_rejected_without_object_or_position_change(self) -> None:
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
        self.assertIn("no object", result.summary_lines[0])

    def test_resource_cache_interact_accepted_without_changing_lrs(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.srs_state = integrated_play._create_minimal_srs_for_sector(
            seed=state.lrs_state.effective_seed,
            lrs_position=state.lrs_state.player_position,
            sector_symbol="R",
        )
        state.srs_state = replace(
            state.srs_state,
            player_position=srs_model.Position(4, 5),
            fuel=3,
            max_fuel=9,
        )
        old_lrs = state.lrs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("INTERACT"),
        )
        rendered = integrated_play.render_integrated_response(state, result)

        self.assertTrue(result.accepted)
        self.assertEqual(result.command_type, integrated_play.COMMAND_INTERACT)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertIn("resource-cache-1", state.srs_state.persistent_state.consumed_object_ids)
        self.assertTrue(state.srs_state.objects["resource-cache-1"].consumed)
        self.assertTrue(any("CACHE acquired" in line for line in result.summary_lines))
        self.assertIn("LAST    CACHE acquired", rendered)

    def test_station_interact_accepted_without_changing_lrs(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.srs_state = integrated_play._create_minimal_srs_for_sector(
            seed=state.lrs_state.effective_seed,
            lrs_position=state.lrs_state.player_position,
            sector_symbol="B",
        )
        state.srs_state = replace(state.srs_state, fuel=2, max_fuel=9)
        old_lrs = state.lrs_state.player_position
        old_srs = state.srs_state.player_position

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("INTERACT"),
        )
        rendered = integrated_play.render_integrated_response(state, result)

        self.assertTrue(result.accepted)
        self.assertEqual(state.lrs_state.player_position, old_lrs)
        self.assertEqual(state.srs_state.player_position, old_srs)
        self.assertIn("station-1", state.srs_state.persistent_state.activated_object_ids)
        self.assertTrue(state.srs_state.objects["station-1"].activated)
        self.assertTrue(any("BASE station activated" in line for line in result.summary_lines))
        self.assertIn("LAST    BASE station activated", rendered)

    def test_repeated_resource_cache_interact_is_rejected_after_consumption(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.srs_state = integrated_play._create_minimal_srs_for_sector(
            seed=state.lrs_state.effective_seed,
            lrs_position=state.lrs_state.player_position,
            sector_symbol="R",
        )
        state.srs_state = replace(
            state.srs_state,
            player_position=srs_model.Position(4, 5),
            fuel=3,
            max_fuel=9,
        )

        first = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("INTERACT"),
        )
        second = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("INTERACT"),
        )

        self.assertTrue(first.accepted)
        self.assertFalse(second.accepted)
        self.assertIn("already consumed", second.summary_lines[0])

    def test_consumed_resource_cache_symbol_is_visible_after_player_moves_off_cell(self) -> None:
        state = integrated_play.create_integrated_game(42)
        state.srs_state = integrated_play._create_minimal_srs_for_sector(
            seed=state.lrs_state.effective_seed,
            lrs_position=state.lrs_state.player_position,
            sector_symbol="R",
        )
        state.srs_state = replace(
            state.srs_state,
            player_position=srs_model.Position(4, 5),
            fuel=3,
            max_fuel=9,
        )

        result = integrated_play.execute_integrated_command(
            state,
            integrated_play.parse_integrated_command("INTERACT"),
        )
        state.srs_state = self._replace_srs_position(state.srs_state, srs_model.Position(4, 4))
        rendered = integrated_play.render_integrated_response(state, result)

        self.assertIn(" 6  ? ? . . r . . ? ?", rendered)

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

    def _replace_srs_position(
        self,
        state: srs_model.SrsGameState,
        position: srs_model.Position,
    ) -> srs_model.SrsGameState:
        return srs_model.SrsGameState(
            descriptor=state.descriptor,
            actual_map=state.actual_map,
            known_state=state.known_state,
            persistent_state=state.persistent_state,
            player_position=position,
            objects=state.objects,
            combat_state=state.combat_state,
            srs_turn=state.srs_turn,
            fuel=state.fuel,
            max_fuel=state.max_fuel,
        )
