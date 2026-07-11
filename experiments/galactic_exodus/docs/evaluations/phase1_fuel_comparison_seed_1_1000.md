# Galactic Exodus Fuel Comparison

> **文書区分:** 評価根拠
>
> この文書は gameplay 仕様の正本ではありません。現行仕様は `experiments/galactic_exodus/docs/specs/` を参照してください。

## Reproduction

```bash
python experiments/galactic_exodus/fuel_metrics.py \
  --seed-start 1 \
  --seed-count 1000 \
  --rift-density 0.10,0.15 \
  --initial-fuels 24,27,30,33 \
  --base-supplies 8,10,12 \
  --resource-supply 5 \
  --resource-counts 0,1,3 \
  --csv-output experiments/galactic_exodus/results/fuel_comparison_seed_1_1000.csv \
  --markdown-output experiments/galactic_exodus/docs/evaluations/phase1_fuel_comparison_seed_1_1000.md
```

## Conclusion

- 全 72 configuration を評価した結果、`direct_feasible_ratio`、`any_feasible_ratio`、`still_infeasible_ratio` と各 rescue 指標は、同じ `rift_density` の中では `initial_fuel`、`base_supply`、`resource_count` を変えても不変だった。
- `initial_fuel=24/27/30/33`、`base_supply=8/10/12`、`resource_count=0/1/3` のどの組み合わせでも、`rescued_by_base`、`rescued_by_resource`、`base_only_rescue`、`resource_only_rescue`、`both_supply_options` はすべて `0.0%` だった。
- `any_feasible_ratio` は `direct_feasible_ratio` と完全に一致した。一方で `remaining_fuel_at_goal` は `initial_fuel` と `base_supply` に応じて変化し、`best_cost_via_resource` などの R 関連分布は `resource_count` に応じて変化した。
- したがって、今回の候補集合では補給メカニクスは gameplay 上の選択肢としてまだ有効化されていない。現在の 8x8 マップと地形コストでは、到達可能な seed の多くが `initial_fuel=24` で既に走破できる。

## Main Comparison

| density | direct feasible | any feasible | still infeasible | rescued by B | rescued by R | observation |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 0.10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 補給候補を変えても不変 |
| 0.15 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 補給候補を変えても不変 |

`0.15` は `0.10` より `still_infeasible_ratio` が `+3.9pt` 悪化する一方で、補給の追加価値は増えなかった。主候補を `0.10` とする判断は安定している。

## Recommendation For #1040

- `initial_fuel`: `24`
  - テストした候補の中で最小値。
  - `27/30/33` に上げても `direct / any / still infeasible` は改善せず、余裕燃料だけが増えた。
- `base_supply`: 暫定的に最小の `8`
  - `10/12` は走破率や rescue 指標を改善しなかった。
  - `remaining_fuel_at_goal` は増えるが、補給の有効化にはつながっていない。
  - ただし「B 補給を有効な救済手段にする」という目的は未達なので、最終確定には追加評価が必要。
- `resource_count`: 暫定的に `0`
  - `1/3` を追加しても `resource_only_rescue` と `any_feasible_ratio_delta_vs_r0` は全条件で `0.0%`。
  - R を gameplay に残したいなら、現状の候補集合のままでは根拠を作れない。
- `resource_supply=5`: 維持判断は保留
  - 今回は `resource_supply=5` の善し悪しを測れる領域に入っていない。
  - `initial_fuel` を 24 より下げる、または地形コスト/盤面規模を再調整した再評価が必要。

## Deferred Work

- 現行候補で補給差が出ない以上、#1040 で supply 値を最終確定する前に、`initial_fuel < 24` を含む追加評価を行うのが妥当。
- 追加評価なしに Phase 1 へ進む場合は、「補給は当面フレーバーに近く、到達不能率は主に断層で決まる」という前提を明記する必要がある。

## Summary Table

FUEL COMPARISON

| density | R | init | B | direct | any | still | base rescue | resource rescue | base only | resource only | both | req supply med | remain med | resource cost p90 | delta any vs R0 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0.10 | 0 | 24 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.10 | 0 | 24 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | N/A | 0.0% |
| 0.10 | 0 | 24 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | N/A | 0.0% |
| 0.10 | 0 | 27 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | N/A | 0.0% |
| 0.10 | 0 | 27 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | N/A | 0.0% |
| 0.10 | 0 | 27 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 22 | N/A | 0.0% |
| 0.10 | 0 | 30 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 21 | N/A | 0.0% |
| 0.10 | 0 | 30 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 23 | N/A | 0.0% |
| 0.10 | 0 | 30 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 25 | N/A | 0.0% |
| 0.10 | 0 | 33 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 24 | N/A | 0.0% |
| 0.10 | 0 | 33 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 26 | N/A | 0.0% |
| 0.10 | 0 | 33 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 28 | N/A | 0.0% |
| 0.10 | 1 | 24 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 21 | 0.0% |
| 0.10 | 1 | 24 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 21 | 0.0% |
| 0.10 | 1 | 24 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 21 | 0.0% |
| 0.10 | 1 | 27 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | 21 | 0.0% |
| 0.10 | 1 | 27 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | 21 | 0.0% |
| 0.10 | 1 | 27 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 22 | 21 | 0.0% |
| 0.10 | 1 | 30 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 21 | 21 | 0.0% |
| 0.10 | 1 | 30 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 23 | 21 | 0.0% |
| 0.10 | 1 | 30 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 25 | 21 | 0.0% |
| 0.10 | 1 | 33 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 24 | 21 | 0.0% |
| 0.10 | 1 | 33 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 26 | 21 | 0.0% |
| 0.10 | 1 | 33 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 28 | 21 | 0.0% |
| 0.10 | 3 | 24 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 16 | 18 | 0.0% |
| 0.10 | 3 | 24 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | 18 | 0.0% |
| 0.10 | 3 | 24 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | 18 | 0.0% |
| 0.10 | 3 | 27 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 18 | 0.0% |
| 0.10 | 3 | 27 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 21 | 18 | 0.0% |
| 0.10 | 3 | 27 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 23 | 18 | 0.0% |
| 0.10 | 3 | 30 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 22 | 18 | 0.0% |
| 0.10 | 3 | 30 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 24 | 18 | 0.0% |
| 0.10 | 3 | 30 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 26 | 18 | 0.0% |
| 0.10 | 3 | 33 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 25 | 18 | 0.0% |
| 0.10 | 3 | 33 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 27 | 18 | 0.0% |
| 0.10 | 3 | 33 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 29 | 18 | 0.0% |
| 0.15 | 0 | 24 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.15 | 0 | 24 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | N/A | 0.0% |
| 0.15 | 0 | 24 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | N/A | 0.0% |
| 0.15 | 0 | 27 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | N/A | 0.0% |
| 0.15 | 0 | 27 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | N/A | 0.0% |
| 0.15 | 0 | 27 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 22 | N/A | 0.0% |
| 0.15 | 0 | 30 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 21 | N/A | 0.0% |
| 0.15 | 0 | 30 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 23 | N/A | 0.0% |
| 0.15 | 0 | 30 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 25 | N/A | 0.0% |
| 0.15 | 0 | 33 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 24 | N/A | 0.0% |
| 0.15 | 0 | 33 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 26 | N/A | 0.0% |
| 0.15 | 0 | 33 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 28 | N/A | 0.0% |
| 0.15 | 1 | 24 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 23 | 0.0% |
| 0.15 | 1 | 24 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 23 | 0.0% |
| 0.15 | 1 | 24 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 23 | 0.0% |
| 0.15 | 1 | 27 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | 23 | 0.0% |
| 0.15 | 1 | 27 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | 23 | 0.0% |
| 0.15 | 1 | 27 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 22 | 23 | 0.0% |
| 0.15 | 1 | 30 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 21 | 23 | 0.0% |
| 0.15 | 1 | 30 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 23 | 23 | 0.0% |
| 0.15 | 1 | 30 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 25 | 23 | 0.0% |
| 0.15 | 1 | 33 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 24 | 23 | 0.0% |
| 0.15 | 1 | 33 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 26 | 23 | 0.0% |
| 0.15 | 1 | 33 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 28 | 23 | 0.0% |
| 0.15 | 3 | 24 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 19 | 0.0% |
| 0.15 | 3 | 24 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 19 | 0.0% |
| 0.15 | 3 | 24 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 19 | 0.0% |
| 0.15 | 3 | 27 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | 19 | 0.0% |
| 0.15 | 3 | 27 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | 19 | 0.0% |
| 0.15 | 3 | 27 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 22 | 19 | 0.0% |
| 0.15 | 3 | 30 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 21 | 19 | 0.0% |
| 0.15 | 3 | 30 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 23 | 19 | 0.0% |
| 0.15 | 3 | 30 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 25 | 19 | 0.0% |
| 0.15 | 3 | 33 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 24 | 19 | 0.0% |
| 0.15 | 3 | 33 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 26 | 19 | 0.0% |
| 0.15 | 3 | 33 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 28 | 19 | 0.0% |

## Detailed Metrics

### density=0.10 R=0 initial=24 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 8
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=24 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 10
  - median: 17
  - p90: 19
  - max: 20

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=24 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 12
  - median: 19
  - p90: 21
  - max: 22

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=27 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 11
  - median: 18
  - p90: 20
  - max: 21

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=27 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 13
  - median: 20
  - p90: 22
  - max: 23

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=27 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 15
  - median: 22
  - p90: 24
  - max: 25

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=30 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 21
  - p90: 23
  - max: 24

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=30 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 16
  - median: 23
  - p90: 25
  - max: 26

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=30 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 18
  - median: 25
  - p90: 27
  - max: 28

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=33 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 17
  - median: 24
  - p90: 26
  - max: 27

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=33 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 19
  - median: 26
  - p90: 28
  - max: 29

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=33 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 21
  - median: 28
  - p90: 30
  - max: 31

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=1 initial=24 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 976 (97.6%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 7
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=24 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 976 (97.6%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 9
  - median: 17
  - p90: 19
  - max: 20

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=24 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 976 (97.6%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 11
  - median: 19
  - p90: 21
  - max: 22

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=27 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 10
  - median: 18
  - p90: 20
  - max: 21

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=27 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 12
  - median: 20
  - p90: 22
  - max: 23

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=27 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 22
  - p90: 24
  - max: 25

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=30 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 13
  - median: 21
  - p90: 23
  - max: 24

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=30 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 15
  - median: 23
  - p90: 25
  - max: 26

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=30 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 17
  - median: 25
  - p90: 27
  - max: 28

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=33 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 16
  - median: 24
  - p90: 26
  - max: 27

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=33 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 18
  - median: 26
  - p90: 28
  - max: 29

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=33 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 20
  - median: 28
  - p90: 30
  - max: 31

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=3 initial=24 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 10
  - median: 16
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=24 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 11
  - median: 18
  - p90: 19
  - max: 20

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=24 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 12
  - median: 20
  - p90: 21
  - max: 22

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=27 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 13
  - median: 19
  - p90: 20
  - max: 21

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=27 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 21
  - p90: 22
  - max: 23

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=27 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 15
  - median: 23
  - p90: 24
  - max: 25

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=30 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 16
  - median: 22
  - p90: 23
  - max: 24

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=30 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 17
  - median: 24
  - p90: 25
  - max: 26

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=30 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 18
  - median: 26
  - p90: 27
  - max: 28

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=33 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 19
  - median: 25
  - p90: 26
  - max: 27

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=33 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 20
  - median: 27
  - p90: 28
  - max: 29

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=33 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 21
  - median: 29
  - p90: 30
  - max: 31

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.15 R=0 initial=24 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 6
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=24 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 7
  - median: 17
  - p90: 19
  - max: 20

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=24 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 9
  - median: 19
  - p90: 21
  - max: 22

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=27 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 9
  - median: 18
  - p90: 20
  - max: 21

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=27 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 10
  - median: 20
  - p90: 22
  - max: 23

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=27 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 12
  - median: 22
  - p90: 24
  - max: 25

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=30 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 12
  - median: 21
  - p90: 23
  - max: 24

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=30 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 13
  - median: 23
  - p90: 25
  - max: 26

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=30 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 15
  - median: 25
  - p90: 27
  - max: 28

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=33 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 15
  - median: 24
  - p90: 26
  - max: 27

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=33 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 16
  - median: 26
  - p90: 28
  - max: 29

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=33 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 18
  - median: 28
  - p90: 30
  - max: 31

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=1 initial=24 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 930 (93.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 6
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=24 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 930 (93.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 8
  - median: 17
  - p90: 19
  - max: 20

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=24 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 930 (93.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 10
  - median: 19
  - p90: 21
  - max: 22

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=27 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 9
  - median: 18
  - p90: 20
  - max: 21

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=27 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 11
  - median: 20
  - p90: 22
  - max: 23

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=27 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 13
  - median: 22
  - p90: 24
  - max: 25

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=30 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 12
  - median: 21
  - p90: 23
  - max: 24

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=30 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 23
  - p90: 25
  - max: 26

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=30 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 16
  - median: 25
  - p90: 27
  - max: 28

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=33 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 15
  - median: 24
  - p90: 26
  - max: 27

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=33 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 17
  - median: 26
  - p90: 28
  - max: 29

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=33 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 937 (93.7%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 19
  - median: 28
  - p90: 30
  - max: 31

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=3 initial=24 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 8
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=24 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 8
  - median: 17
  - p90: 19
  - max: 20

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=24 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 10
  - median: 19
  - p90: 21
  - max: 22

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=27 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 11
  - median: 18
  - p90: 20
  - max: 21

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=27 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 11
  - median: 20
  - p90: 22
  - max: 23

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=27 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 13
  - median: 22
  - p90: 24
  - max: 25

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=30 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 21
  - p90: 23
  - max: 24

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=30 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 23
  - p90: 25
  - max: 26

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=30 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 16
  - median: 25
  - p90: 27
  - max: 28

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=33 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 17
  - median: 24
  - p90: 26
  - max: 27

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=33 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 17
  - median: 26
  - p90: 28
  - max: 29

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=33 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 938 (93.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 0 (0.0%)
- rescued by resource: 0 (0.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 0.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 0.0%
- base only share among rescued: N/A
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 19
  - median: 28
  - p90: 30
  - max: 31

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 0

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25
