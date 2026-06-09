import csv
from pathlib import Path
import unittest

from experiments.galactic_exodus import fuel_metrics
from experiments.galactic_exodus import metrics
from experiments.galactic_exodus import simulate


def make_fuel_analysis(
    *,
    direct: bool,
    via_base: bool,
    via_resource: bool,
    remaining_direct: int | None = None,
    remaining_base: int | None = None,
    remaining_resource: int | None = None,
    remaining_goal: int | None = None,
    required_supply: int | None = None,
    resource_cost: int | None = None,
) -> simulate.FuelAnalysis:
    return simulate.FuelAnalysis(
        fuel_feasible_direct=direct,
        fuel_feasible_via_base=via_base,
        fuel_feasible_via_resource=via_resource,
        remaining_fuel_direct=remaining_direct,
        remaining_fuel_via_base=remaining_base,
        remaining_fuel_via_resource=remaining_resource,
        remaining_fuel_at_goal=remaining_goal,
        required_supply=required_supply,
        best_cost_via_resource=resource_cost,
        best_resource_position=None,
    )


def make_seed_metric(
    seed: int,
    *,
    direct: bool,
    via_base: bool,
    via_resource: bool,
    remaining_goal: int | None = None,
    required_supply: int | None = None,
    resource_cost: int | None = None,
) -> fuel_metrics.SeedFuelMetrics:
    return fuel_metrics.derive_seed_metrics(
        seed,
        make_fuel_analysis(
            direct=direct,
            via_base=via_base,
            via_resource=via_resource,
            remaining_goal=remaining_goal,
            required_supply=required_supply,
            resource_cost=resource_cost,
        ),
    )


class ParseTests(unittest.TestCase):
    def test_parse_csv_lists_and_seed_range(self) -> None:
        self.assertEqual(fuel_metrics.parse_csv_ints("initial-fuels", "24, 27,30"), [24, 27, 30])
        self.assertEqual(fuel_metrics.parse_csv_floats("rift-density", "0.10,0.15"), [0.10, 0.15])
        self.assertEqual(list(fuel_metrics.seed_range(5, 3)), [5, 6, 7])

    def test_parse_csv_rejects_empty_items(self) -> None:
        with self.assertRaisesRegex(ValueError, "no empty items"):
            fuel_metrics.parse_csv_ints("initial-fuels", "24,,30")


class ConfigurationTests(unittest.TestCase):
    def test_build_configurations_deduplicates_and_sorts(self) -> None:
        configurations = fuel_metrics.build_configurations(
            rift_densities=[0.15, 0.10, 0.10],
            initial_fuels=[30, 24],
            base_supplies=[12, 8],
            resource_supply=5,
            resource_counts=[3, 0, 3],
        )

        self.assertEqual(len(configurations), 16)
        self.assertEqual(
            configurations[0],
            fuel_metrics.FuelMetricConfiguration(
                rift_density=0.10,
                resource_count=0,
                initial_fuel=24,
                base_supply=8,
                resource_supply=5,
            ),
        )
        self.assertEqual(
            configurations[-1],
            fuel_metrics.FuelMetricConfiguration(
                rift_density=0.15,
                resource_count=3,
                initial_fuel=30,
                base_supply=12,
                resource_supply=5,
            ),
        )


class DerivedMetricTests(unittest.TestCase):
    def test_rescue_flags_cover_all_non_direct_combinations(self) -> None:
        direct_metric = make_seed_metric(1, direct=True, via_base=False, via_resource=False)
        base_only_metric = make_seed_metric(2, direct=False, via_base=True, via_resource=False)
        resource_only_metric = make_seed_metric(3, direct=False, via_base=False, via_resource=True)
        both_metric = make_seed_metric(4, direct=False, via_base=True, via_resource=True)
        none_metric = make_seed_metric(5, direct=False, via_base=False, via_resource=False)

        self.assertTrue(direct_metric.fuel_feasible_any)
        self.assertFalse(direct_metric.rescued_by_base)
        self.assertTrue(base_only_metric.rescued_by_base)
        self.assertTrue(base_only_metric.base_only_rescue)
        self.assertTrue(resource_only_metric.rescued_by_resource)
        self.assertTrue(resource_only_metric.resource_only_rescue)
        self.assertTrue(both_metric.rescued_by_base)
        self.assertTrue(both_metric.rescued_by_resource)
        self.assertTrue(both_metric.both_supply_options)
        self.assertTrue(none_metric.still_infeasible)


class SummaryTests(unittest.TestCase):
    def test_summary_counts_ratios_and_distributions(self) -> None:
        configuration = fuel_metrics.FuelMetricConfiguration(0.10, 1, 27, 10, 5)
        seed_metrics = [
            make_seed_metric(1, direct=True, via_base=True, via_resource=False, remaining_goal=3, required_supply=0, resource_cost=11),
            make_seed_metric(2, direct=False, via_base=True, via_resource=False, remaining_goal=1, required_supply=2, resource_cost=12),
            make_seed_metric(3, direct=False, via_base=False, via_resource=True, remaining_goal=2, required_supply=3, resource_cost=10),
            make_seed_metric(4, direct=False, via_base=False, via_resource=False, remaining_goal=None, required_supply=None, resource_cost=None),
        ]

        summary = fuel_metrics.summarize_configuration(
            configuration=configuration,
            seed_start=1,
            seed_count=4,
            seed_metrics=seed_metrics,
            any_feasible_ratio_delta_vs_r0=0.25,
            still_infeasible_ratio_delta_vs_r0=-0.25,
        )

        self.assertEqual(summary.direct_feasible_count, 1)
        self.assertEqual(summary.via_base_feasible_count, 2)
        self.assertEqual(summary.via_resource_feasible_count, 1)
        self.assertEqual(summary.any_feasible_count, 3)
        self.assertEqual(summary.still_infeasible_count, 1)
        self.assertEqual(summary.rescued_by_base_count, 1)
        self.assertEqual(summary.rescued_by_resource_count, 1)
        self.assertEqual(summary.base_only_rescue_count, 1)
        self.assertEqual(summary.resource_only_rescue_count, 1)
        self.assertEqual(summary.both_supply_options_count, 0)
        self.assertAlmostEqual(summary.base_rescue_rate_among_direct_failures, 1 / 3)
        self.assertAlmostEqual(summary.any_rescue_rate_among_direct_failures, 2 / 3)
        self.assertAlmostEqual(summary.base_only_share_among_rescued, 0.5)
        self.assertEqual(summary.remaining_fuel_at_goal_stats.sample_count, 3)
        self.assertEqual(summary.remaining_fuel_at_goal_stats.excluded_count, 1)
        self.assertEqual(summary.remaining_fuel_at_goal_stats.p90_value, 3)
        self.assertEqual(summary.required_supply_stats.median_value, 2)
        self.assertEqual(summary.best_cost_via_resource_stats.p90_value, 12)

    def test_conditional_ratios_are_none_when_denominators_are_zero(self) -> None:
        configuration = fuel_metrics.FuelMetricConfiguration(0.10, 0, 33, 8, 5)
        all_direct = [
            make_seed_metric(1, direct=True, via_base=True, via_resource=False, remaining_goal=4, required_supply=0),
            make_seed_metric(2, direct=True, via_base=True, via_resource=False, remaining_goal=5, required_supply=0),
        ]
        summary = fuel_metrics.summarize_configuration(
            configuration=configuration,
            seed_start=1,
            seed_count=2,
            seed_metrics=all_direct,
            any_feasible_ratio_delta_vs_r0=0.0,
            still_infeasible_ratio_delta_vs_r0=0.0,
        )

        self.assertIsNone(summary.base_rescue_rate_among_direct_failures)
        self.assertIsNone(summary.resource_rescue_rate_among_direct_failures)
        self.assertIsNone(summary.any_rescue_rate_among_direct_failures)
        self.assertIsNone(summary.base_only_share_among_rescued)

    def test_summarize_configurations_computes_delta_vs_resource_zero(self) -> None:
        baseline = fuel_metrics.FuelMetricConfiguration(0.10, 0, 27, 10, 5)
        variant = fuel_metrics.FuelMetricConfiguration(0.10, 1, 27, 10, 5)
        collected = {
            variant: [
                make_seed_metric(1, direct=True, via_base=True, via_resource=False, remaining_goal=1, required_supply=0),
                make_seed_metric(2, direct=False, via_base=False, via_resource=True, remaining_goal=1, required_supply=1),
            ],
            baseline: [
                make_seed_metric(1, direct=True, via_base=True, via_resource=False, remaining_goal=1, required_supply=0),
                make_seed_metric(2, direct=False, via_base=False, via_resource=False, remaining_goal=None, required_supply=None),
            ],
        }

        summaries = fuel_metrics.summarize_configurations(seed_start=1, seed_count=2, collected=collected)

        self.assertEqual([summary.configuration.resource_count for summary in summaries], [0, 1])
        self.assertAlmostEqual(summaries[0].any_feasible_ratio_delta_vs_r0, 0.0)
        self.assertAlmostEqual(summaries[1].any_feasible_ratio_delta_vs_r0, 0.5)
        self.assertAlmostEqual(summaries[1].still_infeasible_ratio_delta_vs_r0, -0.5)

    def test_summarize_configurations_requires_resource_zero_baseline(self) -> None:
        variant = fuel_metrics.FuelMetricConfiguration(0.10, 1, 27, 10, 5)
        with self.assertRaisesRegex(ValueError, "include 0"):
            fuel_metrics.summarize_configurations(
                seed_start=1,
                seed_count=1,
                collected={variant: [make_seed_metric(1, direct=True, via_base=True, via_resource=False)]},
            )


class CollectionAndCsvTests(unittest.TestCase):
    def test_resource_count_zero_produces_stable_resource_metrics(self) -> None:
        configuration = fuel_metrics.FuelMetricConfiguration(0.10, 0, 27, 10, 5)
        collected = fuel_metrics.collect_seed_metrics(seed_start=1, seed_count=3, configurations=[configuration])

        self.assertEqual(len(collected[configuration]), 3)
        self.assertTrue(all(not entry.fuel_feasible_via_resource for entry in collected[configuration]))
        self.assertTrue(all(entry.best_cost_via_resource is None for entry in collected[configuration]))

    def test_csv_columns_and_row_order_are_deterministic(self) -> None:
        summary_a = fuel_metrics.FuelMetricSummary(
            configuration=fuel_metrics.FuelMetricConfiguration(0.10, 0, 24, 8, 5),
            seed_start=1,
            seed_count=2,
            seed_end=2,
            total_runs=2,
            direct_feasible_count=1,
            direct_feasible_ratio=0.5,
            via_base_feasible_count=1,
            via_base_feasible_ratio=0.5,
            via_resource_feasible_count=0,
            via_resource_feasible_ratio=0.0,
            any_feasible_count=1,
            any_feasible_ratio=0.5,
            still_infeasible_count=1,
            still_infeasible_ratio=0.5,
            direct_failure_count=1,
            rescued_by_base_count=0,
            rescued_by_base_ratio=0.0,
            rescued_by_resource_count=0,
            rescued_by_resource_ratio=0.0,
            base_only_rescue_count=0,
            base_only_rescue_ratio=0.0,
            resource_only_rescue_count=0,
            resource_only_rescue_ratio=0.0,
            both_supply_options_count=0,
            both_supply_options_ratio=0.0,
            base_rescue_rate_among_direct_failures=0.0,
            resource_rescue_rate_among_direct_failures=0.0,
            any_rescue_rate_among_direct_failures=0.0,
            base_only_share_among_rescued=None,
            any_feasible_ratio_delta_vs_r0=0.0,
            still_infeasible_ratio_delta_vs_r0=0.0,
            remaining_fuel_at_goal_stats=metrics.DistributionStats(1, 1, 1, 1, 1, 1),
            required_supply_stats=metrics.DistributionStats(1, 1, 0, 0, 0, 0),
            best_cost_via_resource_stats=metrics.DistributionStats(0, 2, None, None, None, None),
        )
        summary_b = fuel_metrics.FuelMetricSummary(
            configuration=fuel_metrics.FuelMetricConfiguration(0.10, 1, 24, 8, 5),
            seed_start=1,
            seed_count=2,
            seed_end=2,
            total_runs=2,
            direct_feasible_count=1,
            direct_feasible_ratio=0.5,
            via_base_feasible_count=1,
            via_base_feasible_ratio=0.5,
            via_resource_feasible_count=1,
            via_resource_feasible_ratio=0.5,
            any_feasible_count=2,
            any_feasible_ratio=1.0,
            still_infeasible_count=0,
            still_infeasible_ratio=0.0,
            direct_failure_count=1,
            rescued_by_base_count=0,
            rescued_by_base_ratio=0.0,
            rescued_by_resource_count=1,
            rescued_by_resource_ratio=0.5,
            base_only_rescue_count=0,
            base_only_rescue_ratio=0.0,
            resource_only_rescue_count=1,
            resource_only_rescue_ratio=0.5,
            both_supply_options_count=0,
            both_supply_options_ratio=0.0,
            base_rescue_rate_among_direct_failures=0.0,
            resource_rescue_rate_among_direct_failures=1.0,
            any_rescue_rate_among_direct_failures=1.0,
            base_only_share_among_rescued=0.0,
            any_feasible_ratio_delta_vs_r0=0.5,
            still_infeasible_ratio_delta_vs_r0=-0.5,
            remaining_fuel_at_goal_stats=metrics.DistributionStats(2, 0, 1, 1.5, 2, 2),
            required_supply_stats=metrics.DistributionStats(2, 0, 0, 0.5, 1, 1),
            best_cost_via_resource_stats=metrics.DistributionStats(1, 1, 10, 10, 10, 10),
        )

        output_path = Path(".tmp/fuel-metrics-test.csv")
        output_path.parent.mkdir(exist_ok=True)
        fuel_metrics.write_csv(output_path, [summary_a, summary_b])

        with output_path.open(newline="", encoding="utf-8") as handle:
            rows = list(csv.DictReader(handle))

        self.assertEqual(
            list(rows[0].keys())[:5],
            ["rift_density", "resource_count", "initial_fuel", "base_supply", "resource_supply"],
        )
        self.assertEqual(rows[0]["resource_count"], "0")
        self.assertEqual(rows[1]["resource_count"], "1")
        self.assertEqual(rows[1]["any_feasible_ratio_delta_vs_r0"], "0.5")


if __name__ == "__main__":
    unittest.main()
