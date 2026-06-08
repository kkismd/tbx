#!/usr/bin/env python3

from __future__ import annotations

import argparse
import math
from pathlib import Path
import statistics
from collections import Counter
from dataclasses import dataclass
import sys

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from experiments.galactic_exodus import simulate
else:
    from experiments.galactic_exodus import simulate


@dataclass(frozen=True)
class SeedMetrics:
    seed: int
    verdict: str
    s_to_h_cost: int | None
    s_to_h_steps: int | None
    base_is_mandatory: bool
    base_route_advantage_raw: int | None


@dataclass(frozen=True)
class DistributionStats:
    sample_count: int
    excluded_count: int
    min_value: int | float | None
    median_value: int | float | None
    p90_value: int | float | None
    max_value: int | float | None


@dataclass(frozen=True)
class BatchSummary:
    seed_start: int
    seed_count: int
    seed_end: int
    rift_density: float
    resource_count: int
    total_runs: int
    verdict_counts: dict[str, int]
    verdict_ratios: dict[str, float]
    s_to_h_cost_stats: DistributionStats
    s_to_h_steps_stats: DistributionStats
    base_is_mandatory_count: int
    base_is_mandatory_ratio: float
    base_route_advantage_counts: dict[str, int]
    base_route_advantage_ratios: dict[str, float]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Aggregate Galactic Exodus Phase 0 metrics across multiple seeds.")
    parser.add_argument("--seed-start", type=int, required=True, help="First seed in the contiguous seed range.")
    parser.add_argument("--seed-count", type=int, required=True, help="Number of consecutive seeds to evaluate.")
    parser.add_argument(
        "--rift-density",
        type=float,
        default=simulate.DEFAULT_RIFT_DENSITY,
        help="Density of impassable fault-line edges (default: 0.10).",
    )
    parser.add_argument(
        "--resource-count",
        type=int,
        default=simulate.DEFAULT_RESOURCE_COUNT,
        help="Number of resource objects to place (default: 3).",
    )
    return parser.parse_args()


def validate_seed_count(seed_count: int) -> None:
    if seed_count <= 0:
        raise ValueError("seed-count must be positive")


def percentile_nearest_rank(series: list[int], percentile: float) -> int:
    if not series:
        raise ValueError("series must not be empty")
    if not 0.0 < percentile <= 1.0:
        raise ValueError("percentile must be between 0.0 and 1.0")
    rank = math.ceil(percentile * len(series))
    return sorted(series)[rank - 1]


def build_distribution_stats(series: list[int | None]) -> DistributionStats:
    present_values = sorted(value for value in series if value is not None)
    excluded_count = len(series) - len(present_values)
    if not present_values:
        return DistributionStats(
            sample_count=0,
            excluded_count=excluded_count,
            min_value=None,
            median_value=None,
            p90_value=None,
            max_value=None,
        )
    return DistributionStats(
        sample_count=len(present_values),
        excluded_count=excluded_count,
        min_value=present_values[0],
        median_value=statistics.median(present_values),
        p90_value=percentile_nearest_rank(present_values, 0.90),
        max_value=present_values[-1],
    )


def collect_seed_metrics(seed_start: int, seed_count: int, rift_density: float, resource_count: int) -> list[SeedMetrics]:
    validate_seed_count(seed_count)
    simulate.validate_rift_density(rift_density)
    simulate.validate_resource_count(resource_count)

    collected: list[SeedMetrics] = []
    for seed in range(seed_start, seed_start + seed_count):
        galactic_map = simulate.generate_map(seed, resource_count, rift_density)
        analysis = simulate.analyze_paths(galactic_map)
        collected.append(
            SeedMetrics(
                seed=seed,
                verdict=simulate.classify_verdict(analysis),
                s_to_h_cost=analysis.best_cost,
                s_to_h_steps=analysis.best_path_length,
                base_is_mandatory=analysis.base_is_mandatory,
                base_route_advantage_raw=analysis.base_route_advantage_raw,
            )
        )
    return collected


def summarize_seed_metrics(
    seed_metrics: list[SeedMetrics],
    *,
    seed_start: int,
    seed_count: int,
    rift_density: float,
    resource_count: int,
) -> BatchSummary:
    validate_seed_count(seed_count)
    if len(seed_metrics) != seed_count:
        raise ValueError("seed_metrics length must match seed_count")

    verdict_counter = Counter(metric.verdict for metric in seed_metrics)
    verdict_counts = {
        verdict: verdict_counter.get(verdict, 0)
        for verdict in simulate.VERDICT_PRIORITY_ORDER
    }
    verdict_ratios = {
        verdict: count / seed_count
        for verdict, count in verdict_counts.items()
    }

    cost_stats = build_distribution_stats([metric.s_to_h_cost for metric in seed_metrics])
    step_stats = build_distribution_stats([metric.s_to_h_steps for metric in seed_metrics])

    mandatory_count = sum(1 for metric in seed_metrics if metric.base_is_mandatory)
    advantage_counter = Counter(classify_advantage(metric.base_route_advantage_raw) for metric in seed_metrics)
    advantage_counts = {
        label: advantage_counter.get(label, 0)
        for label in ("negative", "zero", "positive", "unavailable")
    }
    advantage_ratios = {
        label: count / seed_count
        for label, count in advantage_counts.items()
    }

    return BatchSummary(
        seed_start=seed_start,
        seed_count=seed_count,
        seed_end=seed_start + seed_count - 1,
        rift_density=rift_density,
        resource_count=resource_count,
        total_runs=seed_count,
        verdict_counts=verdict_counts,
        verdict_ratios=verdict_ratios,
        s_to_h_cost_stats=cost_stats,
        s_to_h_steps_stats=step_stats,
        base_is_mandatory_count=mandatory_count,
        base_is_mandatory_ratio=mandatory_count / seed_count,
        base_route_advantage_counts=advantage_counts,
        base_route_advantage_ratios=advantage_ratios,
    )


def classify_advantage(value: int | None) -> str:
    if value is None:
        return "unavailable"
    if value < 0:
        return "negative"
    if value > 0:
        return "positive"
    return "zero"


def format_ratio(ratio: float) -> str:
    return f"{ratio * 100:.1f}%"


def format_number(value: int | float | None) -> str:
    if value is None:
        return "N/A"
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


def format_distribution_block(name: str, stats: DistributionStats) -> list[str]:
    return [
        name,
        f"  sample_count: {stats.sample_count}",
        f"  excluded_unreachable: {stats.excluded_count}",
        f"  min: {format_number(stats.min_value)}",
        f"  median: {format_number(stats.median_value)}",
        f"  p90: {format_number(stats.p90_value)}",
        f"  max: {format_number(stats.max_value)}",
    ]


def format_summary(summary: BatchSummary) -> str:
    lines = [
        "PHASE 0 METRICS",
        f"  seed_start: {summary.seed_start}",
        f"  seed_count: {summary.seed_count}",
        f"  seed_end: {summary.seed_end}",
        f"  rift_density: {summary.rift_density:.2f}",
        f"  resource_count: {summary.resource_count}",
        f"  total_runs: {summary.total_runs}",
        "",
        "VERDICT COUNTS",
    ]
    for verdict in simulate.VERDICT_PRIORITY_ORDER:
        lines.append(
            f"  {verdict}: {summary.verdict_counts[verdict]} ({format_ratio(summary.verdict_ratios[verdict])})"
        )
    lines.extend(
        [
            "",
            *format_distribution_block("S_to_H_cost", summary.s_to_h_cost_stats),
            "",
            *format_distribution_block("S_to_H_steps", summary.s_to_h_steps_stats),
            "",
            "base_is_mandatory",
            f"  yes: {summary.base_is_mandatory_count} ({format_ratio(summary.base_is_mandatory_ratio)})",
            f"  no: {summary.total_runs - summary.base_is_mandatory_count} "
            f"({format_ratio(1.0 - summary.base_is_mandatory_ratio)})",
            "",
            "base_route_advantage_raw",
        ]
    )
    for label in ("negative", "zero", "positive", "unavailable"):
        lines.append(
            f"  {label}: {summary.base_route_advantage_counts[label]} "
            f"({format_ratio(summary.base_route_advantage_ratios[label])})"
        )
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    try:
        seed_metrics = collect_seed_metrics(
            seed_start=args.seed_start,
            seed_count=args.seed_count,
            rift_density=args.rift_density,
            resource_count=args.resource_count,
        )
        summary = summarize_seed_metrics(
            seed_metrics,
            seed_start=args.seed_start,
            seed_count=args.seed_count,
            rift_density=args.rift_density,
            resource_count=args.resource_count,
        )
    except ValueError as exc:
        raise SystemExit(f"error: {exc}") from exc
    print(format_summary(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
