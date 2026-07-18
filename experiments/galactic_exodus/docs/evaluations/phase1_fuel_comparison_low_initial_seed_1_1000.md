# Galactic Exodus Fuel Comparison

> **文書区分:** 評価根拠
>
> この文書は gameplay 仕様の正本ではありません。現行仕様は `experiments/galactic_exodus/docs/specs/` を参照してください。

## Reproduction

```bash
python experiments/galactic_exodus/archive/evaluation/phase1_lrs/fuel_metrics.py \
  --seed-start 1 \
  --seed-count 1000 \
  --rift-density 0.10,0.15 \
  --initial-fuels 14,16,18,20,22,24 \
  --base-supplies 8,10,12 \
  --resource-supply 5 \
  --resource-counts 0,1,3 \
  --csv-output experiments/galactic_exodus/results/fuel_comparison_low_initial_seed_1_1000.csv \
  --markdown-output experiments/galactic_exodus/docs/evaluations/phase1_fuel_comparison_low_initial_seed_1_1000.md
```

## Conclusion

- seed `1..1000`、`rift_density=0.10 / 0.15`、`initial_fuel=14/16/18/20/22/24`、`base_supply=8/10/12`、`resource_count=0/1/3` の全 108 configuration を評価した。
- `initial_fuel=24` では #1039 と同様に補給差がほぼ消えるが、`16` まで下げると B/R の救済効果が明確に立ち上がる。
- `initial_fuel=14` は direct 成功率が低すぎる。`direct_feasible_ratio` は `0.10 / R=3 / B=10` でも `13.0%`、`0.15 / R=3 / B=10` では `8.3%` で、補給前提が強すぎる。
- `initial_fuel=18` は補給が弱くなりすぎる。`0.10 / R=3 / B=10` で `direct_feasible_ratio=92.7%`、`rescued_by_base_ratio=5.0%`、`rescued_by_resource_ratio=5.0%` まで落ちる。
- `initial_fuel=16` は direct と補給依存が最も素直に分かれる。`0.10 / R=3 / B=10` で `direct=63.4%`、`any=97.7%`、`base rescue=34.3%`、`resource rescue=34.1%`。`0.15 / R=3 / B=10` でも `direct=48.9%`、`any=93.8%`、`base rescue=44.7%`、`resource rescue=42.4%` を保つ。
- `base_supply` の差は小さい。`8 -> 10 -> 12` は主に `remaining_fuel_at_goal` を増やし、走破率や rescue 指標の改善は限定的だった。
- `resource_count=3` は `resource_count=1` より一貫して高い R 救済率を出す。特に `initial_fuel=16` では、`0.15 / B=10` で `34.4% -> 42.4%`、`0.10 / B=10` で `32.4% -> 34.1%` と改善し、`still_infeasible_ratio` は悪化しない。
  - ただしこれは現行 `generate_map(seed, resource_count, rift_density)` による観測上の母集団比較であり、同一マップに R を増やした因果効果ではない。

## Main Findings

| focus | main observation |
| --- | --- |
| initial fuel threshold | `14 -> 16` が最大の段差。direct 成功率が `+40pt` から `+52pt` 改善し、なお補給の役割は残る |
| B rescue activation | B 救済は `14/16` で強く、`18` で急減、`22` 以上でほぼ消える |
| R rescue activation | R 救済は `resource_count=1/3` で `14/16` に強く、`18` で1桁台、`22` 以上でほぼ消える |
| B fixed-solution risk | `resource_count=3` では `both_supply_options_ratio` が高く、`base_only_share_among_rescued` は `initial_fuel=16` で低い |
| density robustness | `0.15` は `0.10` より direct を下げるが、推奨候補の相対順位は維持される |

## Recommendation For #1040

- `initial_fuel`: `16`
  - `14` ほど極端ではなく、`18` ほど補給の意味を失わない中間点。
  - `0.10` 主候補でも `0.15` 比較対照でも、direct と B/R rescue の両方が十分に観測できる。
- `base_supply`: `8`
  - `10/12` は残航行力を増やすが、走破率と rescue 指標の改善は小さい。
  - `16` と組み合わせたときも B only 依存を増やしすぎず、最小値で必要な救済効果を確保できる。
- `resource_count`: `3`
  - `1` より高い R 救済率を示し、`resource_only_rescue` は小さいまま `both_supply_options` を大きく増やす。
  - ただし `resource_count` 間では同じ seed でも地形・B/R 配置を含む標本が変わるため、現行生成条件における観測比較として扱う。
  - `0` では R を gameplay 上の選択肢として残せない。
- `resource_supply`: `5` を維持
  - 今回の評価範囲で `initial_fuel=16` と `resource_count=3` の組み合わせだけで R 救済が十分に立ち上がった。
  - issue の非対象でもあり、追加候補比較なしで維持判断として扱える。

## Rejected Candidates

- `initial_fuel=14`
  - direct が低すぎ、補給前提が強い。
  - `0.15` では `still_infeasible_ratio` が `6.4%〜7.1%` と高めで、理不尽さの懸念が残る。
- `initial_fuel=18`
  - `0.10 / R=3 / B=10` で direct が `92.7%`、`0.15 / R=3 / B=10` で `82.0%` と高く、補給の役割が弱い。
- `initial_fuel=20/22/24`
  - direct が高すぎ、B/R rescue がほぼ消える。
- `base_supply=10/12`
  - `8` に対する改善が小さく、余裕燃料だけを増やしやすい。
- `resource_count=0`
  - R 救済が存在しない。
- `resource_count=1`
  - `3` より救済率が低く、R の追加価値が弱い。

## Final Judgment

- `rift_density=0.10` を主候補、`0.15` を比較対照とする判断は維持する。
- #1040 には、`initial_fuel=16`、`base_supply=8`、`resource_count=3`、`resource_supply=5` を Phase 1 初期候補として渡せる。
- `resource_count=3` の採用理由は現行生成条件での観測比較に基づく。Phase 1 では、R 個数の効果を実プレイとプレイテストで再確認する前提とする。
- 追加の燃料候補比較は必須ではなくなった。以後の課題は、Phase 1 実装でこのパラメータ群が未知情報下のプレイ感でも機能するかの確認へ移せる。

## Initial Fuel Comparison

### density=0.10 R=0 B=8

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 9.4% | 97.1% | 2.9% | 87.7% | 0.0% | N/A | N/A | N/A | N/A | N/A |
| 16 | 61.5% | 97.7% | 2.3% | 36.2% | 0.0% | 52.1% | 0.6% | -0.6% | -51.5% | 0.0% |
| 18 | 92.2% | 97.7% | 2.3% | 5.5% | 0.0% | 30.7% | 0.0% | 0.0% | -30.7% | 0.0% |
| 20 | 97.1% | 97.7% | 2.3% | 0.6% | 0.0% | 4.9% | 0.0% | 0.0% | -4.9% | 0.0% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.6% | 0.0% | 0.0% | -0.6% | 0.0% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=0 B=10

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 9.4% | 97.7% | 2.3% | 88.3% | 0.0% | N/A | N/A | N/A | N/A | N/A |
| 16 | 61.5% | 97.7% | 2.3% | 36.2% | 0.0% | 52.1% | 0.0% | 0.0% | -52.1% | 0.0% |
| 18 | 92.2% | 97.7% | 2.3% | 5.5% | 0.0% | 30.7% | 0.0% | 0.0% | -30.7% | 0.0% |
| 20 | 97.1% | 97.7% | 2.3% | 0.6% | 0.0% | 4.9% | 0.0% | 0.0% | -4.9% | 0.0% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.6% | 0.0% | 0.0% | -0.6% | 0.0% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=0 B=12

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 9.4% | 97.7% | 2.3% | 88.3% | 0.0% | N/A | N/A | N/A | N/A | N/A |
| 16 | 61.5% | 97.7% | 2.3% | 36.2% | 0.0% | 52.1% | 0.0% | 0.0% | -52.1% | 0.0% |
| 18 | 92.2% | 97.7% | 2.3% | 5.5% | 0.0% | 30.7% | 0.0% | 0.0% | -30.7% | 0.0% |
| 20 | 97.1% | 97.7% | 2.3% | 0.6% | 0.0% | 4.9% | 0.0% | 0.0% | -4.9% | 0.0% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.6% | 0.0% | 0.0% | -0.6% | 0.0% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=1 B=8

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 8.0% | 97.5% | 2.5% | 89.3% | 63.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 58.7% | 97.6% | 2.4% | 38.9% | 32.4% | 50.7% | 0.1% | -0.1% | -50.4% | -31.5% |
| 18 | 91.7% | 97.7% | 2.3% | 6.0% | 4.9% | 33.0% | 0.1% | -0.1% | -32.9% | -27.5% |
| 20 | 97.1% | 97.7% | 2.3% | 0.6% | 0.5% | 5.4% | 0.0% | 0.0% | -5.4% | -4.4% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.6% | 0.0% | 0.0% | -0.6% | -0.5% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=1 B=10

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 8.0% | 97.6% | 2.4% | 89.6% | 63.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 58.7% | 97.7% | 2.3% | 39.0% | 32.4% | 50.7% | 0.1% | -0.1% | -50.6% | -31.5% |
| 18 | 91.7% | 97.7% | 2.3% | 6.0% | 4.9% | 33.0% | 0.0% | 0.0% | -33.0% | -27.5% |
| 20 | 97.1% | 97.7% | 2.3% | 0.6% | 0.5% | 5.4% | 0.0% | 0.0% | -5.4% | -4.4% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.6% | 0.0% | 0.0% | -0.6% | -0.5% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=1 B=12

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 8.0% | 97.7% | 2.3% | 89.7% | 63.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 58.7% | 97.7% | 2.3% | 39.0% | 32.4% | 50.7% | 0.0% | 0.0% | -50.7% | -31.5% |
| 18 | 91.7% | 97.7% | 2.3% | 6.0% | 4.9% | 33.0% | 0.0% | 0.0% | -33.0% | -27.5% |
| 20 | 97.1% | 97.7% | 2.3% | 0.6% | 0.5% | 5.4% | 0.0% | 0.0% | -5.4% | -4.4% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.6% | 0.0% | 0.0% | -0.6% | -0.5% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=3 B=8

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 13.0% | 97.7% | 2.3% | 84.3% | 79.8% | N/A | N/A | N/A | N/A | N/A |
| 16 | 63.4% | 97.7% | 2.3% | 34.3% | 34.1% | 50.4% | 0.0% | 0.0% | -50.0% | -45.7% |
| 18 | 92.7% | 97.7% | 2.3% | 5.0% | 5.0% | 29.3% | 0.0% | 0.0% | -29.3% | -29.1% |
| 20 | 97.4% | 97.7% | 2.3% | 0.3% | 0.3% | 4.7% | 0.0% | 0.0% | -4.7% | -4.7% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.3% | 0.0% | 0.0% | -0.3% | -0.3% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=3 B=10

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 13.0% | 97.7% | 2.3% | 84.6% | 79.8% | N/A | N/A | N/A | N/A | N/A |
| 16 | 63.4% | 97.7% | 2.3% | 34.3% | 34.1% | 50.4% | 0.0% | 0.0% | -50.3% | -45.7% |
| 18 | 92.7% | 97.7% | 2.3% | 5.0% | 5.0% | 29.3% | 0.0% | 0.0% | -29.3% | -29.1% |
| 20 | 97.4% | 97.7% | 2.3% | 0.3% | 0.3% | 4.7% | 0.0% | 0.0% | -4.7% | -4.7% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.3% | 0.0% | 0.0% | -0.3% | -0.3% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.10 R=3 B=12

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 13.0% | 97.7% | 2.3% | 84.6% | 79.8% | N/A | N/A | N/A | N/A | N/A |
| 16 | 63.4% | 97.7% | 2.3% | 34.3% | 34.1% | 50.4% | 0.0% | 0.0% | -50.3% | -45.7% |
| 18 | 92.7% | 97.7% | 2.3% | 5.0% | 5.0% | 29.3% | 0.0% | 0.0% | -29.3% | -29.1% |
| 20 | 97.4% | 97.7% | 2.3% | 0.3% | 0.3% | 4.7% | 0.0% | 0.0% | -4.7% | -4.7% |
| 22 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.3% | 0.0% | 0.0% | -0.3% | -0.3% |
| 24 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

### density=0.15 R=0 B=8

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 5.6% | 92.3% | 7.7% | 86.7% | 0.0% | N/A | N/A | N/A | N/A | N/A |
| 16 | 44.7% | 93.5% | 6.5% | 48.8% | 0.0% | 39.1% | 1.2% | -1.2% | -37.9% | 0.0% |
| 18 | 81.1% | 93.8% | 6.2% | 12.7% | 0.0% | 36.4% | 0.3% | -0.3% | -36.1% | 0.0% |
| 20 | 92.3% | 93.8% | 6.2% | 1.5% | 0.0% | 11.2% | 0.0% | 0.0% | -11.2% | 0.0% |
| 22 | 93.7% | 93.8% | 6.2% | 0.1% | 0.0% | 1.4% | 0.0% | 0.0% | -1.4% | 0.0% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% | -0.1% | 0.0% |

### density=0.15 R=0 B=10

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 5.6% | 93.4% | 6.6% | 87.8% | 0.0% | N/A | N/A | N/A | N/A | N/A |
| 16 | 44.7% | 93.6% | 6.4% | 48.9% | 0.0% | 39.1% | 0.2% | -0.2% | -38.9% | 0.0% |
| 18 | 81.1% | 93.8% | 6.2% | 12.7% | 0.0% | 36.4% | 0.2% | -0.2% | -36.2% | 0.0% |
| 20 | 92.3% | 93.8% | 6.2% | 1.5% | 0.0% | 11.2% | 0.0% | 0.0% | -11.2% | 0.0% |
| 22 | 93.7% | 93.8% | 6.2% | 0.1% | 0.0% | 1.4% | 0.0% | 0.0% | -1.4% | 0.0% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% | -0.1% | 0.0% |

### density=0.15 R=0 B=12

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 5.6% | 93.5% | 6.5% | 87.9% | 0.0% | N/A | N/A | N/A | N/A | N/A |
| 16 | 44.7% | 93.6% | 6.4% | 48.9% | 0.0% | 39.1% | 0.1% | -0.1% | -39.0% | 0.0% |
| 18 | 81.1% | 93.8% | 6.2% | 12.7% | 0.0% | 36.4% | 0.2% | -0.2% | -36.2% | 0.0% |
| 20 | 92.3% | 93.8% | 6.2% | 1.5% | 0.0% | 11.2% | 0.0% | 0.0% | -11.2% | 0.0% |
| 22 | 93.7% | 93.8% | 6.2% | 0.1% | 0.0% | 1.4% | 0.0% | 0.0% | -1.4% | 0.0% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% | -0.1% | 0.0% |

### density=0.15 R=1 B=8

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 4.1% | 92.5% | 7.5% | 87.2% | 51.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 46.0% | 93.5% | 6.5% | 47.3% | 34.4% | 41.9% | 1.0% | -1.0% | -39.9% | -17.5% |
| 18 | 80.6% | 93.8% | 6.2% | 13.2% | 9.4% | 34.6% | 0.3% | -0.3% | -34.1% | -25.0% |
| 20 | 91.8% | 93.8% | 6.2% | 2.0% | 1.7% | 11.2% | 0.0% | 0.0% | -11.2% | -7.7% |
| 22 | 93.5% | 93.8% | 6.2% | 0.3% | 0.3% | 1.7% | 0.0% | 0.0% | -1.7% | -1.4% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.3% | 0.0% | 0.0% | -0.3% | -0.3% |

### density=0.15 R=1 B=10

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 4.1% | 93.3% | 6.7% | 89.0% | 51.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 46.0% | 93.7% | 6.3% | 47.6% | 34.4% | 41.9% | 0.4% | -0.4% | -41.4% | -17.5% |
| 18 | 80.6% | 93.8% | 6.2% | 13.2% | 9.4% | 34.6% | 0.1% | -0.1% | -34.4% | -25.0% |
| 20 | 91.8% | 93.8% | 6.2% | 2.0% | 1.7% | 11.2% | 0.0% | 0.0% | -11.2% | -7.7% |
| 22 | 93.5% | 93.8% | 6.2% | 0.3% | 0.3% | 1.7% | 0.0% | 0.0% | -1.7% | -1.4% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.3% | 0.0% | 0.0% | -0.3% | -0.3% |

### density=0.15 R=1 B=12

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 4.1% | 93.5% | 6.5% | 89.3% | 51.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 46.0% | 93.7% | 6.3% | 47.6% | 34.4% | 41.9% | 0.2% | -0.2% | -41.7% | -17.5% |
| 18 | 80.6% | 93.8% | 6.2% | 13.2% | 9.4% | 34.6% | 0.1% | -0.1% | -34.4% | -25.0% |
| 20 | 91.8% | 93.8% | 6.2% | 2.0% | 1.7% | 11.2% | 0.0% | 0.0% | -11.2% | -7.7% |
| 22 | 93.5% | 93.8% | 6.2% | 0.3% | 0.3% | 1.7% | 0.0% | 0.0% | -1.7% | -1.4% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.3% | 0.0% | 0.0% | -0.3% | -0.3% |

### density=0.15 R=3 B=8

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 8.3% | 92.9% | 7.1% | 83.5% | 74.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 48.9% | 93.8% | 6.2% | 44.5% | 42.4% | 40.6% | 0.9% | -0.9% | -39.0% | -32.5% |
| 18 | 82.0% | 93.8% | 6.2% | 11.8% | 11.3% | 33.1% | 0.0% | 0.0% | -32.7% | -31.1% |
| 20 | 92.3% | 93.8% | 6.2% | 1.5% | 1.5% | 10.3% | 0.0% | 0.0% | -10.3% | -9.8% |
| 22 | 93.7% | 93.8% | 6.2% | 0.1% | 0.1% | 1.4% | 0.0% | 0.0% | -1.4% | -1.4% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% | -0.1% | -0.1% |

### density=0.15 R=3 B=10

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 8.3% | 93.6% | 6.4% | 84.8% | 74.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 48.9% | 93.8% | 6.2% | 44.7% | 42.4% | 40.6% | 0.2% | -0.2% | -40.1% | -32.5% |
| 18 | 82.0% | 93.8% | 6.2% | 11.8% | 11.3% | 33.1% | 0.0% | 0.0% | -32.9% | -31.1% |
| 20 | 92.3% | 93.8% | 6.2% | 1.5% | 1.5% | 10.3% | 0.0% | 0.0% | -10.3% | -9.8% |
| 22 | 93.7% | 93.8% | 6.2% | 0.1% | 0.1% | 1.4% | 0.0% | 0.0% | -1.4% | -1.4% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% | -0.1% | -0.1% |

### density=0.15 R=3 B=12

| initial | direct | any | still | base rescue | resource rescue | direct delta | any delta | still delta | base delta | resource delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 14 | 8.3% | 93.6% | 6.4% | 85.0% | 74.9% | N/A | N/A | N/A | N/A | N/A |
| 16 | 48.9% | 93.8% | 6.2% | 44.7% | 42.4% | 40.6% | 0.2% | -0.2% | -40.3% | -32.5% |
| 18 | 82.0% | 93.8% | 6.2% | 11.8% | 11.3% | 33.1% | 0.0% | 0.0% | -32.9% | -31.1% |
| 20 | 92.3% | 93.8% | 6.2% | 1.5% | 1.5% | 10.3% | 0.0% | 0.0% | -10.3% | -9.8% |
| 22 | 93.7% | 93.8% | 6.2% | 0.1% | 0.1% | 1.4% | 0.0% | 0.0% | -1.4% | -1.4% |
| 24 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% | -0.1% | -0.1% |


## Base Supply Comparison

### density=0.10 R=0 initial=14

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.1% | 2.9% | 87.7% | 0.0% | 87.7% | 0.0% | 100.0% | 5 | 3 |
| 10 | 97.7% | 2.3% | 88.3% | 0.0% | 88.3% | 0.0% | 100.0% | 7 | 3 |
| 12 | 97.7% | 2.3% | 88.3% | 0.0% | 88.3% | 0.0% | 100.0% | 9 | 3 |

### density=0.10 R=0 initial=16

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 36.2% | 0.0% | 36.2% | 0.0% | 100.0% | 7 | 0 |
| 10 | 97.7% | 2.3% | 36.2% | 0.0% | 36.2% | 0.0% | 100.0% | 9 | 0 |
| 12 | 97.7% | 2.3% | 36.2% | 0.0% | 36.2% | 0.0% | 100.0% | 11 | 0 |

### density=0.10 R=0 initial=18

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 5.5% | 0.0% | 5.5% | 0.0% | 100.0% | 9 | 0 |
| 10 | 97.7% | 2.3% | 5.5% | 0.0% | 5.5% | 0.0% | 100.0% | 11 | 0 |
| 12 | 97.7% | 2.3% | 5.5% | 0.0% | 5.5% | 0.0% | 100.0% | 13 | 0 |

### density=0.10 R=0 initial=20

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.6% | 0.0% | 0.6% | 0.0% | 100.0% | 11 | 0 |
| 10 | 97.7% | 2.3% | 0.6% | 0.0% | 0.6% | 0.0% | 100.0% | 13 | 0 |
| 12 | 97.7% | 2.3% | 0.6% | 0.0% | 0.6% | 0.0% | 100.0% | 15 | 0 |

### density=0.10 R=0 initial=22

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 13 | 0 |
| 10 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 12 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |

### density=0.10 R=0 initial=24

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 10 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |
| 12 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 19 | 0 |

### density=0.10 R=1 initial=14

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.5% | 2.5% | 89.3% | 63.9% | 25.6% | 63.7% | 28.6% | 5 | 2 |
| 10 | 97.6% | 2.4% | 89.6% | 63.9% | 25.7% | 63.9% | 28.7% | 7 | 2 |
| 12 | 97.7% | 2.3% | 89.7% | 63.9% | 25.8% | 63.9% | 28.8% | 9 | 2 |

### density=0.10 R=1 initial=16

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.6% | 2.4% | 38.9% | 32.4% | 6.5% | 32.4% | 16.7% | 7 | 0 |
| 10 | 97.7% | 2.3% | 39.0% | 32.4% | 6.6% | 32.4% | 16.9% | 9 | 0 |
| 12 | 97.7% | 2.3% | 39.0% | 32.4% | 6.6% | 32.4% | 16.9% | 11 | 0 |

### density=0.10 R=1 initial=18

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 6.0% | 4.9% | 1.1% | 4.9% | 18.3% | 9 | 0 |
| 10 | 97.7% | 2.3% | 6.0% | 4.9% | 1.1% | 4.9% | 18.3% | 11 | 0 |
| 12 | 97.7% | 2.3% | 6.0% | 4.9% | 1.1% | 4.9% | 18.3% | 13 | 0 |

### density=0.10 R=1 initial=20

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.6% | 0.5% | 0.1% | 0.5% | 16.7% | 11 | 0 |
| 10 | 97.7% | 2.3% | 0.6% | 0.5% | 0.1% | 0.5% | 16.7% | 13 | 0 |
| 12 | 97.7% | 2.3% | 0.6% | 0.5% | 0.1% | 0.5% | 16.7% | 15 | 0 |

### density=0.10 R=1 initial=22

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 13 | 0 |
| 10 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 12 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |

### density=0.10 R=1 initial=24

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 10 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |
| 12 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 19 | 0 |

### density=0.10 R=3 initial=14

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 84.3% | 79.8% | 4.9% | 79.4% | 5.8% | 6 | 2 |
| 10 | 97.7% | 2.3% | 84.6% | 79.8% | 4.9% | 79.7% | 5.8% | 8 | 2 |
| 12 | 97.7% | 2.3% | 84.6% | 79.8% | 4.9% | 79.7% | 5.8% | 10 | 2 |

### density=0.10 R=3 initial=16

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 34.3% | 34.1% | 0.2% | 34.1% | 0.6% | 8 | 0 |
| 10 | 97.7% | 2.3% | 34.3% | 34.1% | 0.2% | 34.1% | 0.6% | 10 | 0 |
| 12 | 97.7% | 2.3% | 34.3% | 34.1% | 0.2% | 34.1% | 0.6% | 12 | 0 |

### density=0.10 R=3 initial=18

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 5.0% | 5.0% | 0.0% | 5.0% | 0.0% | 10 | 0 |
| 10 | 97.7% | 2.3% | 5.0% | 5.0% | 0.0% | 5.0% | 0.0% | 12 | 0 |
| 12 | 97.7% | 2.3% | 5.0% | 5.0% | 0.0% | 5.0% | 0.0% | 14 | 0 |

### density=0.10 R=3 initial=20

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.3% | 0.3% | 0.0% | 0.3% | 0.0% | 12 | 0 |
| 10 | 97.7% | 2.3% | 0.3% | 0.3% | 0.0% | 0.3% | 0.0% | 14 | 0 |
| 12 | 97.7% | 2.3% | 0.3% | 0.3% | 0.0% | 0.3% | 0.0% | 16 | 0 |

### density=0.10 R=3 initial=22

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 14 | 0 |
| 10 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 16 | 0 |
| 12 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 18 | 0 |

### density=0.10 R=3 initial=24

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 16 | 0 |
| 10 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 18 | 0 |
| 12 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 20 | 0 |

### density=0.15 R=0 initial=14

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 92.3% | 7.7% | 86.7% | 0.0% | 86.7% | 0.0% | 100.0% | 5 | 3 |
| 10 | 93.4% | 6.6% | 87.8% | 0.0% | 87.8% | 0.0% | 100.0% | 7 | 3 |
| 12 | 93.5% | 6.5% | 87.9% | 0.0% | 87.9% | 0.0% | 100.0% | 9 | 3 |

### density=0.15 R=0 initial=16

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.5% | 6.5% | 48.8% | 0.0% | 48.8% | 0.0% | 100.0% | 7 | 1 |
| 10 | 93.6% | 6.4% | 48.9% | 0.0% | 48.9% | 0.0% | 100.0% | 9 | 1 |
| 12 | 93.6% | 6.4% | 48.9% | 0.0% | 48.9% | 0.0% | 100.0% | 11 | 1 |

### density=0.15 R=0 initial=18

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 12.7% | 0.0% | 12.7% | 0.0% | 100.0% | 9 | 0 |
| 10 | 93.8% | 6.2% | 12.7% | 0.0% | 12.7% | 0.0% | 100.0% | 11 | 0 |
| 12 | 93.8% | 6.2% | 12.7% | 0.0% | 12.7% | 0.0% | 100.0% | 13 | 0 |

### density=0.15 R=0 initial=20

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 100.0% | 11 | 0 |
| 10 | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 100.0% | 13 | 0 |
| 12 | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 100.0% | 15 | 0 |

### density=0.15 R=0 initial=22

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 100.0% | 13 | 0 |
| 10 | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 100.0% | 15 | 0 |
| 12 | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 100.0% | 17 | 0 |

### density=0.15 R=0 initial=24

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 10 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |
| 12 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 19 | 0 |

### density=0.15 R=1 initial=14

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 92.5% | 7.5% | 87.2% | 51.9% | 36.5% | 50.7% | 41.3% | 5 | 3 |
| 10 | 93.3% | 6.7% | 89.0% | 51.9% | 37.3% | 51.7% | 41.8% | 7 | 3 |
| 12 | 93.5% | 6.5% | 89.3% | 51.9% | 37.5% | 51.8% | 41.9% | 9 | 3 |

### density=0.15 R=1 initial=16

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.5% | 6.5% | 47.3% | 34.4% | 13.1% | 34.2% | 27.6% | 7 | 1 |
| 10 | 93.7% | 6.3% | 47.6% | 34.4% | 13.3% | 34.3% | 27.9% | 9 | 1 |
| 12 | 93.7% | 6.3% | 47.6% | 34.4% | 13.3% | 34.3% | 27.9% | 11 | 1 |

### density=0.15 R=1 initial=18

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 13.2% | 9.4% | 3.8% | 9.4% | 28.8% | 9 | 0 |
| 10 | 93.8% | 6.2% | 13.2% | 9.4% | 3.8% | 9.4% | 28.8% | 11 | 0 |
| 12 | 93.8% | 6.2% | 13.2% | 9.4% | 3.8% | 9.4% | 28.8% | 13 | 0 |

### density=0.15 R=1 initial=20

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 2.0% | 1.7% | 0.3% | 1.7% | 15.0% | 11 | 0 |
| 10 | 93.8% | 6.2% | 2.0% | 1.7% | 0.3% | 1.7% | 15.0% | 13 | 0 |
| 12 | 93.8% | 6.2% | 2.0% | 1.7% | 0.3% | 1.7% | 15.0% | 15 | 0 |

### density=0.15 R=1 initial=22

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 0.3% | 0.3% | 0.0% | 0.3% | 0.0% | 13 | 0 |
| 10 | 93.8% | 6.2% | 0.3% | 0.3% | 0.0% | 0.3% | 0.0% | 15 | 0 |
| 12 | 93.8% | 6.2% | 0.3% | 0.3% | 0.0% | 0.3% | 0.0% | 17 | 0 |

### density=0.15 R=1 initial=24

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 10 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |
| 12 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 19 | 0 |

### density=0.15 R=3 initial=14

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 92.9% | 7.1% | 83.5% | 74.9% | 9.7% | 73.8% | 11.5% | 5 | 3 |
| 10 | 93.6% | 6.4% | 84.8% | 74.9% | 10.4% | 74.4% | 12.2% | 7 | 3 |
| 12 | 93.6% | 6.4% | 85.0% | 74.9% | 10.4% | 74.6% | 12.2% | 9 | 3 |

### density=0.15 R=3 initial=16

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 44.5% | 42.4% | 2.5% | 42.0% | 5.6% | 7 | 0 |
| 10 | 93.8% | 6.2% | 44.7% | 42.4% | 2.5% | 42.2% | 5.6% | 9 | 0 |
| 12 | 93.8% | 6.2% | 44.7% | 42.4% | 2.5% | 42.2% | 5.6% | 11 | 0 |

### density=0.15 R=3 initial=18

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 11.8% | 11.3% | 0.5% | 11.3% | 4.2% | 9 | 0 |
| 10 | 93.8% | 6.2% | 11.8% | 11.3% | 0.5% | 11.3% | 4.2% | 11 | 0 |
| 12 | 93.8% | 6.2% | 11.8% | 11.3% | 0.5% | 11.3% | 4.2% | 13 | 0 |

### density=0.15 R=3 initial=20

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 1.5% | 1.5% | 0.0% | 1.5% | 0.0% | 11 | 0 |
| 10 | 93.8% | 6.2% | 1.5% | 1.5% | 0.0% | 1.5% | 0.0% | 13 | 0 |
| 12 | 93.8% | 6.2% | 1.5% | 1.5% | 0.0% | 1.5% | 0.0% | 15 | 0 |

### density=0.15 R=3 initial=22

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 0.1% | 0.1% | 0.0% | 0.1% | 0.0% | 13 | 0 |
| 10 | 93.8% | 6.2% | 0.1% | 0.1% | 0.0% | 0.1% | 0.0% | 15 | 0 |
| 12 | 93.8% | 6.2% | 0.1% | 0.1% | 0.0% | 0.1% | 0.0% | 17 | 0 |

### density=0.15 R=3 initial=24

| B supply | any | still | base rescue | resource rescue | base only | both | base-only share among rescued | remain med | required med |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 15 | 0 |
| 10 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 17 | 0 |
| 12 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | N/A | 19 | 0 |


## Resource Count Comparison

### density=0.10 initial=14 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.1% | 2.9% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.5% | 2.5% | 63.9% | 0.2% | 63.7% | 0.4% | -0.4% | 21 |
| 3 | 97.7% | 2.3% | 79.8% | 0.4% | 79.4% | 0.6% | -0.6% | 18 |

### density=0.10 initial=14 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.6% | 2.4% | 63.9% | 0.0% | 63.9% | -0.1% | 0.1% | 21 |
| 3 | 97.7% | 2.3% | 79.8% | 0.1% | 79.7% | 0.0% | 0.0% | 18 |

### density=0.10 initial=14 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 63.9% | 0.0% | 63.9% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 79.8% | 0.1% | 79.7% | 0.0% | 0.0% | 18 |

### density=0.10 initial=16 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.6% | 2.4% | 32.4% | 0.0% | 32.4% | -0.1% | 0.1% | 21 |
| 3 | 97.7% | 2.3% | 34.1% | 0.0% | 34.1% | 0.0% | 0.0% | 18 |

### density=0.10 initial=16 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 32.4% | 0.0% | 32.4% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 34.1% | 0.0% | 34.1% | 0.0% | 0.0% | 18 |

### density=0.10 initial=16 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 32.4% | 0.0% | 32.4% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 34.1% | 0.0% | 34.1% | 0.0% | 0.0% | 18 |

### density=0.10 initial=18 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 4.9% | 0.0% | 4.9% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 5.0% | 0.0% | 5.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=18 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 4.9% | 0.0% | 4.9% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 5.0% | 0.0% | 5.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=18 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 4.9% | 0.0% | 4.9% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 5.0% | 0.0% | 5.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=20 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.5% | 0.0% | 0.5% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.3% | 0.0% | 0.3% | 0.0% | 0.0% | 18 |

### density=0.10 initial=20 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.5% | 0.0% | 0.5% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.3% | 0.0% | 0.3% | 0.0% | 0.0% | 18 |

### density=0.10 initial=20 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.5% | 0.0% | 0.5% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.3% | 0.0% | 0.3% | 0.0% | 0.0% | 18 |

### density=0.10 initial=22 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=22 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=22 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=24 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=24 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 18 |

### density=0.10 initial=24 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 21 |
| 3 | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 18 |

### density=0.15 initial=14 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 92.3% | 7.7% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 92.5% | 7.5% | 51.9% | 1.2% | 50.7% | 0.2% | -0.2% | 23 |
| 3 | 92.9% | 7.1% | 74.9% | 1.1% | 73.8% | 0.6% | -0.6% | 19 |

### density=0.15 initial=14 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.4% | 6.6% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.3% | 6.7% | 51.9% | 0.2% | 51.7% | -0.1% | 0.1% | 23 |
| 3 | 93.6% | 6.4% | 74.9% | 0.5% | 74.4% | 0.2% | -0.2% | 19 |

### density=0.15 initial=14 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.5% | 6.5% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.5% | 6.5% | 51.9% | 0.1% | 51.8% | 0.0% | 0.0% | 23 |
| 3 | 93.6% | 6.4% | 74.9% | 0.3% | 74.6% | 0.1% | -0.1% | 19 |

### density=0.15 initial=16 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.5% | 6.5% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.5% | 6.5% | 34.4% | 0.2% | 34.2% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 42.4% | 0.4% | 42.0% | 0.3% | -0.3% | 19 |

### density=0.15 initial=16 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.6% | 6.4% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.7% | 6.3% | 34.4% | 0.1% | 34.3% | 0.1% | -0.1% | 23 |
| 3 | 93.8% | 6.2% | 42.4% | 0.2% | 42.2% | 0.2% | -0.2% | 19 |

### density=0.15 initial=16 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.6% | 6.4% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.7% | 6.3% | 34.4% | 0.1% | 34.3% | 0.1% | -0.1% | 23 |
| 3 | 93.8% | 6.2% | 42.4% | 0.2% | 42.2% | 0.2% | -0.2% | 19 |

### density=0.15 initial=18 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 9.4% | 0.0% | 9.4% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 11.3% | 0.0% | 11.3% | 0.0% | 0.0% | 19 |

### density=0.15 initial=18 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 9.4% | 0.0% | 9.4% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 11.3% | 0.0% | 11.3% | 0.0% | 0.0% | 19 |

### density=0.15 initial=18 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 9.4% | 0.0% | 9.4% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 11.3% | 0.0% | 11.3% | 0.0% | 0.0% | 19 |

### density=0.15 initial=20 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 1.7% | 0.0% | 1.7% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 0.0% | 19 |

### density=0.15 initial=20 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 1.7% | 0.0% | 1.7% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 0.0% | 19 |

### density=0.15 initial=20 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 1.7% | 0.0% | 1.7% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 0.0% | 19 |

### density=0.15 initial=22 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 0.3% | 0.0% | 0.3% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 0.0% | 19 |

### density=0.15 initial=22 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 0.3% | 0.0% | 0.3% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 0.0% | 19 |

### density=0.15 initial=22 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 0.3% | 0.0% | 0.3% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 0.0% | 19 |

### density=0.15 initial=24 B=8

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 19 |

### density=0.15 initial=24 B=10

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 19 |

### density=0.15 initial=24 B=12

| R count | any | still | resource rescue | resource only | both | delta any vs R0 | delta still vs R0 | resource cost p90 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | N/A |
| 1 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 23 |
| 3 | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 19 |


## Summary Table

FUEL COMPARISON

| density | R | init | B | direct | any | still | base rescue | resource rescue | base only | resource only | both | req supply med | remain med | resource cost p90 | delta any vs R0 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0.10 | 0 | 14 | 8 | 9.4% | 97.1% | 2.9% | 87.7% | 0.0% | 87.7% | 0.0% | 0.0% | 3 | 5 | N/A | 0.0% |
| 0.10 | 0 | 14 | 10 | 9.4% | 97.7% | 2.3% | 88.3% | 0.0% | 88.3% | 0.0% | 0.0% | 3 | 7 | N/A | 0.0% |
| 0.10 | 0 | 14 | 12 | 9.4% | 97.7% | 2.3% | 88.3% | 0.0% | 88.3% | 0.0% | 0.0% | 3 | 9 | N/A | 0.0% |
| 0.10 | 0 | 16 | 8 | 61.5% | 97.7% | 2.3% | 36.2% | 0.0% | 36.2% | 0.0% | 0.0% | 0 | 7 | N/A | 0.0% |
| 0.10 | 0 | 16 | 10 | 61.5% | 97.7% | 2.3% | 36.2% | 0.0% | 36.2% | 0.0% | 0.0% | 0 | 9 | N/A | 0.0% |
| 0.10 | 0 | 16 | 12 | 61.5% | 97.7% | 2.3% | 36.2% | 0.0% | 36.2% | 0.0% | 0.0% | 0 | 11 | N/A | 0.0% |
| 0.10 | 0 | 18 | 8 | 92.2% | 97.7% | 2.3% | 5.5% | 0.0% | 5.5% | 0.0% | 0.0% | 0 | 9 | N/A | 0.0% |
| 0.10 | 0 | 18 | 10 | 92.2% | 97.7% | 2.3% | 5.5% | 0.0% | 5.5% | 0.0% | 0.0% | 0 | 11 | N/A | 0.0% |
| 0.10 | 0 | 18 | 12 | 92.2% | 97.7% | 2.3% | 5.5% | 0.0% | 5.5% | 0.0% | 0.0% | 0 | 13 | N/A | 0.0% |
| 0.10 | 0 | 20 | 8 | 97.1% | 97.7% | 2.3% | 0.6% | 0.0% | 0.6% | 0.0% | 0.0% | 0 | 11 | N/A | 0.0% |
| 0.10 | 0 | 20 | 10 | 97.1% | 97.7% | 2.3% | 0.6% | 0.0% | 0.6% | 0.0% | 0.0% | 0 | 13 | N/A | 0.0% |
| 0.10 | 0 | 20 | 12 | 97.1% | 97.7% | 2.3% | 0.6% | 0.0% | 0.6% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.10 | 0 | 22 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 13 | N/A | 0.0% |
| 0.10 | 0 | 22 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.10 | 0 | 22 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | N/A | 0.0% |
| 0.10 | 0 | 24 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.10 | 0 | 24 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | N/A | 0.0% |
| 0.10 | 0 | 24 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | N/A | 0.0% |
| 0.10 | 1 | 14 | 8 | 8.0% | 97.5% | 2.5% | 89.3% | 63.9% | 25.6% | 0.2% | 63.7% | 2 | 5 | 21 | 0.4% |
| 0.10 | 1 | 14 | 10 | 8.0% | 97.6% | 2.4% | 89.6% | 63.9% | 25.7% | 0.0% | 63.9% | 2 | 7 | 21 | -0.1% |
| 0.10 | 1 | 14 | 12 | 8.0% | 97.7% | 2.3% | 89.7% | 63.9% | 25.8% | 0.0% | 63.9% | 2 | 9 | 21 | 0.0% |
| 0.10 | 1 | 16 | 8 | 58.7% | 97.6% | 2.4% | 38.9% | 32.4% | 6.5% | 0.0% | 32.4% | 0 | 7 | 21 | -0.1% |
| 0.10 | 1 | 16 | 10 | 58.7% | 97.7% | 2.3% | 39.0% | 32.4% | 6.6% | 0.0% | 32.4% | 0 | 9 | 21 | 0.0% |
| 0.10 | 1 | 16 | 12 | 58.7% | 97.7% | 2.3% | 39.0% | 32.4% | 6.6% | 0.0% | 32.4% | 0 | 11 | 21 | 0.0% |
| 0.10 | 1 | 18 | 8 | 91.7% | 97.7% | 2.3% | 6.0% | 4.9% | 1.1% | 0.0% | 4.9% | 0 | 9 | 21 | 0.0% |
| 0.10 | 1 | 18 | 10 | 91.7% | 97.7% | 2.3% | 6.0% | 4.9% | 1.1% | 0.0% | 4.9% | 0 | 11 | 21 | 0.0% |
| 0.10 | 1 | 18 | 12 | 91.7% | 97.7% | 2.3% | 6.0% | 4.9% | 1.1% | 0.0% | 4.9% | 0 | 13 | 21 | 0.0% |
| 0.10 | 1 | 20 | 8 | 97.1% | 97.7% | 2.3% | 0.6% | 0.5% | 0.1% | 0.0% | 0.5% | 0 | 11 | 21 | 0.0% |
| 0.10 | 1 | 20 | 10 | 97.1% | 97.7% | 2.3% | 0.6% | 0.5% | 0.1% | 0.0% | 0.5% | 0 | 13 | 21 | 0.0% |
| 0.10 | 1 | 20 | 12 | 97.1% | 97.7% | 2.3% | 0.6% | 0.5% | 0.1% | 0.0% | 0.5% | 0 | 15 | 21 | 0.0% |
| 0.10 | 1 | 22 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 13 | 21 | 0.0% |
| 0.10 | 1 | 22 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 21 | 0.0% |
| 0.10 | 1 | 22 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 21 | 0.0% |
| 0.10 | 1 | 24 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 21 | 0.0% |
| 0.10 | 1 | 24 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 21 | 0.0% |
| 0.10 | 1 | 24 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 21 | 0.0% |
| 0.10 | 3 | 14 | 8 | 13.0% | 97.7% | 2.3% | 84.3% | 79.8% | 4.9% | 0.4% | 79.4% | 2 | 6 | 18 | 0.6% |
| 0.10 | 3 | 14 | 10 | 13.0% | 97.7% | 2.3% | 84.6% | 79.8% | 4.9% | 0.1% | 79.7% | 2 | 8 | 18 | 0.0% |
| 0.10 | 3 | 14 | 12 | 13.0% | 97.7% | 2.3% | 84.6% | 79.8% | 4.9% | 0.1% | 79.7% | 2 | 10 | 18 | 0.0% |
| 0.10 | 3 | 16 | 8 | 63.4% | 97.7% | 2.3% | 34.3% | 34.1% | 0.2% | 0.0% | 34.1% | 0 | 8 | 18 | 0.0% |
| 0.10 | 3 | 16 | 10 | 63.4% | 97.7% | 2.3% | 34.3% | 34.1% | 0.2% | 0.0% | 34.1% | 0 | 10 | 18 | 0.0% |
| 0.10 | 3 | 16 | 12 | 63.4% | 97.7% | 2.3% | 34.3% | 34.1% | 0.2% | 0.0% | 34.1% | 0 | 12 | 18 | 0.0% |
| 0.10 | 3 | 18 | 8 | 92.7% | 97.7% | 2.3% | 5.0% | 5.0% | 0.0% | 0.0% | 5.0% | 0 | 10 | 18 | 0.0% |
| 0.10 | 3 | 18 | 10 | 92.7% | 97.7% | 2.3% | 5.0% | 5.0% | 0.0% | 0.0% | 5.0% | 0 | 12 | 18 | 0.0% |
| 0.10 | 3 | 18 | 12 | 92.7% | 97.7% | 2.3% | 5.0% | 5.0% | 0.0% | 0.0% | 5.0% | 0 | 14 | 18 | 0.0% |
| 0.10 | 3 | 20 | 8 | 97.4% | 97.7% | 2.3% | 0.3% | 0.3% | 0.0% | 0.0% | 0.3% | 0 | 12 | 18 | 0.0% |
| 0.10 | 3 | 20 | 10 | 97.4% | 97.7% | 2.3% | 0.3% | 0.3% | 0.0% | 0.0% | 0.3% | 0 | 14 | 18 | 0.0% |
| 0.10 | 3 | 20 | 12 | 97.4% | 97.7% | 2.3% | 0.3% | 0.3% | 0.0% | 0.0% | 0.3% | 0 | 16 | 18 | 0.0% |
| 0.10 | 3 | 22 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 14 | 18 | 0.0% |
| 0.10 | 3 | 22 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 16 | 18 | 0.0% |
| 0.10 | 3 | 22 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | 18 | 0.0% |
| 0.10 | 3 | 24 | 8 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 16 | 18 | 0.0% |
| 0.10 | 3 | 24 | 10 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 18 | 18 | 0.0% |
| 0.10 | 3 | 24 | 12 | 97.7% | 97.7% | 2.3% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 20 | 18 | 0.0% |
| 0.15 | 0 | 14 | 8 | 5.6% | 92.3% | 7.7% | 86.7% | 0.0% | 86.7% | 0.0% | 0.0% | 3 | 5 | N/A | 0.0% |
| 0.15 | 0 | 14 | 10 | 5.6% | 93.4% | 6.6% | 87.8% | 0.0% | 87.8% | 0.0% | 0.0% | 3 | 7 | N/A | 0.0% |
| 0.15 | 0 | 14 | 12 | 5.6% | 93.5% | 6.5% | 87.9% | 0.0% | 87.9% | 0.0% | 0.0% | 3 | 9 | N/A | 0.0% |
| 0.15 | 0 | 16 | 8 | 44.7% | 93.5% | 6.5% | 48.8% | 0.0% | 48.8% | 0.0% | 0.0% | 1 | 7 | N/A | 0.0% |
| 0.15 | 0 | 16 | 10 | 44.7% | 93.6% | 6.4% | 48.9% | 0.0% | 48.9% | 0.0% | 0.0% | 1 | 9 | N/A | 0.0% |
| 0.15 | 0 | 16 | 12 | 44.7% | 93.6% | 6.4% | 48.9% | 0.0% | 48.9% | 0.0% | 0.0% | 1 | 11 | N/A | 0.0% |
| 0.15 | 0 | 18 | 8 | 81.1% | 93.8% | 6.2% | 12.7% | 0.0% | 12.7% | 0.0% | 0.0% | 0 | 9 | N/A | 0.0% |
| 0.15 | 0 | 18 | 10 | 81.1% | 93.8% | 6.2% | 12.7% | 0.0% | 12.7% | 0.0% | 0.0% | 0 | 11 | N/A | 0.0% |
| 0.15 | 0 | 18 | 12 | 81.1% | 93.8% | 6.2% | 12.7% | 0.0% | 12.7% | 0.0% | 0.0% | 0 | 13 | N/A | 0.0% |
| 0.15 | 0 | 20 | 8 | 92.3% | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 0.0% | 0 | 11 | N/A | 0.0% |
| 0.15 | 0 | 20 | 10 | 92.3% | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 0.0% | 0 | 13 | N/A | 0.0% |
| 0.15 | 0 | 20 | 12 | 92.3% | 93.8% | 6.2% | 1.5% | 0.0% | 1.5% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.15 | 0 | 22 | 8 | 93.7% | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 0.0% | 0 | 13 | N/A | 0.0% |
| 0.15 | 0 | 22 | 10 | 93.7% | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.15 | 0 | 22 | 12 | 93.7% | 93.8% | 6.2% | 0.1% | 0.0% | 0.1% | 0.0% | 0.0% | 0 | 17 | N/A | 0.0% |
| 0.15 | 0 | 24 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | N/A | 0.0% |
| 0.15 | 0 | 24 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | N/A | 0.0% |
| 0.15 | 0 | 24 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | N/A | 0.0% |
| 0.15 | 1 | 14 | 8 | 4.1% | 92.5% | 7.5% | 87.2% | 51.9% | 36.5% | 1.2% | 50.7% | 3 | 5 | 23 | 0.2% |
| 0.15 | 1 | 14 | 10 | 4.1% | 93.3% | 6.7% | 89.0% | 51.9% | 37.3% | 0.2% | 51.7% | 3 | 7 | 23 | -0.1% |
| 0.15 | 1 | 14 | 12 | 4.1% | 93.5% | 6.5% | 89.3% | 51.9% | 37.5% | 0.1% | 51.8% | 3 | 9 | 23 | 0.0% |
| 0.15 | 1 | 16 | 8 | 46.0% | 93.5% | 6.5% | 47.3% | 34.4% | 13.1% | 0.2% | 34.2% | 1 | 7 | 23 | 0.0% |
| 0.15 | 1 | 16 | 10 | 46.0% | 93.7% | 6.3% | 47.6% | 34.4% | 13.3% | 0.1% | 34.3% | 1 | 9 | 23 | 0.1% |
| 0.15 | 1 | 16 | 12 | 46.0% | 93.7% | 6.3% | 47.6% | 34.4% | 13.3% | 0.1% | 34.3% | 1 | 11 | 23 | 0.1% |
| 0.15 | 1 | 18 | 8 | 80.6% | 93.8% | 6.2% | 13.2% | 9.4% | 3.8% | 0.0% | 9.4% | 0 | 9 | 23 | 0.0% |
| 0.15 | 1 | 18 | 10 | 80.6% | 93.8% | 6.2% | 13.2% | 9.4% | 3.8% | 0.0% | 9.4% | 0 | 11 | 23 | 0.0% |
| 0.15 | 1 | 18 | 12 | 80.6% | 93.8% | 6.2% | 13.2% | 9.4% | 3.8% | 0.0% | 9.4% | 0 | 13 | 23 | 0.0% |
| 0.15 | 1 | 20 | 8 | 91.8% | 93.8% | 6.2% | 2.0% | 1.7% | 0.3% | 0.0% | 1.7% | 0 | 11 | 23 | 0.0% |
| 0.15 | 1 | 20 | 10 | 91.8% | 93.8% | 6.2% | 2.0% | 1.7% | 0.3% | 0.0% | 1.7% | 0 | 13 | 23 | 0.0% |
| 0.15 | 1 | 20 | 12 | 91.8% | 93.8% | 6.2% | 2.0% | 1.7% | 0.3% | 0.0% | 1.7% | 0 | 15 | 23 | 0.0% |
| 0.15 | 1 | 22 | 8 | 93.5% | 93.8% | 6.2% | 0.3% | 0.3% | 0.0% | 0.0% | 0.3% | 0 | 13 | 23 | 0.0% |
| 0.15 | 1 | 22 | 10 | 93.5% | 93.8% | 6.2% | 0.3% | 0.3% | 0.0% | 0.0% | 0.3% | 0 | 15 | 23 | 0.0% |
| 0.15 | 1 | 22 | 12 | 93.5% | 93.8% | 6.2% | 0.3% | 0.3% | 0.0% | 0.0% | 0.3% | 0 | 17 | 23 | 0.0% |
| 0.15 | 1 | 24 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 23 | 0.0% |
| 0.15 | 1 | 24 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 23 | 0.0% |
| 0.15 | 1 | 24 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 23 | 0.0% |
| 0.15 | 3 | 14 | 8 | 8.3% | 92.9% | 7.1% | 83.5% | 74.9% | 9.7% | 1.1% | 73.8% | 3 | 5 | 19 | 0.6% |
| 0.15 | 3 | 14 | 10 | 8.3% | 93.6% | 6.4% | 84.8% | 74.9% | 10.4% | 0.5% | 74.4% | 3 | 7 | 19 | 0.2% |
| 0.15 | 3 | 14 | 12 | 8.3% | 93.6% | 6.4% | 85.0% | 74.9% | 10.4% | 0.3% | 74.6% | 3 | 9 | 19 | 0.1% |
| 0.15 | 3 | 16 | 8 | 48.9% | 93.8% | 6.2% | 44.5% | 42.4% | 2.5% | 0.4% | 42.0% | 0 | 7 | 19 | 0.3% |
| 0.15 | 3 | 16 | 10 | 48.9% | 93.8% | 6.2% | 44.7% | 42.4% | 2.5% | 0.2% | 42.2% | 0 | 9 | 19 | 0.2% |
| 0.15 | 3 | 16 | 12 | 48.9% | 93.8% | 6.2% | 44.7% | 42.4% | 2.5% | 0.2% | 42.2% | 0 | 11 | 19 | 0.2% |
| 0.15 | 3 | 18 | 8 | 82.0% | 93.8% | 6.2% | 11.8% | 11.3% | 0.5% | 0.0% | 11.3% | 0 | 9 | 19 | 0.0% |
| 0.15 | 3 | 18 | 10 | 82.0% | 93.8% | 6.2% | 11.8% | 11.3% | 0.5% | 0.0% | 11.3% | 0 | 11 | 19 | 0.0% |
| 0.15 | 3 | 18 | 12 | 82.0% | 93.8% | 6.2% | 11.8% | 11.3% | 0.5% | 0.0% | 11.3% | 0 | 13 | 19 | 0.0% |
| 0.15 | 3 | 20 | 8 | 92.3% | 93.8% | 6.2% | 1.5% | 1.5% | 0.0% | 0.0% | 1.5% | 0 | 11 | 19 | 0.0% |
| 0.15 | 3 | 20 | 10 | 92.3% | 93.8% | 6.2% | 1.5% | 1.5% | 0.0% | 0.0% | 1.5% | 0 | 13 | 19 | 0.0% |
| 0.15 | 3 | 20 | 12 | 92.3% | 93.8% | 6.2% | 1.5% | 1.5% | 0.0% | 0.0% | 1.5% | 0 | 15 | 19 | 0.0% |
| 0.15 | 3 | 22 | 8 | 93.7% | 93.8% | 6.2% | 0.1% | 0.1% | 0.0% | 0.0% | 0.1% | 0 | 13 | 19 | 0.0% |
| 0.15 | 3 | 22 | 10 | 93.7% | 93.8% | 6.2% | 0.1% | 0.1% | 0.0% | 0.0% | 0.1% | 0 | 15 | 19 | 0.0% |
| 0.15 | 3 | 22 | 12 | 93.7% | 93.8% | 6.2% | 0.1% | 0.1% | 0.0% | 0.0% | 0.1% | 0 | 17 | 19 | 0.0% |
| 0.15 | 3 | 24 | 8 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 15 | 19 | 0.0% |
| 0.15 | 3 | 24 | 10 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 17 | 19 | 0.0% |
| 0.15 | 3 | 24 | 12 | 93.8% | 93.8% | 6.2% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0 | 19 | 19 | 0.0% |

## Detailed Metrics

### density=0.10 R=0 initial=14 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 94 (9.4%)
- via base feasible: 971 (97.1%)
- via resource feasible: 0 (0.0%)
- any feasible: 971 (97.1%)
- still infeasible: 29 (2.9%)
- rescued by base: 877 (87.7%)
- rescued by resource: 0 (0.0%)
- base only rescue: 877 (87.7%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 96.8%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 96.8%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 971
  - excluded_count: 29
  - min: 0
  - median: 5
  - p90: 7
  - max: 8

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 3
  - p90: 5
  - max: 10

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=14 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 94 (9.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 883 (88.3%)
- rescued by resource: 0 (0.0%)
- base only rescue: 883 (88.3%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 97.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 97.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 3
  - p90: 5
  - max: 10

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=14 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 94 (9.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 883 (88.3%)
- rescued by resource: 0 (0.0%)
- base only rescue: 883 (88.3%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 97.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 97.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 2
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 3
  - p90: 5
  - max: 10

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=16 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 615 (61.5%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 362 (36.2%)
- rescued by resource: 0 (0.0%)
- base only rescue: 362 (36.2%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 94.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 94.0%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 3
  - max: 8

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=16 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 615 (61.5%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 362 (36.2%)
- rescued by resource: 0 (0.0%)
- base only rescue: 362 (36.2%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 94.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 94.0%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 2
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 3
  - max: 8

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=16 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 615 (61.5%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 362 (36.2%)
- rescued by resource: 0 (0.0%)
- base only rescue: 362 (36.2%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 94.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 94.0%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 4
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 3
  - max: 8

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=18 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 922 (92.2%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 55 (5.5%)
- rescued by resource: 0 (0.0%)
- base only rescue: 55 (5.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 70.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 70.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 2
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 5

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=18 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 922 (92.2%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 55 (5.5%)
- rescued by resource: 0 (0.0%)
- base only rescue: 55 (5.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 70.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 70.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 4
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 5

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=18 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 922 (92.2%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 55 (5.5%)
- rescued by resource: 0 (0.0%)
- base only rescue: 55 (5.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 70.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 70.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 6
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 5

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=20 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 971 (97.1%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 6 (0.6%)
- rescued by resource: 0 (0.0%)
- base only rescue: 6 (0.6%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 20.7%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 20.7%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 4
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 2

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=20 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 971 (97.1%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 6 (0.6%)
- rescued by resource: 0 (0.0%)
- base only rescue: 6 (0.6%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 20.7%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 20.7%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 6
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 2

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=20 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 971 (97.1%)
- via base feasible: 977 (97.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 6 (0.6%)
- rescued by resource: 0 (0.0%)
- base only rescue: 6 (0.6%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 20.7%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 20.7%
- base only share among rescued: 100.0%
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
  - max: 2

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.10 R=0 initial=22 B=8 Rs=5

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
  - min: 6
  - median: 13
  - p90: 15
  - max: 16

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

### density=0.10 R=0 initial=22 B=10 Rs=5

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

### density=0.10 R=0 initial=22 B=12 Rs=5

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

### density=0.10 R=1 initial=14 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 80 (8.0%)
- via base feasible: 973 (97.3%)
- via resource feasible: 713 (71.3%)
- any feasible: 975 (97.5%)
- still infeasible: 25 (2.5%)
- rescued by base: 893 (89.3%)
- rescued by resource: 639 (63.9%)
- base only rescue: 256 (25.6%)
- resource only rescue: 2 (0.2%)
- both supply options: 637 (63.7%)
- base rescue among direct failures: 97.1%
- resource rescue among direct failures: 69.5%
- any rescue among direct failures: 97.3%
- base only share among rescued: 28.6%
- any feasible delta vs R=0: 0.4%
- still infeasible delta vs R=0: -0.4%

- remaining_fuel_at_goal
  - sample_count: 975
  - excluded_count: 25
  - min: 0
  - median: 5
  - p90: 7
  - max: 8

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 2
  - p90: 5
  - max: 10

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=14 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 80 (8.0%)
- via base feasible: 976 (97.6%)
- via resource feasible: 713 (71.3%)
- any feasible: 976 (97.6%)
- still infeasible: 24 (2.4%)
- rescued by base: 896 (89.6%)
- rescued by resource: 639 (63.9%)
- base only rescue: 257 (25.7%)
- resource only rescue: 0 (0.0%)
- both supply options: 639 (63.9%)
- base rescue among direct failures: 97.4%
- resource rescue among direct failures: 69.5%
- any rescue among direct failures: 97.4%
- base only share among rescued: 28.7%
- any feasible delta vs R=0: -0.1%
- still infeasible delta vs R=0: 0.1%

- remaining_fuel_at_goal
  - sample_count: 976
  - excluded_count: 24
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 2
  - p90: 5
  - max: 10

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=14 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 80 (8.0%)
- via base feasible: 977 (97.7%)
- via resource feasible: 713 (71.3%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 897 (89.7%)
- rescued by resource: 639 (63.9%)
- base only rescue: 258 (25.8%)
- resource only rescue: 0 (0.0%)
- both supply options: 639 (63.9%)
- base rescue among direct failures: 97.5%
- resource rescue among direct failures: 69.5%
- any rescue among direct failures: 97.5%
- base only share among rescued: 28.8%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 1
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 2
  - p90: 5
  - max: 10

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=16 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 587 (58.7%)
- via base feasible: 976 (97.6%)
- via resource feasible: 876 (87.6%)
- any feasible: 976 (97.6%)
- still infeasible: 24 (2.4%)
- rescued by base: 389 (38.9%)
- rescued by resource: 324 (32.4%)
- base only rescue: 65 (6.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 324 (32.4%)
- base rescue among direct failures: 94.2%
- resource rescue among direct failures: 78.5%
- any rescue among direct failures: 94.2%
- base only share among rescued: 16.7%
- any feasible delta vs R=0: -0.1%
- still infeasible delta vs R=0: 0.1%

- remaining_fuel_at_goal
  - sample_count: 976
  - excluded_count: 24
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 3
  - max: 8

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=16 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 587 (58.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 876 (87.6%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 390 (39.0%)
- rescued by resource: 324 (32.4%)
- base only rescue: 66 (6.6%)
- resource only rescue: 0 (0.0%)
- both supply options: 324 (32.4%)
- base rescue among direct failures: 94.4%
- resource rescue among direct failures: 78.5%
- any rescue among direct failures: 94.4%
- base only share among rescued: 16.9%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 1
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 3
  - max: 8

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=16 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 587 (58.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 876 (87.6%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 390 (39.0%)
- rescued by resource: 324 (32.4%)
- base only rescue: 66 (6.6%)
- resource only rescue: 0 (0.0%)
- both supply options: 324 (32.4%)
- base rescue among direct failures: 94.4%
- resource rescue among direct failures: 78.5%
- any rescue among direct failures: 94.4%
- base only share among rescued: 16.9%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 3
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 3
  - max: 8

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=18 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 917 (91.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 937 (93.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 60 (6.0%)
- rescued by resource: 49 (4.9%)
- base only rescue: 11 (1.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 49 (4.9%)
- base rescue among direct failures: 72.3%
- resource rescue among direct failures: 59.0%
- any rescue among direct failures: 72.3%
- base only share among rescued: 18.3%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 1
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 4

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=18 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 917 (91.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 937 (93.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 60 (6.0%)
- rescued by resource: 49 (4.9%)
- base only rescue: 11 (1.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 49 (4.9%)
- base rescue among direct failures: 72.3%
- resource rescue among direct failures: 59.0%
- any rescue among direct failures: 72.3%
- base only share among rescued: 18.3%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 3
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 4

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=18 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 917 (91.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 937 (93.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 60 (6.0%)
- rescued by resource: 49 (4.9%)
- base only rescue: 11 (1.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 49 (4.9%)
- base rescue among direct failures: 72.3%
- resource rescue among direct failures: 59.0%
- any rescue among direct failures: 72.3%
- base only share among rescued: 18.3%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 5
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 4

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=20 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 971 (97.1%)
- via base feasible: 977 (97.7%)
- via resource feasible: 965 (96.5%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 6 (0.6%)
- rescued by resource: 5 (0.5%)
- base only rescue: 1 (0.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 5 (0.5%)
- base rescue among direct failures: 20.7%
- resource rescue among direct failures: 17.2%
- any rescue among direct failures: 20.7%
- base only share among rescued: 16.7%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 3
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 2

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=20 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 971 (97.1%)
- via base feasible: 977 (97.7%)
- via resource feasible: 965 (96.5%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 6 (0.6%)
- rescued by resource: 5 (0.5%)
- base only rescue: 1 (0.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 5 (0.5%)
- base rescue among direct failures: 20.7%
- resource rescue among direct failures: 17.2%
- any rescue among direct failures: 20.7%
- base only share among rescued: 16.7%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 5
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 2

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=20 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 971 (97.1%)
- via base feasible: 977 (97.7%)
- via resource feasible: 965 (96.5%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 6 (0.6%)
- rescued by resource: 5 (0.5%)
- base only rescue: 1 (0.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 5 (0.5%)
- base rescue among direct failures: 20.7%
- resource rescue among direct failures: 17.2%
- any rescue among direct failures: 20.7%
- base only share among rescued: 16.7%
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
  - max: 2

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 18
  - p90: 21
  - max: 31

### density=0.10 R=1 initial=22 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 974 (97.4%)
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
  - min: 5
  - median: 13
  - p90: 15
  - max: 16

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

### density=0.10 R=1 initial=22 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 974 (97.4%)
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

### density=0.10 R=1 initial=22 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 977 (97.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 974 (97.4%)
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

### density=0.10 R=3 initial=14 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 130 (13.0%)
- via base feasible: 973 (97.3%)
- via resource feasible: 928 (92.8%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 843 (84.3%)
- rescued by resource: 798 (79.8%)
- base only rescue: 49 (4.9%)
- resource only rescue: 4 (0.4%)
- both supply options: 794 (79.4%)
- base rescue among direct failures: 96.9%
- resource rescue among direct failures: 91.7%
- any rescue among direct failures: 97.4%
- base only share among rescued: 5.8%
- any feasible delta vs R=0: 0.6%
- still infeasible delta vs R=0: -0.6%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 6
  - p90: 7
  - max: 8

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 2
  - p90: 4
  - max: 7

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=14 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 130 (13.0%)
- via base feasible: 976 (97.6%)
- via resource feasible: 928 (92.8%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 846 (84.6%)
- rescued by resource: 798 (79.8%)
- base only rescue: 49 (4.9%)
- resource only rescue: 1 (0.1%)
- both supply options: 797 (79.7%)
- base rescue among direct failures: 97.2%
- resource rescue among direct failures: 91.7%
- any rescue among direct failures: 97.4%
- base only share among rescued: 5.8%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 8
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 2
  - p90: 4
  - max: 7

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=14 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 130 (13.0%)
- via base feasible: 976 (97.6%)
- via resource feasible: 928 (92.8%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 846 (84.6%)
- rescued by resource: 798 (79.8%)
- base only rescue: 49 (4.9%)
- resource only rescue: 1 (0.1%)
- both supply options: 797 (79.7%)
- base rescue among direct failures: 97.2%
- resource rescue among direct failures: 91.7%
- any rescue among direct failures: 97.4%
- base only share among rescued: 5.8%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 10
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 2
  - p90: 4
  - max: 7

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=16 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 634 (63.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 975 (97.5%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 343 (34.3%)
- rescued by resource: 341 (34.1%)
- base only rescue: 2 (0.2%)
- resource only rescue: 0 (0.0%)
- both supply options: 341 (34.1%)
- base rescue among direct failures: 93.7%
- resource rescue among direct failures: 93.2%
- any rescue among direct failures: 93.7%
- base only share among rescued: 0.6%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 2
  - median: 8
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 2
  - max: 5

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=16 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 634 (63.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 975 (97.5%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 343 (34.3%)
- rescued by resource: 341 (34.1%)
- base only rescue: 2 (0.2%)
- resource only rescue: 0 (0.0%)
- both supply options: 341 (34.1%)
- base rescue among direct failures: 93.7%
- resource rescue among direct failures: 93.2%
- any rescue among direct failures: 93.7%
- base only share among rescued: 0.6%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 3
  - median: 10
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 2
  - max: 5

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=16 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 634 (63.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 975 (97.5%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 343 (34.3%)
- rescued by resource: 341 (34.1%)
- base only rescue: 2 (0.2%)
- resource only rescue: 0 (0.0%)
- both supply options: 341 (34.1%)
- base rescue among direct failures: 93.7%
- resource rescue among direct failures: 93.2%
- any rescue among direct failures: 93.7%
- base only share among rescued: 0.6%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 4
  - median: 12
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 2
  - max: 5

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=18 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 927 (92.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 50 (5.0%)
- rescued by resource: 50 (5.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 50 (5.0%)
- base rescue among direct failures: 68.5%
- resource rescue among direct failures: 68.5%
- any rescue among direct failures: 68.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 4
  - median: 10
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=18 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 927 (92.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 50 (5.0%)
- rescued by resource: 50 (5.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 50 (5.0%)
- base rescue among direct failures: 68.5%
- resource rescue among direct failures: 68.5%
- any rescue among direct failures: 68.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 5
  - median: 12
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=18 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 927 (92.7%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 50 (5.0%)
- rescued by resource: 50 (5.0%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 50 (5.0%)
- base rescue among direct failures: 68.5%
- resource rescue among direct failures: 68.5%
- any rescue among direct failures: 68.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 6
  - median: 14
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=20 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 974 (97.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 3 (0.3%)
- rescued by resource: 3 (0.3%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 3 (0.3%)
- base rescue among direct failures: 11.5%
- resource rescue among direct failures: 11.5%
- any rescue among direct failures: 11.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 6
  - median: 12
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 1

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=20 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 974 (97.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 3 (0.3%)
- rescued by resource: 3 (0.3%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 3 (0.3%)
- base rescue among direct failures: 11.5%
- resource rescue among direct failures: 11.5%
- any rescue among direct failures: 11.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 7
  - median: 14
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 1

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=20 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 974 (97.4%)
- via base feasible: 977 (97.7%)
- via resource feasible: 977 (97.7%)
- any feasible: 977 (97.7%)
- still infeasible: 23 (2.3%)
- rescued by base: 3 (0.3%)
- rescued by resource: 3 (0.3%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 3 (0.3%)
- base rescue among direct failures: 11.5%
- resource rescue among direct failures: 11.5%
- any rescue among direct failures: 11.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 977
  - excluded_count: 23
  - min: 8
  - median: 16
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 977
  - excluded_count: 23
  - min: 0
  - median: 0
  - p90: 0
  - max: 1

- best_cost_via_resource
  - sample_count: 977
  - excluded_count: 23
  - min: 14
  - median: 16
  - p90: 18
  - max: 22

### density=0.10 R=3 initial=22 B=8 Rs=5

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
  - min: 8
  - median: 14
  - p90: 15
  - max: 16

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

### density=0.10 R=3 initial=22 B=10 Rs=5

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
  - min: 9
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

### density=0.10 R=3 initial=22 B=12 Rs=5

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

### density=0.15 R=0 initial=14 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 56 (5.6%)
- via base feasible: 923 (92.3%)
- via resource feasible: 0 (0.0%)
- any feasible: 923 (92.3%)
- still infeasible: 77 (7.7%)
- rescued by base: 867 (86.7%)
- rescued by resource: 0 (0.0%)
- base only rescue: 867 (86.7%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 91.8%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 91.8%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 923
  - excluded_count: 77
  - min: 0
  - median: 5
  - p90: 7
  - max: 8

- required_supply
  - sample_count: 935
  - excluded_count: 65
  - min: 0
  - median: 3
  - p90: 6
  - max: 11

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=14 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 56 (5.6%)
- via base feasible: 934 (93.4%)
- via resource feasible: 0 (0.0%)
- any feasible: 934 (93.4%)
- still infeasible: 66 (6.6%)
- rescued by base: 878 (87.8%)
- rescued by resource: 0 (0.0%)
- base only rescue: 878 (87.8%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 93.0%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 93.0%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 934
  - excluded_count: 66
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 935
  - excluded_count: 65
  - min: 0
  - median: 3
  - p90: 6
  - max: 11

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=14 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 56 (5.6%)
- via base feasible: 935 (93.5%)
- via resource feasible: 0 (0.0%)
- any feasible: 935 (93.5%)
- still infeasible: 65 (6.5%)
- rescued by base: 879 (87.9%)
- rescued by resource: 0 (0.0%)
- base only rescue: 879 (87.9%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 93.1%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 93.1%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 935
  - excluded_count: 65
  - min: 1
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 935
  - excluded_count: 65
  - min: 0
  - median: 3
  - p90: 6
  - max: 11

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=16 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 447 (44.7%)
- via base feasible: 935 (93.5%)
- via resource feasible: 0 (0.0%)
- any feasible: 935 (93.5%)
- still infeasible: 65 (6.5%)
- rescued by base: 488 (48.8%)
- rescued by resource: 0 (0.0%)
- base only rescue: 488 (48.8%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 88.2%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 88.2%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 935
  - excluded_count: 65
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 936
  - excluded_count: 64
  - min: 0
  - median: 1
  - p90: 4
  - max: 9

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=16 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 447 (44.7%)
- via base feasible: 936 (93.6%)
- via resource feasible: 0 (0.0%)
- any feasible: 936 (93.6%)
- still infeasible: 64 (6.4%)
- rescued by base: 489 (48.9%)
- rescued by resource: 0 (0.0%)
- base only rescue: 489 (48.9%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 88.4%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 88.4%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 936
  - excluded_count: 64
  - min: 1
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 936
  - excluded_count: 64
  - min: 0
  - median: 1
  - p90: 4
  - max: 9

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=16 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 447 (44.7%)
- via base feasible: 936 (93.6%)
- via resource feasible: 0 (0.0%)
- any feasible: 936 (93.6%)
- still infeasible: 64 (6.4%)
- rescued by base: 489 (48.9%)
- rescued by resource: 0 (0.0%)
- base only rescue: 489 (48.9%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 88.4%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 88.4%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 936
  - excluded_count: 64
  - min: 3
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 936
  - excluded_count: 64
  - min: 0
  - median: 1
  - p90: 4
  - max: 9

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=18 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 811 (81.1%)
- via base feasible: 937 (93.7%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 127 (12.7%)
- rescued by resource: 0 (0.0%)
- base only rescue: 127 (12.7%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 67.2%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 67.2%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 7

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=18 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 811 (81.1%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 127 (12.7%)
- rescued by resource: 0 (0.0%)
- base only rescue: 127 (12.7%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 67.2%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 67.2%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 1
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 7

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=18 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 811 (81.1%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 127 (12.7%)
- rescued by resource: 0 (0.0%)
- base only rescue: 127 (12.7%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 67.2%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 67.2%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 3
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 7

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=20 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 923 (92.3%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 15 (1.5%)
- rescued by resource: 0 (0.0%)
- base only rescue: 15 (1.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 19.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 19.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 2
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=20 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 923 (92.3%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 15 (1.5%)
- rescued by resource: 0 (0.0%)
- base only rescue: 15 (1.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 19.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 19.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 3
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=20 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 923 (92.3%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 15 (1.5%)
- rescued by resource: 0 (0.0%)
- base only rescue: 15 (1.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 19.5%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 19.5%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 5
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=22 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 937 (93.7%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 1 (0.1%)
- rescued by resource: 0 (0.0%)
- base only rescue: 1 (0.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 1.6%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 1.6%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 1

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=22 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 937 (93.7%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 1 (0.1%)
- rescued by resource: 0 (0.0%)
- base only rescue: 1 (0.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 1.6%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 1.6%
- base only share among rescued: 100.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 5
  - median: 15
  - p90: 17
  - max: 18

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 1

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

### density=0.15 R=0 initial=22 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 937 (93.7%)
- via base feasible: 938 (93.8%)
- via resource feasible: 0 (0.0%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 1 (0.1%)
- rescued by resource: 0 (0.0%)
- base only rescue: 1 (0.1%)
- resource only rescue: 0 (0.0%)
- both supply options: 0 (0.0%)
- base rescue among direct failures: 1.6%
- resource rescue among direct failures: 0.0%
- any rescue among direct failures: 1.6%
- base only share among rescued: 100.0%
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
  - max: 1

- best_cost_via_resource
  - sample_count: 0
  - excluded_count: 1000
  - min: N/A
  - median: N/A
  - p90: N/A
  - max: N/A

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

### density=0.15 R=1 initial=14 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 41 (4.1%)
- via base feasible: 913 (91.3%)
- via resource feasible: 555 (55.5%)
- any feasible: 925 (92.5%)
- still infeasible: 75 (7.5%)
- rescued by base: 872 (87.2%)
- rescued by resource: 519 (51.9%)
- base only rescue: 365 (36.5%)
- resource only rescue: 12 (1.2%)
- both supply options: 507 (50.7%)
- base rescue among direct failures: 90.9%
- resource rescue among direct failures: 54.1%
- any rescue among direct failures: 92.2%
- base only share among rescued: 41.3%
- any feasible delta vs R=0: 0.2%
- still infeasible delta vs R=0: -0.2%

- remaining_fuel_at_goal
  - sample_count: 925
  - excluded_count: 75
  - min: 0
  - median: 5
  - p90: 7
  - max: 8

- required_supply
  - sample_count: 937
  - excluded_count: 63
  - min: 0
  - median: 3
  - p90: 6
  - max: 11

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=14 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 41 (4.1%)
- via base feasible: 931 (93.1%)
- via resource feasible: 555 (55.5%)
- any feasible: 933 (93.3%)
- still infeasible: 67 (6.7%)
- rescued by base: 890 (89.0%)
- rescued by resource: 519 (51.9%)
- base only rescue: 373 (37.3%)
- resource only rescue: 2 (0.2%)
- both supply options: 517 (51.7%)
- base rescue among direct failures: 92.8%
- resource rescue among direct failures: 54.1%
- any rescue among direct failures: 93.0%
- base only share among rescued: 41.8%
- any feasible delta vs R=0: -0.1%
- still infeasible delta vs R=0: 0.1%

- remaining_fuel_at_goal
  - sample_count: 933
  - excluded_count: 67
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 937
  - excluded_count: 63
  - min: 0
  - median: 3
  - p90: 6
  - max: 11

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=14 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 41 (4.1%)
- via base feasible: 934 (93.4%)
- via resource feasible: 555 (55.5%)
- any feasible: 935 (93.5%)
- still infeasible: 65 (6.5%)
- rescued by base: 893 (89.3%)
- rescued by resource: 519 (51.9%)
- base only rescue: 375 (37.5%)
- resource only rescue: 1 (0.1%)
- both supply options: 518 (51.8%)
- base rescue among direct failures: 93.1%
- resource rescue among direct failures: 54.1%
- any rescue among direct failures: 93.2%
- base only share among rescued: 41.9%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 935
  - excluded_count: 65
  - min: 0
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 937
  - excluded_count: 63
  - min: 0
  - median: 3
  - p90: 6
  - max: 11

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=16 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 460 (46.0%)
- via base feasible: 933 (93.3%)
- via resource feasible: 750 (75.0%)
- any feasible: 935 (93.5%)
- still infeasible: 65 (6.5%)
- rescued by base: 473 (47.3%)
- rescued by resource: 344 (34.4%)
- base only rescue: 131 (13.1%)
- resource only rescue: 2 (0.2%)
- both supply options: 342 (34.2%)
- base rescue among direct failures: 87.6%
- resource rescue among direct failures: 63.7%
- any rescue among direct failures: 88.0%
- base only share among rescued: 27.6%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 935
  - excluded_count: 65
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 1
  - p90: 4
  - max: 8

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=16 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 460 (46.0%)
- via base feasible: 936 (93.6%)
- via resource feasible: 750 (75.0%)
- any feasible: 937 (93.7%)
- still infeasible: 63 (6.3%)
- rescued by base: 476 (47.6%)
- rescued by resource: 344 (34.4%)
- base only rescue: 133 (13.3%)
- resource only rescue: 1 (0.1%)
- both supply options: 343 (34.3%)
- base rescue among direct failures: 88.1%
- resource rescue among direct failures: 63.7%
- any rescue among direct failures: 88.3%
- base only share among rescued: 27.9%
- any feasible delta vs R=0: 0.1%
- still infeasible delta vs R=0: -0.1%

- remaining_fuel_at_goal
  - sample_count: 937
  - excluded_count: 63
  - min: 0
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 1
  - p90: 4
  - max: 8

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=16 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 460 (46.0%)
- via base feasible: 936 (93.6%)
- via resource feasible: 750 (75.0%)
- any feasible: 937 (93.7%)
- still infeasible: 63 (6.3%)
- rescued by base: 476 (47.6%)
- rescued by resource: 344 (34.4%)
- base only rescue: 133 (13.3%)
- resource only rescue: 1 (0.1%)
- both supply options: 343 (34.3%)
- base rescue among direct failures: 88.1%
- resource rescue among direct failures: 63.7%
- any rescue among direct failures: 88.3%
- base only share among rescued: 27.9%
- any feasible delta vs R=0: 0.1%
- still infeasible delta vs R=0: -0.1%

- remaining_fuel_at_goal
  - sample_count: 937
  - excluded_count: 63
  - min: 0
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 1
  - p90: 4
  - max: 8

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=18 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 806 (80.6%)
- via base feasible: 938 (93.8%)
- via resource feasible: 852 (85.2%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 132 (13.2%)
- rescued by resource: 94 (9.4%)
- base only rescue: 38 (3.8%)
- resource only rescue: 0 (0.0%)
- both supply options: 94 (9.4%)
- base rescue among direct failures: 68.0%
- resource rescue among direct failures: 48.5%
- any rescue among direct failures: 68.0%
- base only share among rescued: 28.8%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 6

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=18 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 806 (80.6%)
- via base feasible: 938 (93.8%)
- via resource feasible: 852 (85.2%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 132 (13.2%)
- rescued by resource: 94 (9.4%)
- base only rescue: 38 (3.8%)
- resource only rescue: 0 (0.0%)
- both supply options: 94 (9.4%)
- base rescue among direct failures: 68.0%
- resource rescue among direct failures: 48.5%
- any rescue among direct failures: 68.0%
- base only share among rescued: 28.8%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 2
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 6

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=18 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 806 (80.6%)
- via base feasible: 938 (93.8%)
- via resource feasible: 852 (85.2%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 132 (13.2%)
- rescued by resource: 94 (9.4%)
- base only rescue: 38 (3.8%)
- resource only rescue: 0 (0.0%)
- both supply options: 94 (9.4%)
- base rescue among direct failures: 68.0%
- resource rescue among direct failures: 48.5%
- any rescue among direct failures: 68.0%
- base only share among rescued: 28.8%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 6

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=20 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 918 (91.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 891 (89.1%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 20 (2.0%)
- rescued by resource: 17 (1.7%)
- base only rescue: 3 (0.3%)
- resource only rescue: 0 (0.0%)
- both supply options: 17 (1.7%)
- base rescue among direct failures: 24.4%
- resource rescue among direct failures: 20.7%
- any rescue among direct failures: 24.4%
- base only share among rescued: 15.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 2
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 4

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=20 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 918 (91.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 891 (89.1%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 20 (2.0%)
- rescued by resource: 17 (1.7%)
- base only rescue: 3 (0.3%)
- resource only rescue: 0 (0.0%)
- both supply options: 17 (1.7%)
- base rescue among direct failures: 24.4%
- resource rescue among direct failures: 20.7%
- any rescue among direct failures: 24.4%
- base only share among rescued: 15.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 4

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=20 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 918 (91.8%)
- via base feasible: 938 (93.8%)
- via resource feasible: 891 (89.1%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 20 (2.0%)
- rescued by resource: 17 (1.7%)
- base only rescue: 3 (0.3%)
- resource only rescue: 0 (0.0%)
- both supply options: 17 (1.7%)
- base rescue among direct failures: 24.4%
- resource rescue among direct failures: 20.7%
- any rescue among direct failures: 24.4%
- base only share among rescued: 15.0%
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
  - max: 4

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=22 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 935 (93.5%)
- via base feasible: 938 (93.8%)
- via resource feasible: 923 (92.3%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 3 (0.3%)
- rescued by resource: 3 (0.3%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 3 (0.3%)
- base rescue among direct failures: 4.6%
- resource rescue among direct failures: 4.6%
- any rescue among direct failures: 4.6%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 2

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=22 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 935 (93.5%)
- via base feasible: 938 (93.8%)
- via resource feasible: 923 (92.3%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 3 (0.3%)
- rescued by resource: 3 (0.3%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 3 (0.3%)
- base rescue among direct failures: 4.6%
- resource rescue among direct failures: 4.6%
- any rescue among direct failures: 4.6%
- base only share among rescued: 0.0%
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
  - max: 2

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

### density=0.15 R=1 initial=22 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 935 (93.5%)
- via base feasible: 938 (93.8%)
- via resource feasible: 923 (92.3%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 3 (0.3%)
- rescued by resource: 3 (0.3%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 3 (0.3%)
- base rescue among direct failures: 4.6%
- resource rescue among direct failures: 4.6%
- any rescue among direct failures: 4.6%
- base only share among rescued: 0.0%
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
  - max: 2

- best_cost_via_resource
  - sample_count: 937
  - excluded_count: 63
  - min: 14
  - median: 18
  - p90: 23
  - max: 32

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

### density=0.15 R=3 initial=14 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 83 (8.3%)
- via base feasible: 918 (91.8%)
- via resource feasible: 832 (83.2%)
- any feasible: 929 (92.9%)
- still infeasible: 71 (7.1%)
- rescued by base: 835 (83.5%)
- rescued by resource: 749 (74.9%)
- base only rescue: 97 (9.7%)
- resource only rescue: 11 (1.1%)
- both supply options: 738 (73.8%)
- base rescue among direct failures: 91.1%
- resource rescue among direct failures: 81.7%
- any rescue among direct failures: 92.3%
- base only share among rescued: 11.5%
- any feasible delta vs R=0: 0.6%
- still infeasible delta vs R=0: -0.6%

- remaining_fuel_at_goal
  - sample_count: 929
  - excluded_count: 71
  - min: 0
  - median: 5
  - p90: 7
  - max: 8

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 3
  - p90: 5
  - max: 9

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=14 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 83 (8.3%)
- via base feasible: 931 (93.1%)
- via resource feasible: 832 (83.2%)
- any feasible: 936 (93.6%)
- still infeasible: 64 (6.4%)
- rescued by base: 848 (84.8%)
- rescued by resource: 749 (74.9%)
- base only rescue: 104 (10.4%)
- resource only rescue: 5 (0.5%)
- both supply options: 744 (74.4%)
- base rescue among direct failures: 92.5%
- resource rescue among direct failures: 81.7%
- any rescue among direct failures: 93.0%
- base only share among rescued: 12.2%
- any feasible delta vs R=0: 0.2%
- still infeasible delta vs R=0: -0.2%

- remaining_fuel_at_goal
  - sample_count: 936
  - excluded_count: 64
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 3
  - p90: 5
  - max: 9

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=14 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 83 (8.3%)
- via base feasible: 933 (93.3%)
- via resource feasible: 832 (83.2%)
- any feasible: 936 (93.6%)
- still infeasible: 64 (6.4%)
- rescued by base: 850 (85.0%)
- rescued by resource: 749 (74.9%)
- base only rescue: 104 (10.4%)
- resource only rescue: 3 (0.3%)
- both supply options: 746 (74.6%)
- base rescue among direct failures: 92.7%
- resource rescue among direct failures: 81.7%
- any rescue among direct failures: 93.0%
- base only share among rescued: 12.2%
- any feasible delta vs R=0: 0.1%
- still infeasible delta vs R=0: -0.1%

- remaining_fuel_at_goal
  - sample_count: 936
  - excluded_count: 64
  - min: 0
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 3
  - p90: 5
  - max: 9

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=16 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 489 (48.9%)
- via base feasible: 934 (93.4%)
- via resource feasible: 913 (91.3%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 445 (44.5%)
- rescued by resource: 424 (42.4%)
- base only rescue: 25 (2.5%)
- resource only rescue: 4 (0.4%)
- both supply options: 420 (42.0%)
- base rescue among direct failures: 87.1%
- resource rescue among direct failures: 83.0%
- any rescue among direct failures: 87.9%
- base only share among rescued: 5.6%
- any feasible delta vs R=0: 0.3%
- still infeasible delta vs R=0: -0.3%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 7
  - p90: 9
  - max: 10

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 3
  - max: 7

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=16 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 489 (48.9%)
- via base feasible: 936 (93.6%)
- via resource feasible: 913 (91.3%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 447 (44.7%)
- rescued by resource: 424 (42.4%)
- base only rescue: 25 (2.5%)
- resource only rescue: 2 (0.2%)
- both supply options: 422 (42.2%)
- base rescue among direct failures: 87.5%
- resource rescue among direct failures: 83.0%
- any rescue among direct failures: 87.9%
- base only share among rescued: 5.6%
- any feasible delta vs R=0: 0.2%
- still infeasible delta vs R=0: -0.2%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 3
  - max: 7

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=16 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 489 (48.9%)
- via base feasible: 936 (93.6%)
- via resource feasible: 913 (91.3%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 447 (44.7%)
- rescued by resource: 424 (42.4%)
- base only rescue: 25 (2.5%)
- resource only rescue: 2 (0.2%)
- both supply options: 422 (42.2%)
- base rescue among direct failures: 87.5%
- resource rescue among direct failures: 83.0%
- any rescue among direct failures: 87.9%
- base only share among rescued: 5.6%
- any feasible delta vs R=0: 0.2%
- still infeasible delta vs R=0: -0.2%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 3
  - max: 7

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=18 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 820 (82.0%)
- via base feasible: 938 (93.8%)
- via resource feasible: 932 (93.2%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 118 (11.8%)
- rescued by resource: 113 (11.3%)
- base only rescue: 5 (0.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 113 (11.3%)
- base rescue among direct failures: 65.6%
- resource rescue among direct failures: 62.8%
- any rescue among direct failures: 65.6%
- base only share among rescued: 4.2%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 2
  - median: 9
  - p90: 11
  - max: 12

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 5

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=18 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 820 (82.0%)
- via base feasible: 938 (93.8%)
- via resource feasible: 932 (93.2%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 118 (11.8%)
- rescued by resource: 113 (11.3%)
- base only rescue: 5 (0.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 113 (11.3%)
- base rescue among direct failures: 65.6%
- resource rescue among direct failures: 62.8%
- any rescue among direct failures: 65.6%
- base only share among rescued: 4.2%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 2
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 5

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=18 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 820 (82.0%)
- via base feasible: 938 (93.8%)
- via resource feasible: 932 (93.2%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 118 (11.8%)
- rescued by resource: 113 (11.3%)
- base only rescue: 5 (0.5%)
- resource only rescue: 0 (0.0%)
- both supply options: 113 (11.3%)
- base rescue among direct failures: 65.6%
- resource rescue among direct failures: 62.8%
- any rescue among direct failures: 65.6%
- base only share among rescued: 4.2%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 1
  - max: 5

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=20 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 923 (92.3%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 15 (1.5%)
- rescued by resource: 15 (1.5%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 15 (1.5%)
- base rescue among direct failures: 19.5%
- resource rescue among direct failures: 19.5%
- any rescue among direct failures: 19.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 11
  - p90: 13
  - max: 14

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=20 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 923 (92.3%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 15 (1.5%)
- rescued by resource: 15 (1.5%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 15 (1.5%)
- base rescue among direct failures: 19.5%
- resource rescue among direct failures: 19.5%
- any rescue among direct failures: 19.5%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 4
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 3

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=20 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 923 (92.3%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 15 (1.5%)
- rescued by resource: 15 (1.5%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 15 (1.5%)
- base rescue among direct failures: 19.5%
- resource rescue among direct failures: 19.5%
- any rescue among direct failures: 19.5%
- base only share among rescued: 0.0%
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
  - max: 3

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=22 B=8 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 937 (93.7%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 1 (0.1%)
- rescued by resource: 1 (0.1%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 1 (0.1%)
- base rescue among direct failures: 1.6%
- resource rescue among direct failures: 1.6%
- any rescue among direct failures: 1.6%
- base only share among rescued: 0.0%
- any feasible delta vs R=0: 0.0%
- still infeasible delta vs R=0: 0.0%

- remaining_fuel_at_goal
  - sample_count: 938
  - excluded_count: 62
  - min: 6
  - median: 13
  - p90: 15
  - max: 16

- required_supply
  - sample_count: 938
  - excluded_count: 62
  - min: 0
  - median: 0
  - p90: 0
  - max: 1

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=22 B=10 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 937 (93.7%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 1 (0.1%)
- rescued by resource: 1 (0.1%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 1 (0.1%)
- base rescue among direct failures: 1.6%
- resource rescue among direct failures: 1.6%
- any rescue among direct failures: 1.6%
- base only share among rescued: 0.0%
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
  - max: 1

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

### density=0.15 R=3 initial=22 B=12 Rs=5

- seed range: 1..1000 (1000 seeds)
- direct feasible: 937 (93.7%)
- via base feasible: 938 (93.8%)
- via resource feasible: 938 (93.8%)
- any feasible: 938 (93.8%)
- still infeasible: 62 (6.2%)
- rescued by base: 1 (0.1%)
- rescued by resource: 1 (0.1%)
- base only rescue: 0 (0.0%)
- resource only rescue: 0 (0.0%)
- both supply options: 1 (0.1%)
- base rescue among direct failures: 1.6%
- resource rescue among direct failures: 1.6%
- any rescue among direct failures: 1.6%
- base only share among rescued: 0.0%
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
  - max: 1

- best_cost_via_resource
  - sample_count: 938
  - excluded_count: 62
  - min: 14
  - median: 17
  - p90: 19
  - max: 25

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
