import json
import unittest
from unittest.mock import patch

from experiments.galactic_exodus import engine
from experiments.galactic_exodus import simulate


def filled_cells(symbol: str = ".") -> simulate.Cells:
    return {
        (x, y): symbol
        for y in range(1, simulate.HEIGHT + 1)
        for x in range(1, simulate.WIDTH + 1)
    }


def make_actual_map(
    *,
    cells: simulate.Cells,
    base_position: simulate.Position = (4, 4),
    resource_positions: tuple[simulate.Position, ...] = (),
    rift_edges: tuple[simulate.Edge, ...] = (),
) -> engine.ActualMap:
    map_cells = dict(cells)
    map_cells[simulate.SPECIAL_S] = "S"
    map_cells[simulate.SPECIAL_H] = "H"
    map_cells[base_position] = "B"
    for position in resource_positions:
        map_cells[position] = "R"
    return engine.ActualMap(
        cells=map_cells,
        rift_edges=rift_edges,
        base_position=base_position,
        resource_positions=resource_positions,
    )


def make_state(
    *,
    actual_map: engine.ActualMap,
    settings: engine.GameSettings | None = None,
    player_position: simulate.Position = simulate.SPECIAL_S,
    remaining_fuel: int = 16,
    known_cells: dict[simulate.Position, str] | None = None,
    visited_cells: set[simulate.Position] | None = None,
    known_routes: dict[simulate.Edge, str] | None = None,
    used_resource_positions: set[simulate.Position] | None = None,
    base_visit_count: int = 0,
    base_refuel_count: int = 0,
    resource_visit_count: int = 0,
    resource_refuel_count: int = 0,
    last_supply_source: engine.SupplySource | None = None,
    turn_count: int = 0,
    requested_seed: int = 1,
    effective_seed: int = 1,
    reroll_count: int = 0,
    path: list[simulate.Position] | None = None,
) -> engine.GameState:
    effective_settings = settings or engine.DEFAULT_SETTINGS
    state = engine.GameState(
        settings=effective_settings,
        actual_map=actual_map,
        known_cells=(
            {
                simulate.SPECIAL_S: actual_map.cells[simulate.SPECIAL_S],
                simulate.SPECIAL_H: actual_map.cells[simulate.SPECIAL_H],
            }
            if known_cells is None
            else dict(known_cells)
        ),
        visited_cells={player_position} if visited_cells is None else set(visited_cells),
        known_routes={} if known_routes is None else dict(known_routes),
        player_position=player_position,
        remaining_fuel=remaining_fuel,
        used_resource_positions=set() if used_resource_positions is None else set(used_resource_positions),
        base_visit_count=base_visit_count,
        base_refuel_count=base_refuel_count,
        resource_visit_count=resource_visit_count,
        resource_refuel_count=resource_refuel_count,
        last_supply_source=last_supply_source,
        turn_count=turn_count,
        game_status=engine.GAME_STATUS_IN_PROGRESS,
        requested_seed=requested_seed,
        effective_seed=effective_seed,
        reroll_count=reroll_count,
        path=[player_position] if path is None else list(path),
    )
    state.game_status = engine.determine_game_status(state)
    return state


def start_neighborhood_known_cells(
    actual_map: engine.ActualMap,
) -> dict[simulate.Position, str]:
    return {
        (1, 1): actual_map.cells[(1, 1)],
        (1, 2): actual_map.cells[(1, 2)],
        (2, 1): actual_map.cells[(2, 1)],
        (2, 2): actual_map.cells[(2, 2)],
        simulate.SPECIAL_H: actual_map.cells[simulate.SPECIAL_H],
    }


class CreateGameTests(unittest.TestCase):
    def test_game_settings_rejects_initial_fuel_above_max_fuel(self) -> None:
        with self.assertRaisesRegex(ValueError, "initial-fuel must be less than or equal to max-fuel"):
            engine.GameSettings(initial_fuel=17, max_fuel=16).validate()

    def test_create_game_reveals_start_neighborhood_and_home(self) -> None:
        custom_map = simulate.GalacticMap(
            seed=42,
            resource_count=1,
            rift_density=0.10,
            b_position=(4, 4),
            r_positions=[(5, 5)],
            rift_edges=(),
            cells=make_actual_map(
                cells=filled_cells("."),
                base_position=(4, 4),
                resource_positions=((5, 5),),
            ).cells,
        )
        with patch.object(
            engine,
            "create_playable_map",
            return_value=(custom_map, 42, 0),
        ):
            state = engine.create_game(42)

        self.assertEqual(state.player_position, simulate.SPECIAL_S)
        self.assertEqual(
            state.known_cells,
            {
                (1, 1): "S",
                (1, 2): ".",
                (2, 1): ".",
                (2, 2): ".",
                simulate.SPECIAL_H: "H",
            },
        )
        self.assertEqual(state.visited_cells, {simulate.SPECIAL_S})
        self.assertEqual(state.known_routes, {})
        self.assertEqual(state.turn_count, 0)
        self.assertEqual(state.remaining_fuel, engine.DEFAULT_SETTINGS.initial_fuel)
        self.assertEqual(state.path, [simulate.SPECIAL_S])

    def test_create_game_from_actual_map_reveals_start_neighborhood_and_goal(self) -> None:
        actual_map = make_actual_map(cells=filled_cells("."), resource_positions=((5, 5),))

        state = engine.create_game_from_actual_map(
            actual_map,
            requested_seed=10,
            effective_seed=12,
            reroll_count=2,
        )

        self.assertEqual(
            state.known_cells,
            {
                (1, 1): "S",
                (1, 2): ".",
                (2, 1): ".",
                (2, 2): ".",
                (8, 8): "H",
            },
        )
        self.assertEqual(state.requested_seed, 10)
        self.assertEqual(state.effective_seed, 12)
        self.assertEqual(state.reroll_count, 2)

    def test_create_game_preserves_seed_compatibility_for_first_reachable_candidate(self) -> None:
        for seed in range(1, 1001):
            state = engine.create_game(seed)
            self.assertEqual(state.actual_map.cells, simulate.generate_map(state.effective_seed, 3, 0.10).cells)
            self.assertEqual(state.actual_map.rift_edges, simulate.generate_map(state.effective_seed, 3, 0.10).rift_edges)
            self.assertEqual(state.effective_seed, seed + state.reroll_count)
            self.assertGreaterEqual(state.reroll_count, 0)
            self.assertLess(state.reroll_count, engine.MAX_GENERATION_ATTEMPTS)

    def test_create_playable_map_rerolls_until_reachable_candidate(self) -> None:
        unreachable = simulate.GalacticMap(
            seed=10,
            resource_count=0,
            rift_density=0.10,
            b_position=(4, 4),
            r_positions=[],
            rift_edges=(),
            cells=filled_cells("."),
        )
        reachable = simulate.generate_map(12, 3, 0.10)
        generated = {
            10: unreachable,
            11: unreachable,
            12: reachable,
        }
        with (
            patch.object(engine.simulate, "generate_map", side_effect=lambda seed, *_: generated[seed]),
            patch.object(engine, "is_goal_reachable", side_effect=lambda galactic_map: galactic_map.seed == 12),
        ):
            galactic_map, effective_seed, reroll_count = engine.create_playable_map(10, engine.DEFAULT_SETTINGS)

        self.assertEqual(galactic_map.seed, 12)
        self.assertEqual(effective_seed, 12)
        self.assertEqual(reroll_count, 2)

    def test_seed_overflow_raises_explicit_generation_error(self) -> None:
        with self.assertRaises(engine.GenerationError) as ctx:
            engine.add_seed_offset(engine.MAX_INT64, 1)

        self.assertEqual(ctx.exception.reason, "SEED_OVERFLOW")
        self.assertEqual(ctx.exception.requested_seed, engine.MAX_INT64)
        self.assertEqual(ctx.exception.attempts, 2)
        self.assertEqual(ctx.exception.last_candidate_seed, engine.MAX_INT64)

    def test_exhausted_generation_attempts_raise_generation_error(self) -> None:
        reachable = simulate.generate_map(1, 3, 0.10)
        with (
            patch.object(engine.simulate, "generate_map", return_value=reachable),
            patch.object(engine, "is_goal_reachable", return_value=False),
        ):
            with self.assertRaises(engine.GenerationError) as ctx:
                engine.create_playable_map(1, engine.DEFAULT_SETTINGS)

        self.assertEqual(ctx.exception.reason, "NO_REACHABLE_MAP")
        self.assertEqual(ctx.exception.attempts, engine.MAX_GENERATION_ATTEMPTS)
        self.assertEqual(ctx.exception.last_candidate_seed, 100)

    def test_create_playable_map_uses_injected_generation_dependencies(self) -> None:
        generated = simulate.generate_map(99, 3, 0.10)

        galactic_map, effective_seed, reroll_count = engine.create_playable_map(
            10,
            engine.DEFAULT_SETTINGS,
            generate_candidate=lambda seed, *_: generated,
            is_reachable=lambda _: True,
        )

        self.assertEqual(galactic_map, generated)
        self.assertEqual(effective_seed, 10)
        self.assertEqual(reroll_count, 0)

    def test_validate_actual_map_rejects_mismatched_base_metadata(self) -> None:
        cells = filled_cells(".")
        cells[(1, 1)] = "S"
        cells[(8, 8)] = "H"
        cells[(3, 3)] = "B"
        actual_map = engine.ActualMap(
            cells=cells,
            rift_edges=(),
            base_position=(4, 4),
            resource_positions=(),
        )

        with self.assertRaisesRegex(ValueError, "base_position cell must be 'B'"):
            engine.validate_actual_map(actual_map, engine.DEFAULT_SETTINGS)


class ApplyCommandTests(unittest.TestCase):
    def test_move_consumes_destination_terrain_cost_and_discovers_cell(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        actual_map = make_actual_map(cells=cells)
        state = make_state(
            actual_map=actual_map,
            remaining_fuel=10,
            known_cells=start_neighborhood_known_cells(actual_map),
        )

        event = engine.apply_command(state, "E")

        self.assertEqual(event.outcome, engine.OUTCOME_MOVED)
        self.assertEqual(event.fuel_spent, 3)
        self.assertEqual(
            event.discovered_cells,
            (
                engine.DiscoveredCell(position=(3, 1), symbol="."),
                engine.DiscoveredCell(position=(3, 2), symbol="."),
            ),
        )
        self.assertEqual(state.player_position, (2, 1))
        self.assertEqual(state.remaining_fuel, 7)
        self.assertEqual(state.turn_count, 1)
        self.assertEqual(state.known_routes[simulate.normalize_edge((1, 1), (2, 1))], engine.ROUTE_OPEN)
        self.assertEqual(state.known_cells[(2, 1)], "A")

    def test_move_reveals_centered_three_by_three_without_losing_known_cells(self) -> None:
        cells = filled_cells(".")
        cells[(2, 2)] = "B"
        cells[(3, 2)] = "R"
        cells[(3, 1)] = "A"
        state = make_state(
            actual_map=make_actual_map(cells=cells, base_position=(2, 2), resource_positions=((3, 2),)),
            player_position=(2, 1),
            known_cells={
                (1, 1): "S",
                (1, 2): ".",
                (2, 1): ".",
                (2, 2): "B",
                simulate.SPECIAL_H: "H",
            },
            visited_cells={(1, 1), (2, 1)},
            path=[(1, 1), (2, 1)],
            remaining_fuel=10,
        )

        event = engine.apply_command(state, "N")

        self.assertEqual(event.outcome, engine.OUTCOME_MOVED)
        self.assertEqual(
            event.discovered_cells,
            (
                engine.DiscoveredCell(position=(1, 3), symbol="."),
                engine.DiscoveredCell(position=(2, 3), symbol="."),
                engine.DiscoveredCell(position=(3, 1), symbol="A"),
                engine.DiscoveredCell(position=(3, 2), symbol="R"),
                engine.DiscoveredCell(position=(3, 3), symbol="."),
            ),
        )
        self.assertEqual(state.known_cells[(1, 1)], "S")
        self.assertEqual(state.known_cells[(3, 2)], "R")
        self.assertEqual(state.known_cells[(3, 1)], "A")

    def test_unknown_rift_consumes_one_fuel_and_known_rift_retry_is_rejected(self) -> None:
        rift_edge = (simulate.normalize_edge((1, 1), (1, 2)),)
        state = make_state(actual_map=make_actual_map(cells=filled_cells("."), rift_edges=rift_edge), remaining_fuel=3)

        first = engine.apply_command(state, "N")
        second = engine.apply_command(state, "N")

        self.assertEqual(first.outcome, engine.OUTCOME_BLOCKED_UNKNOWN_RIFT)
        self.assertTrue(first.discovered_rift)
        self.assertEqual(first.fuel_after, 2)
        self.assertIsNone(first.required_fuel)
        self.assertEqual(first.turn, 1)
        self.assertEqual(first.discovered_cells, ())
        self.assertEqual(second.outcome, engine.OUTCOME_REJECTED_KNOWN_RIFT)
        self.assertEqual(second.fuel_after, 2)
        self.assertIsNone(second.required_fuel)
        self.assertEqual(second.turn, 1)
        self.assertEqual(second.discovered_cells, ())
        self.assertEqual(state.rift_attempt_count, 1)

    def test_base_refuels_to_max_on_first_visit_and_revisit(self) -> None:
        settings = engine.GameSettings(initial_fuel=3, max_fuel=5)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), base_position=(2, 1)),
            settings=settings,
            remaining_fuel=3,
        )

        first = engine.apply_command(state, "E")
        engine.apply_command(state, "W")
        second = engine.apply_command(state, "E")

        self.assertEqual(first.supply_result, engine.SUPPLY_RESULT_BASE_REFUELED)
        self.assertEqual(first.supply_source, engine.SupplySource(kind="B", position=(2, 1)))
        self.assertEqual(first.fuel_before_supply, 2)
        self.assertEqual(first.fuel_after_supply, 5)
        self.assertEqual(first.supply_amount, 3)
        self.assertEqual(second.supply_result, engine.SUPPLY_RESULT_BASE_REFUELED)
        self.assertEqual(second.fuel_before_supply, 4)
        self.assertEqual(second.fuel_after_supply, 5)
        self.assertEqual(state.remaining_fuel, 5)
        self.assertEqual(state.base_visit_count, 2)
        self.assertEqual(state.base_refuel_count, 2)
        self.assertEqual(state.last_supply_source, engine.SupplySource(kind="B", position=(2, 1)))

    def test_base_arrival_while_already_full_only_increments_visit_count(self) -> None:
        settings = engine.GameSettings(initial_fuel=4, max_fuel=4)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), base_position=(2, 1)),
            settings=settings,
            remaining_fuel=4,
            last_supply_source=engine.SupplySource(kind="R", position=(7, 7)),
        )

        event = engine.apply_base_supply(state, (2, 1))

        self.assertEqual(event.result, engine.SUPPLY_RESULT_BASE_ALREADY_FULL)
        self.assertEqual(event.supply_amount, 0)
        self.assertEqual(state.remaining_fuel, 4)
        self.assertEqual(state.base_visit_count, 1)
        self.assertEqual(state.base_refuel_count, 0)
        self.assertEqual(state.last_supply_source, engine.SupplySource(kind="R", position=(7, 7)))

    def test_resource_supply_can_apply_after_arriving_with_zero_fuel(self) -> None:
        settings = engine.GameSettings(initial_fuel=1, max_fuel=16, resource_supply=5)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((2, 1),)),
            settings=settings,
            remaining_fuel=1,
        )

        event = engine.apply_command(state, "E")

        self.assertEqual(event.supply_result, engine.SUPPLY_RESULT_RESOURCE_REFUELED)
        self.assertEqual(event.supply_source, engine.SupplySource(kind="R", position=(2, 1)))
        self.assertEqual(state.remaining_fuel, 5)
        self.assertEqual(state.used_resource_positions, {(2, 1)})
        self.assertEqual(state.resource_visit_count, 1)
        self.assertEqual(state.resource_refuel_count, 1)

    def test_resource_supply_is_capped_by_max_fuel(self) -> None:
        settings = engine.GameSettings(initial_fuel=16, max_fuel=16, resource_supply=5)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((2, 1),)),
            settings=settings,
            remaining_fuel=14,
        )

        event = engine.apply_command(state, "E")

        self.assertEqual(event.supply_result, engine.SUPPLY_RESULT_RESOURCE_REFUELED)
        self.assertEqual(event.supply_amount, 3)
        self.assertEqual(event.fuel_before_supply, 13)
        self.assertEqual(event.fuel_after_supply, 16)
        self.assertEqual(state.remaining_fuel, 16)

    def test_unused_resource_is_not_consumed_when_arriving_already_full(self) -> None:
        settings = engine.GameSettings(initial_fuel=16, max_fuel=16, resource_supply=5)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((2, 1),)),
            settings=settings,
            remaining_fuel=16,
            last_supply_source=engine.SupplySource(kind="B", position=(4, 4)),
        )

        event = engine.apply_resource_supply(state, (2, 1))

        self.assertEqual(event.result, engine.SUPPLY_RESULT_RESOURCE_ALREADY_FULL)
        self.assertEqual(event.supply_amount, 0)
        self.assertEqual(state.used_resource_positions, set())
        self.assertEqual(state.resource_visit_count, 1)
        self.assertEqual(state.resource_refuel_count, 0)
        self.assertEqual(state.last_supply_source, engine.SupplySource(kind="B", position=(4, 4)))

    def test_used_resource_can_be_revisited_without_refueling(self) -> None:
        settings = engine.GameSettings(initial_fuel=4, max_fuel=16, resource_supply=5)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((2, 1),)),
            settings=settings,
            remaining_fuel=4,
        )

        first = engine.apply_command(state, "E")
        engine.apply_command(state, "W")
        second = engine.apply_command(state, "E")

        self.assertEqual(first.supply_result, engine.SUPPLY_RESULT_RESOURCE_REFUELED)
        self.assertEqual(second.supply_result, engine.SUPPLY_RESULT_RESOURCE_ALREADY_USED)
        self.assertEqual(second.supply_amount, 0)
        self.assertEqual(state.used_resource_positions, {(2, 1)})
        self.assertEqual(state.resource_visit_count, 2)
        self.assertEqual(state.resource_refuel_count, 1)

    def test_last_supply_source_survives_later_movement(self) -> None:
        settings = engine.GameSettings(initial_fuel=4, resource_supply=5)
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), resource_positions=((2, 1),)),
            settings=settings,
            remaining_fuel=4,
        )

        engine.apply_command(state, "E")
        engine.apply_command(state, "W")

        self.assertEqual(state.last_supply_source, engine.SupplySource(kind="R", position=(2, 1)))

    def test_arriving_at_home_with_zero_fuel_is_a_win(self) -> None:
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells(".")),
            player_position=(7, 8),
            remaining_fuel=1,
            visited_cells={(7, 8)},
            path=[(7, 8)],
        )

        event = engine.apply_command(state, "E")

        self.assertEqual(event.status_after, engine.GAME_STATUS_WON)
        self.assertEqual(state.player_position, simulate.SPECIAL_H)
        self.assertEqual(state.remaining_fuel, 0)

    def test_invalid_out_of_bounds_and_insufficient_fuel_leave_state_unchanged(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        state = make_state(actual_map=make_actual_map(cells=cells), remaining_fuel=2)
        snapshot = (
            state.player_position,
            state.remaining_fuel,
            state.turn_count,
            dict(state.known_cells),
            dict(state.known_routes),
        )

        invalid = engine.apply_command(state, "X")
        out_of_bounds = engine.apply_command(state, "W")
        insufficient = engine.apply_command(state, "E")

        self.assertEqual(invalid.outcome, engine.OUTCOME_INVALID_COMMAND)
        self.assertEqual(out_of_bounds.outcome, engine.OUTCOME_OUT_OF_BOUNDS)
        self.assertEqual(insufficient.outcome, engine.OUTCOME_REJECTED_INSUFFICIENT_FUEL)
        self.assertIsNone(invalid.required_fuel)
        self.assertIsNone(out_of_bounds.required_fuel)
        self.assertEqual(insufficient.required_fuel, 3)
        self.assertEqual(invalid.discovered_cells, ())
        self.assertEqual(out_of_bounds.discovered_cells, ())
        self.assertEqual(insufficient.discovered_cells, ())
        self.assertEqual(
            snapshot,
            (
                state.player_position,
                state.remaining_fuel,
                state.turn_count,
                state.known_cells,
                state.known_routes,
            ),
        )

    def test_actual_map_controls_lost_fuel_detection(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        blocked = (simulate.normalize_edge((1, 1), (1, 2)),)
        state = make_state(
            actual_map=make_actual_map(cells=cells, rift_edges=blocked),
            remaining_fuel=1,
        )

        self.assertEqual(state.game_status, engine.GAME_STATUS_LOST_FUEL)


class RunCommandsTests(unittest.TestCase):
    def test_run_state_commands_preserves_existing_state_metadata(self) -> None:
        state = engine.create_game_from_actual_map(
            make_actual_map(cells=filled_cells(".")),
            requested_seed=5,
            effective_seed=7,
            reroll_count=2,
        )

        log = engine.run_state_commands(state, ["E"], max_turns=10)

        self.assertEqual(log.requested_seed, 5)
        self.assertEqual(log.effective_seed, 7)
        self.assertEqual(log.reroll_count, 2)
        self.assertEqual(log.events[0].turn, 1)

    def test_run_commands_is_deterministic_for_same_seed_and_commands(self) -> None:
        commands = ["E", "N", "N", "W", "S"]

        first = engine.run_commands(42, commands)
        second = engine.run_commands(42, commands)

        self.assertEqual(first.to_dict(), second.to_dict())
        self.assertEqual(first.to_json(), second.to_json())

    def test_run_commands_aborts_when_commands_run_out(self) -> None:
        log = engine.run_commands(42, [])

        self.assertEqual(log.final_summary.outcome, engine.FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION)
        self.assertEqual(log.events, ())
        self.assertIsNone(log.generation_error)

    def test_run_commands_aborts_on_turn_limit(self) -> None:
        settings = engine.GameSettings(initial_fuel=100, max_fuel=100)
        log = engine.run_commands(42, ["E"] * 100, settings=settings, max_turns=1)

        self.assertEqual(log.final_summary.outcome, engine.FINAL_OUTCOME_ABORTED_TURN_LIMIT)
        self.assertEqual(log.final_summary.turn_count, 1)

    def test_run_commands_reports_generation_error_separately(self) -> None:
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
            log = engine.run_commands(99, ["E"])

        self.assertIsNone(log.final_summary)
        self.assertEqual(log.generation_error.kind, "GENERATION_ERROR")
        self.assertEqual(log.generation_error.reason, "NO_REACHABLE_MAP")
        self.assertEqual(log.generation_error.requested_seed, 99)
        self.assertEqual(log.generation_error.attempts, 100)
        self.assertEqual(log.generation_error.last_candidate_seed, 198)

    def test_game_log_schema_and_summary_are_stable(self) -> None:
        log = engine.run_commands(42, ["E", "N"])
        payload = log.to_dict()

        self.assertEqual(
            list(payload.keys()),
            [
                "schema_version",
                "settings",
                "requested_seed",
                "effective_seed",
                "reroll_count",
                "initial_state",
                "events",
                "final_summary",
                "generation_error",
            ],
        )
        self.assertEqual(payload["schema_version"], 3)
        self.assertIn("outcome", payload["final_summary"])
        self.assertIn("path", payload["final_summary"])
        self.assertIn("last_supply_source", payload["final_summary"])
        self.assertIn("used_resource_positions", payload["final_summary"])
        if payload["events"]:
            self.assertIn("supply_source", payload["events"][0])
            self.assertIn("supply_result", payload["events"][0])
            self.assertIn("fuel_before_supply", payload["events"][0])
            self.assertIn("fuel_after_supply", payload["events"][0])
            self.assertIn("supply_amount", payload["events"][0])
            self.assertIn("required_fuel", payload["events"][0])
            self.assertIn("discovered_cells", payload["events"][0])
        json.loads(log.to_json())

    def test_game_log_serializes_structured_last_supply_source(self) -> None:
        state = make_state(
            actual_map=make_actual_map(cells=filled_cells("."), base_position=(2, 1)),
            settings=engine.GameSettings(initial_fuel=1, max_fuel=4, resource_count=0),
            remaining_fuel=1,
            requested_seed=1,
            effective_seed=1,
        )
        with patch.object(engine, "create_game", return_value=state):
            payload = engine.run_commands(1, ["E"], settings=state.settings).to_dict()

        self.assertEqual(
            payload["events"][0]["supply_source"],
            {
                "kind": "B",
                "position": {"x": 2, "y": 1},
            },
        )
        self.assertEqual(
            payload["final_summary"]["last_supply_source"],
            {
                "kind": "B",
                "position": {"x": 2, "y": 1},
            },
        )

    def test_supply_helpers_cover_all_supply_results(self) -> None:
        cells = filled_cells(".")
        actual_map = make_actual_map(
            cells=cells,
            base_position=(2, 1),
            resource_positions=((3, 1),),
        )
        settings = engine.GameSettings(initial_fuel=4, max_fuel=4, resource_supply=5)

        base_full_state = make_state(
            actual_map=actual_map,
            settings=settings,
            remaining_fuel=4,
        )
        resource_full_state = make_state(
            actual_map=actual_map,
            settings=settings,
            player_position=(2, 1),
            visited_cells={(1, 1), (2, 1)},
            path=[(1, 1), (2, 1)],
            remaining_fuel=4,
            base_visit_count=1,
        )
        resource_used_state = make_state(
            actual_map=actual_map,
            settings=settings,
            player_position=(2, 1),
            visited_cells={(1, 1), (2, 1)},
            path=[(1, 1), (2, 1)],
            remaining_fuel=2,
            used_resource_positions={(3, 1)},
            base_visit_count=1,
        )

        self.assertEqual(engine.apply_base_supply(base_full_state, (2, 1)).result, engine.SUPPLY_RESULT_BASE_ALREADY_FULL)
        self.assertEqual(
            engine.apply_resource_supply(resource_full_state, (3, 1)).result,
            engine.SUPPLY_RESULT_RESOURCE_ALREADY_FULL,
        )
        self.assertEqual(
            engine.apply_resource_supply(resource_used_state, (3, 1)).result,
            engine.SUPPLY_RESULT_RESOURCE_ALREADY_USED,
        )

    def test_game_log_serializes_structured_generation_error(self) -> None:
        with patch.object(
            engine,
            "create_game",
            side_effect=engine.GenerationError(
                requested_seed=7,
                attempts=2,
                last_candidate_seed=7,
                reason="SEED_OVERFLOW",
                message="overflow",
            ),
        ):
            payload = engine.run_commands(7, ["E"]).to_dict()

        self.assertEqual(
            payload["generation_error"],
            {
                "kind": "GENERATION_ERROR",
                "requested_seed": 7,
                "attempts": 2,
                "last_candidate_seed": 7,
                "reason": "SEED_OVERFLOW",
                "message": "overflow",
            },
        )

    def test_game_log_serializes_all_discovered_cells_in_deterministic_order(self) -> None:
        cells = filled_cells(".")
        cells[(2, 1)] = "A"
        actual_map = make_actual_map(cells=cells)
        state = make_state(
            actual_map=actual_map,
            remaining_fuel=10,
            known_cells=start_neighborhood_known_cells(actual_map),
        )
        with patch.object(engine, "create_game", return_value=state):
            payload = engine.run_commands(1, ["E"]).to_dict()

        self.assertEqual(
            payload["events"][0]["discovered_cells"],
            [
                {"position": {"x": 3, "y": 1}, "symbol": "."},
                {"position": {"x": 3, "y": 2}, "symbol": "."},
            ],
        )


if __name__ == "__main__":
    unittest.main()
