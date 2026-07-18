import unittest

from experiments.galactic_exodus.archive.evaluation.phase1_lrs import metrics


class PercentileTests(unittest.TestCase):
    def test_percentile_90_uses_nearest_rank(self) -> None:
        self.assertEqual(metrics.percentile_nearest_rank([1, 2, 3, 4, 5], 0.90), 5)
        self.assertEqual(metrics.percentile_nearest_rank([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.90), 9)

    def test_percentile_rejects_empty_series_and_invalid_rate(self) -> None:
        with self.assertRaisesRegex(ValueError, "series must not be empty"):
            metrics.percentile_nearest_rank([], 0.90)
        with self.assertRaisesRegex(ValueError, "between 0.0 and 1.0"):
            metrics.percentile_nearest_rank([1], 0.0)
        with self.assertRaisesRegex(ValueError, "between 0.0 and 1.0"):
            metrics.percentile_nearest_rank([1], 1.01)


class SummaryTests(unittest.TestCase):
    def test_summarize_seed_metrics_computes_counts_ratios_and_distribution_stats(self) -> None:
        seed_metrics = [
            metrics.SeedMetrics(
                seed=10,
                verdict="ACCEPT",
                s_to_h_cost=10,
                s_to_h_steps=8,
                base_is_mandatory=False,
                base_route_advantage_raw=-2,
            ),
            metrics.SeedMetrics(
                seed=11,
                verdict="REJECT_TOO_HARD",
                s_to_h_cost=None,
                s_to_h_steps=None,
                base_is_mandatory=False,
                base_route_advantage_raw=None,
            ),
            metrics.SeedMetrics(
                seed=12,
                verdict="REJECT_BASE_MANDATORY",
                s_to_h_cost=14,
                s_to_h_steps=9,
                base_is_mandatory=True,
                base_route_advantage_raw=None,
            ),
            metrics.SeedMetrics(
                seed=13,
                verdict="ACCEPT",
                s_to_h_cost=16,
                s_to_h_steps=12,
                base_is_mandatory=False,
                base_route_advantage_raw=0,
            ),
            metrics.SeedMetrics(
                seed=14,
                verdict="ACCEPT",
                s_to_h_cost=18,
                s_to_h_steps=15,
                base_is_mandatory=False,
                base_route_advantage_raw=3,
            ),
        ]

        summary = metrics.summarize_seed_metrics(
            seed_metrics,
            seed_start=10,
            seed_count=5,
            rift_density=0.10,
            resource_count=3,
        )

        self.assertEqual(summary.total_runs, 5)
        self.assertEqual(summary.seed_end, 14)
        self.assertEqual(summary.verdict_counts["ACCEPT"], 3)
        self.assertAlmostEqual(summary.verdict_ratios["ACCEPT"], 0.6)
        self.assertEqual(summary.verdict_counts["REJECT_TOO_HARD"], 1)
        self.assertEqual(summary.s_to_h_cost_stats.excluded_count, 1)
        self.assertEqual(summary.s_to_h_cost_stats.min_value, 10)
        self.assertEqual(summary.s_to_h_cost_stats.median_value, 15.0)
        self.assertEqual(summary.s_to_h_cost_stats.p90_value, 18)
        self.assertEqual(summary.s_to_h_cost_stats.max_value, 18)
        self.assertEqual(summary.s_to_h_steps_stats.excluded_count, 1)
        self.assertEqual(summary.s_to_h_steps_stats.min_value, 8)
        self.assertEqual(summary.s_to_h_steps_stats.median_value, 10.5)
        self.assertEqual(summary.s_to_h_steps_stats.p90_value, 15)
        self.assertEqual(summary.s_to_h_steps_stats.max_value, 15)
        self.assertEqual(summary.base_is_mandatory_count, 1)
        self.assertAlmostEqual(summary.base_is_mandatory_ratio, 0.2)
        self.assertEqual(summary.base_route_advantage_counts["negative"], 1)
        self.assertEqual(summary.base_route_advantage_counts["zero"], 1)
        self.assertEqual(summary.base_route_advantage_counts["positive"], 1)
        self.assertEqual(summary.base_route_advantage_counts["unavailable"], 2)
        self.assertAlmostEqual(summary.base_route_advantage_ratios["positive"], 0.2)

    def test_format_summary_renders_required_sections(self) -> None:
        seed_metrics = [
            metrics.SeedMetrics(
                seed=1,
                verdict="ACCEPT",
                s_to_h_cost=9,
                s_to_h_steps=7,
                base_is_mandatory=False,
                base_route_advantage_raw=1,
            ),
            metrics.SeedMetrics(
                seed=2,
                verdict="REJECT_TOO_HARD",
                s_to_h_cost=None,
                s_to_h_steps=None,
                base_is_mandatory=False,
                base_route_advantage_raw=None,
            ),
        ]
        summary = metrics.summarize_seed_metrics(
            seed_metrics,
            seed_start=1,
            seed_count=2,
            rift_density=0.25,
            resource_count=4,
        )

        output = metrics.format_summary(summary)

        self.assertIn("PHASE 0 METRICS", output)
        self.assertIn("  seed_start: 1", output)
        self.assertIn("  seed_count: 2", output)
        self.assertIn("  seed_end: 2", output)
        self.assertIn("VERDICT COUNTS", output)
        self.assertIn("  ACCEPT: 1 (50.0%)", output)
        self.assertIn("  REJECT_TOO_HARD: 1 (50.0%)", output)
        self.assertIn("S_to_H_cost", output)
        self.assertIn("  excluded_unreachable: 1", output)
        self.assertIn("base_is_mandatory", output)
        self.assertIn("  yes: 0 (0.0%)", output)
        self.assertIn("base_route_advantage_raw", output)
        self.assertIn("  unavailable: 1 (50.0%)", output)

    def test_collect_seed_metrics_is_reproducible_for_same_inputs(self) -> None:
        first = metrics.collect_seed_metrics(seed_start=40, seed_count=5, rift_density=0.10, resource_count=3)
        second = metrics.collect_seed_metrics(seed_start=40, seed_count=5, rift_density=0.10, resource_count=3)

        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
