#!/usr/bin/env python3
"""Evaluate deterministic Galactic Exodus policies using the non-interactive engine API."""

from __future__ import annotations

import argparse
import csv
import json
from dataclasses import dataclass
from pathlib import Path
import statistics
import sys
from typing import Callable

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from experiments.galactic_exodus import engine
    from experiments.galactic_exodus import metrics
    from experiments.galactic_exodus import simulate
else:
    from experiments.galactic_exodus import engine
    from experiments.galactic_exodus import metrics
    from experiments.galactic_exodus import simulate


POLICY_GOAL_GREEDY = "GOAL_GREEDY"
POLICY_SUPPLY_AWARE = "SUPPLY_AWARE"
POLICY_ORDER = (POLICY_GOAL_GREEDY, POLICY_SUPPLY_AWARE)
DIRECTION_ORDER = ("N", "E", "S", "W")

CSV_FIELDNAMES = [
    "policy",
    "requested_seed",
    "effective_seed",
    "reroll_count",
    "outcome",
    "turn_count",
    "remaining_fuel",
    "max_fuel",
    "base_visit_count",
    "base_refuel_count",
    "resource_visit_count",
    "resource_refuel_count",
    "used_resource_count",
    "rift_attempts",
    "invalid_or_rejected_actions",
    "path_length",
]

SUMMARY_FIELDS = [
    "total_runs",
    "win_count",
    "win_rate",
    "lost_fuel_count",
    "lost_fuel_rate",
    "aborted_turn_limit_count",
    "aborted_turn_limit_rate",
    "aborted_no_policy_action_count",
    "aborted_no_policy_action_rate",
    "generation_error_count",
    "generation_error_rate",
    "turn_count_sample_count",
    "turn_count_median",
    "turn_count_p90",
    "win_remaining_fuel_sample_count",
    "win_remaining_fuel_median",
    "win_remaining_fuel_p90",
    "base_visit_run_count",
    "base_visit_rate",
    "base_refuel_run_count",
    "base_refuel_rate",
    "multiple_base_refuel_run_count",
    "multiple_base_refuel_rate",
    "resource_visit_run_count",
    "resource_visit_rate",
    "resource_refuel_run_count",
    "resource_refuel_rate",
    "multiple_resource_refuel_run_count",
    "multiple_resource_refuel_rate",
    "no_supply_win_count",
    "no_supply_win_rate",
    "rift_attempt_run_count",
    "rift_attempt_rate",
    "rift_attempt_count_distribution",
    "reroll_occurred_count",
    "reroll_rate",
    "reroll_count_distribution",
    "max_reroll_count",
]


@dataclass(frozen=True)
class PolicyView:
    known_cells: dict[simulate.Position, str]
    known_routes: dict[simulate.Edge, str]
    player_position: simulate.Position
    remaining_fuel: int
    used_resource_positions: set[simulate.Position]
    goal_position: simulate.Position


@dataclass(frozen=True)
class PolicyRun:
    policy: str
    requested_seed: int
    effective_seed: int | None
    reroll_count: int | None
    outcome: str
    turn_count: int | None
    remaining_fuel: int | None
    max_fuel: int | None
    base_visit_count: int | None
    base_refuel_count: int | None
    resource_visit_count: int | None
    resource_refuel_count: int | None
    used_resource_count: int | None
    rift_attempts: int | None
    invalid_or_rejected_actions: int | None
    path_length: int | None

    def to_csv_row(self) -> dict[str, str]:
        return {
            key: format_csv_value(getattr(self, key))
            for key in CSV_FIELDNAMES
        }


def format_csv_value(value: object | None) -> str:
    if value is None:
        return ""
    return str(value)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Evaluate deterministic Galactic Exodus policies over a contiguous seed range."
    )
    parser.add_argument("--seed-start", type=int, required=True, help="First requested seed to evaluate.")
    parser.add_argument("--seed-end", type=int, required=True, help="Last requested seed to evaluate.")
    parser.add_argument("--max-turns", type=int, required=True, help="Turn limit for each run.")
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="CSV file path for per-run results.",
    )
    parser.add_argument(
        "--summary",
        type=Path,
        required=True,
        help="JSON file path for aggregated policy results.",
    )
    return parser.parse_args(argv)


def validate_seed_range(seed_start: int, seed_end: int) -> None:
    if seed_start > seed_end:
        raise ValueError("seed-start must be less than or equal to seed-end")


def validate_schema_version() -> None:
    if engine.SCHEMA_VERSION != 3:
        raise ValueError(f"expected GameLog schema version 3, got {engine.SCHEMA_VERSION}")


def policy_view_from_state(state: engine.GameState) -> PolicyView:
    return PolicyView(
        known_cells=dict(state.known_cells),
        known_routes=dict(state.known_routes),
        player_position=state.player_position,
        remaining_fuel=state.remaining_fuel,
        used_resource_positions=set(state.used_resource_positions),
        goal_position=state.settings.goal_position,
    )


def choose_goal_greedy_action(view: PolicyView) -> str | None:
    return choose_step_toward_target(view, view.goal_position)


def choose_supply_aware_action(view: PolicyView) -> str | None:
    if view.remaining_fuel <= 8:
        target = choose_supply_target(view)
        if target is not None:
            return choose_step_toward_target(view, target)
    return choose_goal_greedy_action(view)


def choose_supply_target(view: PolicyView) -> simulate.Position | None:
    candidates: list[tuple[int, int, simulate.Position]] = []
    for position, symbol in view.known_cells.items():
        if symbol == engine.BASE_CELL:
            candidates.append((manhattan_distance(view.player_position, position), 0, position))
            continue
        if symbol == engine.RESOURCE_CELL and position not in view.used_resource_positions:
            candidates.append((manhattan_distance(view.player_position, position), 1, position))
    if not candidates:
        return None
    return min(candidates, key=lambda item: (item[0], item[1], item[2][0], item[2][1]))[2]


def choose_step_toward_target(view: PolicyView, target: simulate.Position) -> str | None:
    candidates: list[tuple[int, int, str]] = []
    for index, direction in enumerate(DIRECTION_ORDER):
        attempted = engine.move_position(view.player_position, engine.COMMAND_DELTAS[direction])
        if not engine.is_inside_board(attempted):
            continue
        edge = simulate.normalize_edge(view.player_position, attempted)
        if view.known_routes.get(edge) == engine.ROUTE_RIFT:
            continue
        symbol = view.known_cells.get(attempted)
        if symbol is not None and simulate.terrain_cost(symbol) > view.remaining_fuel:
            continue
        candidates.append((manhattan_distance(attempted, target), index, direction))
    if not candidates:
        return None
    return min(candidates)[2]


def manhattan_distance(left: simulate.Position, right: simulate.Position) -> int:
    return abs(left[0] - right[0]) + abs(left[1] - right[1])


def evaluate_policy_run(
    policy: str,
    requested_seed: int,
    max_turns: int,
    settings: engine.GameSettings = engine.DEFAULT_SETTINGS,
) -> PolicyRun:
    simulate.validate_non_negative("max-turns", max_turns)
    policy_fn = resolve_policy(policy)
    try:
        state = engine.create_game(requested_seed, settings)
    except engine.GenerationError:
        return PolicyRun(
            policy=policy,
            requested_seed=requested_seed,
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
        )

    while state.game_status == engine.GAME_STATUS_IN_PROGRESS:
        if state.turn_count >= max_turns:
            return build_policy_run(policy, requested_seed, state, engine.FINAL_OUTCOME_ABORTED_TURN_LIMIT)
        action = policy_fn(policy_view_from_state(state))
        if action is None:
            return build_policy_run(policy, requested_seed, state, engine.FINAL_OUTCOME_ABORTED_NO_POLICY_ACTION)
        engine.apply_command(state, action)

    return build_policy_run(policy, requested_seed, state, engine.final_outcome_for_status(state.game_status))


def build_policy_run(
    policy: str,
    requested_seed: int,
    state: engine.GameState,
    outcome: str,
) -> PolicyRun:
    return PolicyRun(
        policy=policy,
        requested_seed=requested_seed,
        effective_seed=state.effective_seed,
        reroll_count=state.reroll_count,
        outcome=outcome,
        turn_count=state.turn_count,
        remaining_fuel=state.remaining_fuel,
        max_fuel=state.settings.max_fuel,
        base_visit_count=state.base_visit_count,
        base_refuel_count=state.base_refuel_count,
        resource_visit_count=state.resource_visit_count,
        resource_refuel_count=state.resource_refuel_count,
        used_resource_count=len(state.used_resource_positions),
        rift_attempts=state.rift_attempt_count,
        invalid_or_rejected_actions=state.invalid_or_rejected_action_count,
        path_length=max(len(state.path) - 1, 0),
    )


def resolve_policy(policy: str) -> Callable[[PolicyView], str | None]:
    if policy == POLICY_GOAL_GREEDY:
        return choose_goal_greedy_action
    if policy == POLICY_SUPPLY_AWARE:
        return choose_supply_aware_action
    raise ValueError(f"unknown policy: {policy}")


def evaluate_policy_range(
    seed_start: int,
    seed_end: int,
    max_turns: int,
    settings: engine.GameSettings = engine.DEFAULT_SETTINGS,
) -> list[PolicyRun]:
    validate_seed_range(seed_start, seed_end)
    validate_schema_version()

    runs: list[PolicyRun] = []
    for policy in POLICY_ORDER:
        for requested_seed in range(seed_start, seed_end + 1):
            runs.append(evaluate_policy_run(policy, requested_seed, max_turns, settings))
    return runs


def summarize_policy_runs(policy_runs: list[PolicyRun]) -> dict[str, object]:
    if not policy_runs:
        raise ValueError("policy_runs must not be empty")
    policies = {run.policy for run in policy_runs}
    if len(policies) != 1:
        raise ValueError("policy_runs must contain exactly one policy")

    total_runs = len(policy_runs)
    outcome_counts = {
        "WON": count_outcome(policy_runs, "WON"),
        "LOST_FUEL": count_outcome(policy_runs, "LOST_FUEL"),
        "ABORTED_TURN_LIMIT": count_outcome(policy_runs, "ABORTED_TURN_LIMIT"),
        "ABORTED_NO_POLICY_ACTION": count_outcome(policy_runs, "ABORTED_NO_POLICY_ACTION"),
        "GENERATION_ERROR": count_outcome(policy_runs, "GENERATION_ERROR"),
    }

    turn_counts = [run.turn_count for run in policy_runs if run.turn_count is not None]
    win_remaining_fuel = [run.remaining_fuel for run in policy_runs if run.outcome == "WON" and run.remaining_fuel is not None]
    rift_attempt_distribution = distribution_counts(run.rift_attempts for run in policy_runs if run.rift_attempts is not None)
    reroll_distribution = distribution_counts(run.reroll_count for run in policy_runs if run.reroll_count is not None)
    max_reroll_count = max((run.reroll_count for run in policy_runs if run.reroll_count is not None), default=None)

    summary = {
        "total_runs": total_runs,
        "win_count": outcome_counts["WON"],
        "win_rate": ratio(outcome_counts["WON"], total_runs),
        "lost_fuel_count": outcome_counts["LOST_FUEL"],
        "lost_fuel_rate": ratio(outcome_counts["LOST_FUEL"], total_runs),
        "aborted_turn_limit_count": outcome_counts["ABORTED_TURN_LIMIT"],
        "aborted_turn_limit_rate": ratio(outcome_counts["ABORTED_TURN_LIMIT"], total_runs),
        "aborted_no_policy_action_count": outcome_counts["ABORTED_NO_POLICY_ACTION"],
        "aborted_no_policy_action_rate": ratio(outcome_counts["ABORTED_NO_POLICY_ACTION"], total_runs),
        "generation_error_count": outcome_counts["GENERATION_ERROR"],
        "generation_error_rate": ratio(outcome_counts["GENERATION_ERROR"], total_runs),
        "turn_count_sample_count": len(turn_counts),
        "turn_count_median": median_or_none(turn_counts),
        "turn_count_p90": p90_or_none(turn_counts),
        "win_remaining_fuel_sample_count": len(win_remaining_fuel),
        "win_remaining_fuel_median": median_or_none(win_remaining_fuel),
        "win_remaining_fuel_p90": p90_or_none(win_remaining_fuel),
        "base_visit_run_count": count_runs(policy_runs, lambda run: positive(run.base_visit_count)),
        "base_visit_rate": ratio(count_runs(policy_runs, lambda run: positive(run.base_visit_count)), total_runs),
        "base_refuel_run_count": count_runs(policy_runs, lambda run: positive(run.base_refuel_count)),
        "base_refuel_rate": ratio(count_runs(policy_runs, lambda run: positive(run.base_refuel_count)), total_runs),
        "multiple_base_refuel_run_count": count_runs(policy_runs, lambda run: at_least(run.base_refuel_count, 2)),
        "multiple_base_refuel_rate": ratio(count_runs(policy_runs, lambda run: at_least(run.base_refuel_count, 2)), total_runs),
        "resource_visit_run_count": count_runs(policy_runs, lambda run: positive(run.resource_visit_count)),
        "resource_visit_rate": ratio(count_runs(policy_runs, lambda run: positive(run.resource_visit_count)), total_runs),
        "resource_refuel_run_count": count_runs(policy_runs, lambda run: positive(run.resource_refuel_count)),
        "resource_refuel_rate": ratio(count_runs(policy_runs, lambda run: positive(run.resource_refuel_count)), total_runs),
        "multiple_resource_refuel_run_count": count_runs(policy_runs, lambda run: at_least(run.resource_refuel_count, 2)),
        "multiple_resource_refuel_rate": ratio(count_runs(policy_runs, lambda run: at_least(run.resource_refuel_count, 2)), total_runs),
        "no_supply_win_count": count_runs(
            policy_runs,
            lambda run: run.outcome == "WON" and zero(run.base_refuel_count) and zero(run.resource_refuel_count),
        ),
        "no_supply_win_rate": ratio(
            count_runs(
                policy_runs,
                lambda run: run.outcome == "WON" and zero(run.base_refuel_count) and zero(run.resource_refuel_count),
            ),
            total_runs,
        ),
        "rift_attempt_run_count": count_runs(policy_runs, lambda run: positive(run.rift_attempts)),
        "rift_attempt_rate": ratio(count_runs(policy_runs, lambda run: positive(run.rift_attempts)), total_runs),
        "rift_attempt_count_distribution": rift_attempt_distribution,
        "reroll_occurred_count": count_runs(policy_runs, lambda run: positive(run.reroll_count)),
        "reroll_rate": ratio(count_runs(policy_runs, lambda run: positive(run.reroll_count)), total_runs),
        "reroll_count_distribution": reroll_distribution,
        "max_reroll_count": max_reroll_count,
    }
    return {field: summary[field] for field in SUMMARY_FIELDS}


def count_outcome(policy_runs: list[PolicyRun], outcome: str) -> int:
    return sum(1 for run in policy_runs if run.outcome == outcome)


def count_runs(policy_runs: list[PolicyRun], predicate: Callable[[PolicyRun], bool]) -> int:
    return sum(1 for run in policy_runs if predicate(run))


def ratio(count: int, total: int) -> float:
    return count / total if total else 0.0


def median_or_none(series: list[int]) -> float | int | None:
    if not series:
        return None
    return statistics.median(sorted(series))


def p90_or_none(series: list[int]) -> int | None:
    if not series:
        return None
    return metrics.percentile_nearest_rank(sorted(series), 0.90)


def distribution_counts(series) -> dict[str, int]:
    counts: dict[int, int] = {}
    for value in series:
        counts[value] = counts.get(value, 0) + 1
    return {
        str(key): counts[key]
        for key in sorted(counts)
    }


def positive(value: int | None) -> bool:
    return value is not None and value > 0


def zero(value: int | None) -> bool:
    return value == 0


def at_least(value: int | None, threshold: int) -> bool:
    return value is not None and value >= threshold


def build_summary_document(
    policy_runs: list[PolicyRun],
    *,
    seed_start: int,
    seed_end: int,
    max_turns: int,
) -> dict[str, object]:
    by_policy: dict[str, list[PolicyRun]] = {policy: [] for policy in POLICY_ORDER}
    for run in policy_runs:
        by_policy[run.policy].append(run)

    return {
        "schema_version": engine.SCHEMA_VERSION,
        "seed_start": seed_start,
        "seed_end": seed_end,
        "max_turns": max_turns,
        "policies": {
            policy: summarize_policy_runs(by_policy[policy])
            for policy in POLICY_ORDER
        },
    }


def write_policy_runs_csv(output_path: Path, policy_runs: list[PolicyRun]) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    ordered_runs = sorted(policy_runs, key=lambda run: (run.policy, run.requested_seed))
    with output_path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=CSV_FIELDNAMES)
        writer.writeheader()
        writer.writerows(run.to_csv_row() for run in ordered_runs)


def write_summary_json(summary_path: Path, payload: dict[str, object]) -> None:
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(payload, ensure_ascii=True, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        policy_runs = evaluate_policy_range(args.seed_start, args.seed_end, args.max_turns)
        summary = build_summary_document(
            policy_runs,
            seed_start=args.seed_start,
            seed_end=args.seed_end,
            max_turns=args.max_turns,
        )
        write_policy_runs_csv(args.output, policy_runs)
        write_summary_json(args.summary, summary)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
