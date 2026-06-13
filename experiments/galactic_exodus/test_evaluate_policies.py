from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from experiments.galactic_exodus import engine
from experiments.galactic_exodus import evaluate_policies
from experiments.galactic_exodus import simulate
from experiments.galactic_exodus.test_engine import filled_cells
from experiments.galactic_exodus.test_engine import make_actual_map
from experiments.galactic_exodus.test_engine import make_state
from experiments.galactic_exodus.test_engine import start_neighborhood_known_cells


class PolicySelectionTests(unittest.TestCase):
    def test_goal_greedy_prefers_direction_order_on_equal_goal_distance(self) -> None:
        view = evaluate_policies.PolicyView(
            known_cells={},
            known_routes={},
            player_position=(4, 4),
            remaining_fuel=16,
            used_resource_positions=set(),
            goal_position=(5, 5),
        )

        self.assertEqual(evaluate_policies.choose_goal_greedy_action(view), "N")

    def test_goal_greedy_skips_out_of_bounds_and_known_rifts(self) -> None:
        edge = simulate.normalize_edge((1, 1), (1, 2))
        view = evaluate_policies.PolicyView(
            known_cells={},
            known_routes={edge: engine.ROUTE_RIFT},
            player_position=(1, 1),
            remaining_fuel=16,
            used_resource_positions=set(),
            goal_position=(8, 8),
        )

        self.assertEqual(evaluate_policies.choose_goal_greedy_action(view), "E")

    def test_supply_aware_targets_known_supply_when_fuel_is_low(self) -> None:
        view = evaluate_policies.PolicyView(
            known_cells={
                (4, 5): "B",
                (5, 4): "R",
            },
            known_routes={},
            player_position=(4, 4),
            remaining_fuel=8,
            used_resource_positions=set(),
            goal_position=(8, 8),
        )

        self.assertEqual(evaluate_policies.choose_supply_aware_action(view), "N")

    def test_supply_aware_ignores_used_resource_and_falls_back_to_goal_greedy(self) -> None:
        view = evaluate_policies.PolicyView(
            known_cells={
                (3, 4): "R",
            },
            known_routes={},
            player_position=(4, 4),
            remaining_fuel=7,
            used_resource_positions={(3, 4)},
            goal_position=(8, 8),
        )

        self.assertEqual(evaluate_policies.choose_supply_aware_action(view), "N")


class EvaluatePolicyRunTests(unittest.TestCase):
    def test_evaluate_policy_run_reports_generation_error_with_blank_numeric_fields(self) -> None:
        with patch.object(
            engine,
            "create_game",
            side_effect=engine.GenerationError(
                requested_seed=9,
                attempts=100,
                last_candidate_seed=108,
                reason="NO_REACHABLE_MAP",
                message="no map",
            ),
        ):
            run = evaluate_policies.evaluate_policy_run(
                evaluate_policies.POLICY_GOAL_GREEDY,
                9,
                max_turns=256,
            )

        self.assertEqual(run.outcome, "GENERATION_ERROR")
        self.assertEqual(run.to_csv_row()["effective_seed"], "")
        self.assertEqual(run.to_csv_row()["turn_count"], "")

    def test_evaluate_policy_run_aborts_when_no_candidate_action_exists(self) -> None:
        actual_map = make_actual_map(cells=filled_cells("."))
        state = make_state(
            actual_map=actual_map,
            known_routes={
                simulate.normalize_edge((1, 1), (1, 2)): engine.ROUTE_RIFT,
                simulate.normalize_edge((1, 1), (2, 1)): engine.ROUTE_RIFT,
            },
            known_cells=start_neighborhood_known_cells(actual_map),
        )
        with patch.object(engine, "create_game", return_value=state):
            run = evaluate_policies.evaluate_policy_run(
                evaluate_policies.POLICY_GOAL_GREEDY,
                1,
                max_turns=256,
            )

        self.assertEqual(run.outcome, engine.FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION)
        self.assertEqual(run.turn_count, 0)


class SummaryTests(unittest.TestCase):
    def test_summarize_policy_runs_computes_required_metrics(self) -> None:
        runs = [
            evaluate_policies.PolicyRun(
                policy=evaluate_policies.POLICY_GOAL_GREEDY,
                requested_seed=1,
                effective_seed=1,
                reroll_count=0,
                outcome="WON",
                turn_count=10,
                remaining_fuel=3,
                max_fuel=16,
                base_visit_count=0,
                base_refuel_count=0,
                resource_visit_count=0,
                resource_refuel_count=0,
                used_resource_count=0,
                rift_attempts=1,
                invalid_or_rejected_actions=0,
                path_length=10,
            ),
            evaluate_policies.PolicyRun(
                policy=evaluate_policies.POLICY_GOAL_GREEDY,
                requested_seed=2,
                effective_seed=3,
                reroll_count=1,
                outcome="LOST_FUEL",
                turn_count=20,
                remaining_fuel=0,
                max_fuel=16,
                base_visit_count=1,
                base_refuel_count=1,
                resource_visit_count=1,
                resource_refuel_count=0,
                used_resource_count=0,
                rift_attempts=0,
                invalid_or_rejected_actions=2,
                path_length=12,
            ),
            evaluate_policies.PolicyRun(
                policy=evaluate_policies.POLICY_GOAL_GREEDY,
                requested_seed=3,
                effective_seed=3,
                reroll_count=0,
                outcome="ABORTED_TURN_LIMIT",
                turn_count=30,
                remaining_fuel=4,
                max_fuel=16,
                base_visit_count=2,
                base_refuel_count=2,
                resource_visit_count=1,
                resource_refuel_count=1,
                used_resource_count=1,
                rift_attempts=2,
                invalid_or_rejected_actions=1,
                path_length=15,
            ),
            evaluate_policies.PolicyRun(
                policy=evaluate_policies.POLICY_GOAL_GREEDY,
                requested_seed=4,
                effective_seed=None,
                reroll_count=None,
                outcome="GENERATION_ERROR",
                turn_count=None,
                remaining_fuel=None,
                max_fuel=None,
                base_visit_count=None,
                base_refuel_count=None,
                resource_visit_count=None,
                resource_refuel_count=None,
                used_resource_count=None,
                rift_attempts=None,
                invalid_or_rejected_actions=None,
                path_length=None,
            ),
        ]

        summary = evaluate_policies.summarize_policy_runs(runs)

        self.assertEqual(summary["total_runs"], 4)
        self.assertEqual(summary["win_count"], 1)
        self.assertEqual(summary["generation_error_count"], 1)
        self.assertEqual(summary["turn_count_sample_count"], 3)
        self.assertEqual(summary["turn_count_p90"], 30)
        self.assertEqual(summary["win_remaining_fuel_median"], 3)
        self.assertEqual(summary["base_visit_run_count"], 2)
        self.assertEqual(summary["base_refuel_run_count"], 2)
        self.assertEqual(summary["multiple_base_refuel_run_count"], 1)
        self.assertEqual(summary["resource_visit_run_count"], 2)
        self.assertEqual(summary["resource_refuel_run_count"], 1)
        self.assertEqual(summary["no_supply_win_count"], 1)
        self.assertEqual(summary["rift_attempt_run_count"], 2)
        self.assertEqual(summary["rift_attempt_count_distribution"], {"0": 1, "1": 1, "2": 1})
        self.assertEqual(summary["reroll_count_distribution"], {"0": 2, "1": 1})
        self.assertEqual(summary["max_reroll_count"], 1)

    def test_summary_document_preserves_policy_order(self) -> None:
        runs = [
            evaluate_policies.PolicyRun(
                policy=policy,
                requested_seed=1,
                effective_seed=1,
                reroll_count=0,
                outcome="WON",
                turn_count=1,
                remaining_fuel=1,
                max_fuel=16,
                base_visit_count=0,
                base_refuel_count=0,
                resource_visit_count=0,
                resource_refuel_count=0,
                used_resource_count=0,
                rift_attempts=0,
                invalid_or_rejected_actions=0,
                path_length=1,
            )
            for policy in evaluate_policies.POLICY_ORDER
        ]

        payload = evaluate_policies.build_summary_document(runs, seed_start=1, seed_end=1, max_turns=256)

        self.assertEqual(list(payload["policies"].keys()), list(evaluate_policies.POLICY_ORDER))
        self.assertEqual(payload["schema_version"], 3)


class MainTests(unittest.TestCase):
    def test_main_writes_sorted_csv_and_summary(self) -> None:
        runs = [
            evaluate_policies.PolicyRun(
                policy=evaluate_policies.POLICY_SUPPLY_AWARE,
                requested_seed=2,
                effective_seed=2,
                reroll_count=0,
                outcome="WON",
                turn_count=5,
                remaining_fuel=2,
                max_fuel=16,
                base_visit_count=0,
                base_refuel_count=0,
                resource_visit_count=0,
                resource_refuel_count=0,
                used_resource_count=0,
                rift_attempts=0,
                invalid_or_rejected_actions=0,
                path_length=5,
            ),
            evaluate_policies.PolicyRun(
                policy=evaluate_policies.POLICY_GOAL_GREEDY,
                requested_seed=1,
                effective_seed=1,
                reroll_count=0,
                outcome="LOST_FUEL",
                turn_count=4,
                remaining_fuel=0,
                max_fuel=16,
                base_visit_count=0,
                base_refuel_count=0,
                resource_visit_count=0,
                resource_refuel_count=0,
                used_resource_count=0,
                rift_attempts=0,
                invalid_or_rejected_actions=0,
                path_length=4,
            ),
        ]

        with tempfile.TemporaryDirectory(dir=".tmp") as tmp_dir:
            root = Path(tmp_dir)
            output_path = root / "runs.csv"
            summary_path = root / "summary.json"
            with patch.object(evaluate_policies, "evaluate_policy_range", return_value=runs):
                exit_code = evaluate_policies.main(
                    [
                        "--seed-start",
                        "1",
                        "--seed-end",
                        "2",
                        "--max-turns",
                        "256",
                        "--output",
                        str(output_path),
                        "--summary",
                        str(summary_path),
                    ]
                )

            with output_path.open(encoding="utf-8", newline="") as file:
                rows = list(csv.DictReader(file))
            payload = json.loads(summary_path.read_text(encoding="utf-8"))

        self.assertEqual(exit_code, 0)
        self.assertEqual(rows[0]["policy"], evaluate_policies.POLICY_GOAL_GREEDY)
        self.assertEqual(rows[1]["policy"], evaluate_policies.POLICY_SUPPLY_AWARE)
        self.assertEqual(payload["schema_version"], 3)


if __name__ == "__main__":
    unittest.main()
