# Galactic Exodus Phase 0 実験環境

このディレクトリには、`Galactic Exodus` の Phase 0 における数理・ゲームデザイン検証用の小規模な Python 実験環境が含まれています。

これは正式な TBX アプリ実装ではありません。将来の TBX 側の実装は、`examples/galactic_exodus/` など別の場所に置く想定です。

## 実行方法

```bash
python experiments/galactic_exodus/simulate.py --seed 42 --rift-density 0.10
```

複数 seed の Phase 0 統計をまとめて出す場合は、次を実行します。

```bash
python experiments/galactic_exodus/metrics.py --seed-start 1 --seed-count 1000 --rift-density 0.10 --resource-count 3
```

## 地形コスト

基準となる経路分析では、8x8 グリッド上を4方向（`N/E/S/W`）に移動します。
各移動では、移動先マスのコストを加算します。開始地点のコストは加算しません。

| 記号 | 意味 | コスト |
| --- | --- | --- |
| `.` | 安定宙域 | 1 |
| `N` | 星雲 | 2 |
| `A` | 小惑星帯 | 3 |
| `@` | 重力井戸 / 重力異常 | 2 |
| `B` | 辺境基地 | 1 |
| `R` | 資源天体 | 1 |
| `S` | 出発星系 | 0 |
| `H` | 故郷星系 | 1 |

この実験では、マップ全体の情報が既知であることを前提に、移動辺へ任意の断層制約を加えた基準計算を行います。

## 断層航路

グリッドには、無向の隣接辺が合計112本あります。断層辺は seed に基づいて決定的に選ばれます。

```text
rift_count = round(112 * rift_density)
```

密度は `--rift-density FLOAT` で指定します。既定値は `0.10` です。

選択された断層辺は双方向に通行不能となり、次のすべての最短路計算から除外されます。

- `S -> H`
- `S -> B`
- `B -> H`
- `B` を通行禁止にした `S -> H`

## レポート出力

`simulate.py` は、次のセクションをこの順序で出力します。

- `MAP ID`
- `OBJECTS`
- `PARAMETERS`
- `MAP`
- `COSTS`
- `VERDICT`

`COSTS` セクションには、verdict 分類に使う次の最短路指標が含まれます。

- `S_to_H_cost`
- `S_to_H_steps`
- `S_to_B_cost`
- `B_to_H_cost`
- `S_to_H_via_B_cost`
- `S_to_H_without_B_cost`
- `base_route_advantage_raw`
- `base_is_mandatory`（`yes` / `no`）

利用できない指標は `N/A` と表示されます。

## verdict の判定規則

verdict の優先順位は次のとおりです。

1. `REJECT_TOO_HARD`
2. `REJECT_BASE_MANDATORY`
3. `ACCEPT`

分類規則:

- `REJECT_TOO_HARD`: `S -> H`、`S -> B`、`B -> H` のいずれかが到達不能
- `REJECT_BASE_MANDATORY`: 必要な各区間には到達できるが、`B` を回避する `S -> H` 経路が存在しない
- `ACCEPT`: 上位の棄却条件に該当しないマップ

`ACCEPT` は最低限の候補判定にすぎません。そのマップがすでに面白い、バランスが取れている、最終品質に達していることを意味しません。

## Batch Metrics

`metrics.py` は、`rift_density` と `resource_count` を固定したまま、連続した seed 範囲をまとめて評価します。
各マップと断層辺の選択は数値 seed から決定的に導出されるため、同じ入力なら出力は再現可能です。

必須入力:

- `--seed-start`: 連続範囲の開始 seed
- `--seed-count`: 実行する seed 数
- `--rift-density`: バッチ全体で共有する断層密度
- `--resource-count`: バッチ全体で共有する資源天体数

テキストレポートには次が含まれます。

- verdict 件数・割合
- `S_to_H_cost` min / median / p90 / max
- `S_to_H_steps` min / median / p90 / max
- `base_is_mandatory` 件数・割合
- `base_route_advantage_raw` の negative / zero / positive / unavailable 件数・割合

`S_to_H_cost` と `S_to_H_steps` は、到達不能ケースを分布統計から除外し、その件数を `excluded_unreachable` として併記します。

`base_route_advantage_raw` の意味:

- negative: `B` 経由の最良経路が `B` 回避の最良経路より悪い
- zero: `B` 経由と `B` 回避の総コストが同じ
- positive: `B` 経由のほうが `B` 回避より安い
- unavailable: `B` 経由または `B` 回避のどちらかの経路が存在しない

`p90` は、到達可能サンプルを昇順に並べたうえで nearest-rank 方式で求めます。

## テスト

Python 実験環境のテストは、標準ライブラリの `unittest` で実行します。

```bash
python -m unittest experiments.galactic_exodus.test_simulate experiments.galactic_exodus.test_metrics
```
