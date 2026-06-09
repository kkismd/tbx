#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
from dataclasses import dataclass, replace
from pathlib import Path
import sys

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from experiments.galactic_exodus import metrics
    from experiments.galactic_exodus import simulate
else:
    from experiments.galactic_exodus import metrics
    from experiments.galactic_exodus import simulate


@dataclass(frozen=True, order=True)
class FuelMetricConfiguration:
    rift_density: float
    resource_count: int
    initial_fuel: int
    base_supply: int
    resource_supply: int


@dataclass(frozen=True)
class SeedFuelMetrics:
    seed: int
    fuel_feasible_direct: bool
    fuel_feasible_via_base: bool
    fuel_feasible_via_resource: bool
    fuel_feasible_any: bool
    rescued_by_base: bool
    rescued_by_resource: bool
    base_only_rescue: bool
    resource_only_rescue: bool
    both_supply_options: bool
    still_infeasible: bool
    remaining_fuel_direct: int | None
    remaining_fuel_via_base: int | None
    remaining_fuel_via_resource: int | None
    remaining_fuel_at_goal: int | None
    required_supply: int | None
    best_cost_via_resource: int | None


@dataclass(frozen=True)
class FuelMetricSummary:
    configuration: FuelMetricConfiguration
    seed_start: int
    seed_count: int
    seed_end: int
    total_runs: int
    direct_feasible_count: int
    direct_feasible_ratio: float
    via_base_feasible_count: int
    via_base_feasible_ratio: float
    via_resource_feasible_count: int
    via_resource_feasible_ratio: float
    any_feasible_count: int
    any_feasible_ratio: float
    still_infeasible_count: int
    still_infeasible_ratio: float
    direct_failure_count: int
    rescued_by_base_count: int
    rescued_by_base_ratio: float
    rescued_by_resource_count: int
    rescued_by_resource_ratio: float
    base_only_rescue_count: int
    base_only_rescue_ratio: float
    resource_only_rescue_count: int
    resource_only_rescue_ratio: float
    both_supply_options_count: int
    both_supply_options_ratio: float
    base_rescue_rate_among_direct_failures: float | None
    resource_rescue_rate_among_direct_failures: float | None
    any_rescue_rate_among_direct_failures: float | None
    base_only_share_among_rescued: float | None
    any_feasible_ratio_delta_vs_r0: float
    still_infeasible_ratio_delta_vs_r0: float
    remaining_fuel_at_goal_stats: metrics.DistributionStats
    required_supply_stats: metrics.DistributionStats
    best_cost_via_resource_stats: metrics.DistributionStats


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare Galactic Exodus fuel configurations across deterministic seed ranges."
    )
    parser.add_argument("--seed-start", type=int, required=True, help="First seed in the contiguous seed range.")
    parser.add_argument("--seed-count", type=int, required=True, help="Number of consecutive seeds to evaluate.")
    parser.add_argument(
        "--rift-density",
        required=True,
        help="Comma-separated rift density list, for example 0.10 or 0.10,0.15.",
    )
    parser.add_argument(
        "--initial-fuels",
        required=True,
        help="Comma-separated initial fuel candidates.",
    )
    parser.add_argument(
        "--base-supplies",
        required=True,
        help="Comma-separated B resupply candidates.",
    )
    parser.add_argument(
        "--resource-supply",
        type=int,
        required=True,
        help="Fuel gained when resupplying once at R.",
    )
    parser.add_argument(
        "--resource-counts",
        required=True,
        help="Comma-separated resource count candidates.",
    )
    parser.add_argument(
        "--csv-output",
        type=Path,
        default=None,
        help="Optional CSV output path.",
    )
    parser.add_argument(
        "--markdown-output",
        type=Path,
        default=None,
        help="Optional Markdown report output path.",
    )
    return parser.parse_args()


def validate_seed_start(seed_start: int) -> None:
    if seed_start <= 0:
        raise ValueError("seed-start must be positive")


def validate_seed_count(seed_count: int) -> None:
    if seed_count <= 0:
        raise ValueError("seed-count must be positive")


def parse_csv_ints(name: str, raw: str) -> list[int]:
    values = parse_csv_values(name, raw)
    parsed: list[int] = []
    for value in values:
        try:
            parsed.append(int(value))
        except ValueError as exc:
            raise ValueError(f"{name} must contain integers: {value!r}") from exc
    return parsed


def parse_csv_floats(name: str, raw: str) -> list[float]:
    values = parse_csv_values(name, raw)
    parsed: list[float] = []
    for value in values:
        try:
            parsed.append(float(value))
        except ValueError as exc:
            raise ValueError(f"{name} must contain floats: {value!r}") from exc
    return parsed


def parse_csv_values(name: str, raw: str) -> list[str]:
    values = [part.strip() for part in raw.split(",")]
    if not values or any(value == "" for value in values):
        raise ValueError(f"{name} must be a comma-separated list with no empty items")
    return values


def build_configurations(
    *,
    rift_densities: list[float],
    initial_fuels: list[int],
    base_supplies: list[int],
    resource_supply: int,
    resource_counts: list[int],
) -> list[FuelMetricConfiguration]:
    for rift_density in rift_densities:
        simulate.validate_rift_density(rift_density)
    for initial_fuel in initial_fuels:
        simulate.validate_non_negative("initial-fuel", initial_fuel)
    for base_supply in base_supplies:
        simulate.validate_non_negative("base-supply", base_supply)
    simulate.validate_non_negative("resource-supply", resource_supply)
    for resource_count in resource_counts:
        simulate.validate_resource_count(resource_count)

    return sorted(
        FuelMetricConfiguration(
            rift_density=rift_density,
            resource_count=resource_count,
            initial_fuel=initial_fuel,
            base_supply=base_supply,
            resource_supply=resource_supply,
        )
        for rift_density in sorted(set(rift_densities))
        for resource_count in sorted(set(resource_counts))
        for initial_fuel in sorted(set(initial_fuels))
        for base_supply in sorted(set(base_supplies))
    )


def seed_range(seed_start: int, seed_count: int) -> range:
    validate_seed_start(seed_start)
    validate_seed_count(seed_count)
    return range(seed_start, seed_start + seed_count)


def derive_seed_metrics(seed: int, analysis: simulate.FuelAnalysis) -> SeedFuelMetrics:
    fuel_feasible_any = (
        analysis.fuel_feasible_direct
        or analysis.fuel_feasible_via_base
        or analysis.fuel_feasible_via_resource
    )
    rescued_by_base = (not analysis.fuel_feasible_direct) and analysis.fuel_feasible_via_base
    rescued_by_resource = (not analysis.fuel_feasible_direct) and analysis.fuel_feasible_via_resource
    base_only_rescue = rescued_by_base and (not analysis.fuel_feasible_via_resource)
    resource_only_rescue = (not analysis.fuel_feasible_direct) and (not analysis.fuel_feasible_via_base) and analysis.fuel_feasible_via_resource
    both_supply_options = rescued_by_base and analysis.fuel_feasible_via_resource
    still_infeasible = not fuel_feasible_any
    return SeedFuelMetrics(
        seed=seed,
        fuel_feasible_direct=analysis.fuel_feasible_direct,
        fuel_feasible_via_base=analysis.fuel_feasible_via_base,
        fuel_feasible_via_resource=analysis.fuel_feasible_via_resource,
        fuel_feasible_any=fuel_feasible_any,
        rescued_by_base=rescued_by_base,
        rescued_by_resource=rescued_by_resource,
        base_only_rescue=base_only_rescue,
        resource_only_rescue=resource_only_rescue,
        both_supply_options=both_supply_options,
        still_infeasible=still_infeasible,
        remaining_fuel_direct=analysis.remaining_fuel_direct,
        remaining_fuel_via_base=analysis.remaining_fuel_via_base,
        remaining_fuel_via_resource=analysis.remaining_fuel_via_resource,
        remaining_fuel_at_goal=analysis.remaining_fuel_at_goal,
        required_supply=analysis.required_supply,
        best_cost_via_resource=analysis.best_cost_via_resource,
    )


def collect_seed_metrics(
    *,
    seed_start: int,
    seed_count: int,
    configurations: list[FuelMetricConfiguration],
) -> dict[FuelMetricConfiguration, list[SeedFuelMetrics]]:
    seeds = seed_range(seed_start, seed_count)
    collected = {configuration: [] for configuration in configurations}

    map_cache: dict[tuple[int, int, float], simulate.GalacticMap] = {}
    for configuration in configurations:
        for seed in seeds:
            cache_key = (seed, configuration.resource_count, configuration.rift_density)
            galactic_map = map_cache.get(cache_key)
            if galactic_map is None:
                galactic_map = simulate.generate_map(seed, configuration.resource_count, configuration.rift_density)
                map_cache[cache_key] = galactic_map
            analysis = simulate.analyze_fuel(
                galactic_map,
                initial_fuel=configuration.initial_fuel,
                base_supply=configuration.base_supply,
                resource_supply=configuration.resource_supply,
            )
            collected[configuration].append(derive_seed_metrics(seed, analysis))
    return collected


def summarize_configuration(
    *,
    configuration: FuelMetricConfiguration,
    seed_start: int,
    seed_count: int,
    seed_metrics: list[SeedFuelMetrics],
    any_feasible_ratio_delta_vs_r0: float,
    still_infeasible_ratio_delta_vs_r0: float,
) -> FuelMetricSummary:
    if len(seed_metrics) != seed_count:
        raise ValueError("seed_metrics length must match seed_count")

    total_runs = seed_count
    direct_feasible_count = count_true(seed_metrics, "fuel_feasible_direct")
    via_base_feasible_count = count_true(seed_metrics, "fuel_feasible_via_base")
    via_resource_feasible_count = count_true(seed_metrics, "fuel_feasible_via_resource")
    any_feasible_count = count_true(seed_metrics, "fuel_feasible_any")
    still_infeasible_count = count_true(seed_metrics, "still_infeasible")
    direct_failure_count = total_runs - direct_feasible_count
    rescued_by_base_count = count_true(seed_metrics, "rescued_by_base")
    rescued_by_resource_count = count_true(seed_metrics, "rescued_by_resource")
    base_only_rescue_count = count_true(seed_metrics, "base_only_rescue")
    resource_only_rescue_count = count_true(seed_metrics, "resource_only_rescue")
    both_supply_options_count = count_true(seed_metrics, "both_supply_options")
    rescued_count = any_feasible_count - direct_feasible_count

    return FuelMetricSummary(
        configuration=configuration,
        seed_start=seed_start,
        seed_count=seed_count,
        seed_end=seed_start + seed_count - 1,
        total_runs=total_runs,
        direct_feasible_count=direct_feasible_count,
        direct_feasible_ratio=direct_feasible_count / total_runs,
        via_base_feasible_count=via_base_feasible_count,
        via_base_feasible_ratio=via_base_feasible_count / total_runs,
        via_resource_feasible_count=via_resource_feasible_count,
        via_resource_feasible_ratio=via_resource_feasible_count / total_runs,
        any_feasible_count=any_feasible_count,
        any_feasible_ratio=any_feasible_count / total_runs,
        still_infeasible_count=still_infeasible_count,
        still_infeasible_ratio=still_infeasible_count / total_runs,
        direct_failure_count=direct_failure_count,
        rescued_by_base_count=rescued_by_base_count,
        rescued_by_base_ratio=rescued_by_base_count / total_runs,
        rescued_by_resource_count=rescued_by_resource_count,
        rescued_by_resource_ratio=rescued_by_resource_count / total_runs,
        base_only_rescue_count=base_only_rescue_count,
        base_only_rescue_ratio=base_only_rescue_count / total_runs,
        resource_only_rescue_count=resource_only_rescue_count,
        resource_only_rescue_ratio=resource_only_rescue_count / total_runs,
        both_supply_options_count=both_supply_options_count,
        both_supply_options_ratio=both_supply_options_count / total_runs,
        base_rescue_rate_among_direct_failures=ratio_or_none(rescued_by_base_count, direct_failure_count),
        resource_rescue_rate_among_direct_failures=ratio_or_none(rescued_by_resource_count, direct_failure_count),
        any_rescue_rate_among_direct_failures=ratio_or_none(rescued_count, direct_failure_count),
        base_only_share_among_rescued=ratio_or_none(base_only_rescue_count, rescued_count),
        any_feasible_ratio_delta_vs_r0=any_feasible_ratio_delta_vs_r0,
        still_infeasible_ratio_delta_vs_r0=still_infeasible_ratio_delta_vs_r0,
        remaining_fuel_at_goal_stats=metrics.build_distribution_stats(
            [entry.remaining_fuel_at_goal for entry in seed_metrics]
        ),
        required_supply_stats=metrics.build_distribution_stats(
            [entry.required_supply for entry in seed_metrics]
        ),
        best_cost_via_resource_stats=metrics.build_distribution_stats(
            [entry.best_cost_via_resource for entry in seed_metrics]
        ),
    )


def summarize_configurations(
    *,
    seed_start: int,
    seed_count: int,
    collected: dict[FuelMetricConfiguration, list[SeedFuelMetrics]],
) -> list[FuelMetricSummary]:
    summaries_without_delta: dict[FuelMetricConfiguration, FuelMetricSummary] = {}
    baseline_ratios: dict[tuple[float, int, int], tuple[float, float]] = {}

    for configuration in sorted(collected):
        summary = summarize_configuration(
            configuration=configuration,
            seed_start=seed_start,
            seed_count=seed_count,
            seed_metrics=collected[configuration],
            any_feasible_ratio_delta_vs_r0=0.0,
            still_infeasible_ratio_delta_vs_r0=0.0,
        )
        summaries_without_delta[configuration] = summary
        if configuration.resource_count == 0:
            baseline_key = (configuration.rift_density, configuration.initial_fuel, configuration.base_supply)
            baseline_ratios[baseline_key] = (summary.any_feasible_ratio, summary.still_infeasible_ratio)

    finalized: list[FuelMetricSummary] = []
    for configuration in sorted(collected):
        baseline_key = (configuration.rift_density, configuration.initial_fuel, configuration.base_supply)
        if baseline_key not in baseline_ratios:
            raise ValueError("resource-counts must include 0 for every density/initial/base combination")
        baseline_any_ratio, baseline_still_ratio = baseline_ratios[baseline_key]
        base_summary = summaries_without_delta[configuration]
        finalized.append(
            replace(
                base_summary,
                any_feasible_ratio_delta_vs_r0=base_summary.any_feasible_ratio - baseline_any_ratio,
                still_infeasible_ratio_delta_vs_r0=base_summary.still_infeasible_ratio - baseline_still_ratio,
            )
        )
    return finalized


def count_true(seed_metrics: list[SeedFuelMetrics], field_name: str) -> int:
    return sum(1 for entry in seed_metrics if getattr(entry, field_name))


def ratio_or_none(numerator: int, denominator: int) -> float | None:
    if denominator == 0:
        return None
    return numerator / denominator


def format_ratio(value: float | None) -> str:
    if value is None:
        return "N/A"
    return f"{value * 100:.1f}%"


def summary_to_row(summary: FuelMetricSummary) -> dict[str, str | int | float]:
    configuration = summary.configuration
    row: dict[str, str | int | float] = {
        "rift_density": f"{configuration.rift_density:.2f}",
        "resource_count": configuration.resource_count,
        "initial_fuel": configuration.initial_fuel,
        "base_supply": configuration.base_supply,
        "resource_supply": configuration.resource_supply,
        "seed_start": summary.seed_start,
        "seed_count": summary.seed_count,
        "seed_end": summary.seed_end,
        "total_runs": summary.total_runs,
        "direct_feasible_count": summary.direct_feasible_count,
        "direct_feasible_ratio": summary.direct_feasible_ratio,
        "via_base_feasible_count": summary.via_base_feasible_count,
        "via_base_feasible_ratio": summary.via_base_feasible_ratio,
        "via_resource_feasible_count": summary.via_resource_feasible_count,
        "via_resource_feasible_ratio": summary.via_resource_feasible_ratio,
        "any_feasible_count": summary.any_feasible_count,
        "any_feasible_ratio": summary.any_feasible_ratio,
        "still_infeasible_count": summary.still_infeasible_count,
        "still_infeasible_ratio": summary.still_infeasible_ratio,
        "direct_failure_count": summary.direct_failure_count,
        "rescued_by_base_count": summary.rescued_by_base_count,
        "rescued_by_base_ratio": summary.rescued_by_base_ratio,
        "rescued_by_resource_count": summary.rescued_by_resource_count,
        "rescued_by_resource_ratio": summary.rescued_by_resource_ratio,
        "base_only_rescue_count": summary.base_only_rescue_count,
        "base_only_rescue_ratio": summary.base_only_rescue_ratio,
        "resource_only_rescue_count": summary.resource_only_rescue_count,
        "resource_only_rescue_ratio": summary.resource_only_rescue_ratio,
        "both_supply_options_count": summary.both_supply_options_count,
        "both_supply_options_ratio": summary.both_supply_options_ratio,
        "base_rescue_rate_among_direct_failures": optional_number(summary.base_rescue_rate_among_direct_failures),
        "resource_rescue_rate_among_direct_failures": optional_number(summary.resource_rescue_rate_among_direct_failures),
        "any_rescue_rate_among_direct_failures": optional_number(summary.any_rescue_rate_among_direct_failures),
        "base_only_share_among_rescued": optional_number(summary.base_only_share_among_rescued),
        "any_feasible_ratio_delta_vs_r0": summary.any_feasible_ratio_delta_vs_r0,
        "still_infeasible_ratio_delta_vs_r0": summary.still_infeasible_ratio_delta_vs_r0,
    }
    row.update(distribution_row("remaining_fuel_at_goal", summary.remaining_fuel_at_goal_stats))
    row.update(distribution_row("required_supply", summary.required_supply_stats))
    row.update(distribution_row("best_cost_via_resource", summary.best_cost_via_resource_stats))
    return row


def distribution_row(prefix: str, stats: metrics.DistributionStats) -> dict[str, str | int | float]:
    return {
        f"{prefix}_sample_count": stats.sample_count,
        f"{prefix}_excluded_count": stats.excluded_count,
        f"{prefix}_min": optional_number(stats.min_value),
        f"{prefix}_median": optional_number(stats.median_value),
        f"{prefix}_p90": optional_number(stats.p90_value),
        f"{prefix}_max": optional_number(stats.max_value),
    }


def optional_number(value: int | float | None) -> str | int | float:
    if value is None:
        return "N/A"
    return value


def write_csv(path: Path, summaries: list[FuelMetricSummary]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    rows = [summary_to_row(summary) for summary in summaries]
    fieldnames = list(rows[0].keys())
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def format_text_summary(summaries: list[FuelMetricSummary]) -> str:
    lines = [
        "FUEL COMPARISON",
        "",
        "| density | R | init | B | direct | any | still | base rescue | resource rescue | base only | resource only | both | req supply med | remain med | resource cost p90 | delta any vs R0 |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for summary in summaries:
        lines.append(
            "| "
            + " | ".join(
                [
                    f"{summary.configuration.rift_density:.2f}",
                    str(summary.configuration.resource_count),
                    str(summary.configuration.initial_fuel),
                    str(summary.configuration.base_supply),
                    format_ratio(summary.direct_feasible_ratio),
                    format_ratio(summary.any_feasible_ratio),
                    format_ratio(summary.still_infeasible_ratio),
                    format_ratio(summary.rescued_by_base_ratio),
                    format_ratio(summary.rescued_by_resource_ratio),
                    format_ratio(summary.base_only_rescue_ratio),
                    format_ratio(summary.resource_only_rescue_ratio),
                    format_ratio(summary.both_supply_options_ratio),
                    metrics.format_number(summary.required_supply_stats.median_value),
                    metrics.format_number(summary.remaining_fuel_at_goal_stats.median_value),
                    metrics.format_number(summary.best_cost_via_resource_stats.p90_value),
                    format_ratio(summary.any_feasible_ratio_delta_vs_r0),
                ]
            )
            + " |"
        )
    return "\n".join(lines)


def format_markdown_report(command: str, summaries: list[FuelMetricSummary]) -> str:
    lines = [
        "# Galactic Exodus Fuel Comparison",
        "",
        "## Reproduction",
        "",
        "```bash",
        command,
        "```",
        "",
        "## Summary Table",
        "",
        format_text_summary(summaries),
        "",
        "## Detailed Metrics",
        "",
    ]
    for summary in summaries:
        lines.extend(format_configuration_block(summary))
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def format_configuration_block(summary: FuelMetricSummary) -> list[str]:
    configuration = summary.configuration
    return [
        f"### density={configuration.rift_density:.2f} R={configuration.resource_count} "
        f"initial={configuration.initial_fuel} B={configuration.base_supply} Rs={configuration.resource_supply}",
        "",
        f"- seed range: {summary.seed_start}..{summary.seed_end} ({summary.seed_count} seeds)",
        f"- direct feasible: {summary.direct_feasible_count} ({format_ratio(summary.direct_feasible_ratio)})",
        f"- via base feasible: {summary.via_base_feasible_count} ({format_ratio(summary.via_base_feasible_ratio)})",
        f"- via resource feasible: {summary.via_resource_feasible_count} ({format_ratio(summary.via_resource_feasible_ratio)})",
        f"- any feasible: {summary.any_feasible_count} ({format_ratio(summary.any_feasible_ratio)})",
        f"- still infeasible: {summary.still_infeasible_count} ({format_ratio(summary.still_infeasible_ratio)})",
        f"- rescued by base: {summary.rescued_by_base_count} ({format_ratio(summary.rescued_by_base_ratio)})",
        f"- rescued by resource: {summary.rescued_by_resource_count} ({format_ratio(summary.rescued_by_resource_ratio)})",
        f"- base only rescue: {summary.base_only_rescue_count} ({format_ratio(summary.base_only_rescue_ratio)})",
        f"- resource only rescue: {summary.resource_only_rescue_count} ({format_ratio(summary.resource_only_rescue_ratio)})",
        f"- both supply options: {summary.both_supply_options_count} ({format_ratio(summary.both_supply_options_ratio)})",
        f"- base rescue among direct failures: {format_ratio(summary.base_rescue_rate_among_direct_failures)}",
        f"- resource rescue among direct failures: {format_ratio(summary.resource_rescue_rate_among_direct_failures)}",
        f"- any rescue among direct failures: {format_ratio(summary.any_rescue_rate_among_direct_failures)}",
        f"- base only share among rescued: {format_ratio(summary.base_only_share_among_rescued)}",
        f"- any feasible delta vs R=0: {format_ratio(summary.any_feasible_ratio_delta_vs_r0)}",
        f"- still infeasible delta vs R=0: {format_ratio(summary.still_infeasible_ratio_delta_vs_r0)}",
        "",
        *format_distribution_block("remaining_fuel_at_goal", summary.remaining_fuel_at_goal_stats),
        "",
        *format_distribution_block("required_supply", summary.required_supply_stats),
        "",
        *format_distribution_block("best_cost_via_resource", summary.best_cost_via_resource_stats),
    ]


def format_distribution_block(name: str, stats: metrics.DistributionStats) -> list[str]:
    return [
        f"- {name}",
        f"  - sample_count: {stats.sample_count}",
        f"  - excluded_count: {stats.excluded_count}",
        f"  - min: {metrics.format_number(stats.min_value)}",
        f"  - median: {metrics.format_number(stats.median_value)}",
        f"  - p90: {metrics.format_number(stats.p90_value)}",
        f"  - max: {metrics.format_number(stats.max_value)}",
    ]


def build_reproduction_command(args: argparse.Namespace) -> str:
    parts = [
        "python experiments/galactic_exodus/fuel_metrics.py",
        f"--seed-start {args.seed_start}",
        f"--seed-count {args.seed_count}",
        f"--rift-density {args.rift_density}",
        f"--initial-fuels {args.initial_fuels}",
        f"--base-supplies {args.base_supplies}",
        f"--resource-supply {args.resource_supply}",
        f"--resource-counts {args.resource_counts}",
    ]
    if args.csv_output is not None:
        parts.append(f"--csv-output {args.csv_output}")
    if args.markdown_output is not None:
        parts.append(f"--markdown-output {args.markdown_output}")
    return " \\\n  ".join(parts)


def main() -> int:
    args = parse_args()
    try:
        rift_densities = parse_csv_floats("rift-density", args.rift_density)
        initial_fuels = parse_csv_ints("initial-fuels", args.initial_fuels)
        base_supplies = parse_csv_ints("base-supplies", args.base_supplies)
        resource_counts = parse_csv_ints("resource-counts", args.resource_counts)
        configurations = build_configurations(
            rift_densities=rift_densities,
            initial_fuels=initial_fuels,
            base_supplies=base_supplies,
            resource_supply=args.resource_supply,
            resource_counts=resource_counts,
        )
        collected = collect_seed_metrics(
            seed_start=args.seed_start,
            seed_count=args.seed_count,
            configurations=configurations,
        )
        summaries = summarize_configurations(
            seed_start=args.seed_start,
            seed_count=args.seed_count,
            collected=collected,
        )
    except ValueError as exc:
        raise SystemExit(f"error: {exc}") from exc

    print(format_text_summary(summaries))

    reproduction_command = build_reproduction_command(args)
    if args.csv_output is not None:
        write_csv(args.csv_output, summaries)
    if args.markdown_output is not None:
        args.markdown_output.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_output.write_text(
            format_markdown_report(reproduction_command, summaries),
            encoding="utf-8",
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
