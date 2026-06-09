# Galactic Exodus Phase 0 実験環境

このディレクトリには、`Galactic Exodus` の Phase 0 における数理・ゲームデザイン検証用の小規模な Python 実験環境が含まれています。

これは正式な TBX アプリ実装ではありません。将来の TBX 側の実装は、`examples/galactic_exodus/` など別の場所に置く想定です。

## 実行方法

```bash
python experiments/galactic_exodus/simulate.py --seed 42 --rift-density 0.10 --initial-fuel 27 --base-supply 10 --resource-supply 5
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
- `FUEL PARAMETERS`
- `FUEL ANALYSIS`
- `MAP`
- `COSTS`
- `COST CONTRIBUTIONS`
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

## 航行力モデル

`simulate.py` には、1 seed レポートと後続比較用に再利用する純粋関数 `analyze_fuel()` があります。

入力パラメータ:

- `initial_fuel`
- `base_supply`
- `resource_supply`

暫定 CLI 既定値:

- `initial_fuel = 27`
- `base_supply = 10`
- `resource_supply = 5`

いずれも 0 以上の整数で、負数は `ValueError` です。

この分析では、既存の地形コストと断層制約をそのまま使います。

- `S` では消費しない
- 隣接マスへ移動すると、移動先マスの地形コストを消費する
- 断層辺は通行不能
- 燃料が負になる移動はできない
- `B` / `R` / `H` にちょうど 0 で到着してよい
- 補給は `B` または 1 つの `R` で最大 1 回だけ行う
- 補給量は到着後に加算する
- 燃料容量上限は設けない

評価対象の走行計画は次の 3 種類だけです。

- `direct`: 補給なし
- `via_base`: `B` で 1 回補給
- `via_resource`: 選んだ 1 つの `R` で 1 回補給

`FUEL ANALYSIS` セクションには次が含まれます。

- `fuel_feasible_direct`
- `fuel_feasible_via_base`
- `fuel_feasible_via_resource`
- `remaining_fuel_direct`
- `remaining_fuel_via_base`
- `remaining_fuel_via_resource`
- `remaining_fuel_at_goal`
- `required_supply`
- `best_cost_via_resource`
- `best_resource_position`

bool は `yes` / `no`、値なしは `N/A` で表示します。

## この issue で扱わないもの

次はこの燃料分析の対象外です。

- `B` と `R` の両方で補給する経路
- 複数の `R` で補給する経路
- 同じ地点で複数回補給する経路
- 補給地点の巡回順最適化
- 正式な燃料容量
- プローブ、敵、故障などの追加要素

続いて `COST CONTRIBUTIONS` セクションで、同じ `GalacticMap` に対する全情報あり基準分析を出力します。ここでは地形配置と `S/H/B/R` 配置を固定したまま、経路計算条件だけを切り替えます。

- A: `plain`
  - 断層なし
  - 移動先記号に関係なく、1 移動ごとにコスト 1
- B: `terrain_only`
  - 断層なし
  - 現行の地形コスト表を使用
- C: `full`
  - 生成済み断層あり
  - 現行の地形コスト表を使用

開始地点 `S` のコストは、A/B/C のどれでも加算しません。

`COST CONTRIBUTIONS` セクションには次の 5 指標が含まれます。

- `plain_cost`
  - A の `S -> H` 最小コスト
- `terrain_only_cost`
  - B の `S -> H` 最小コスト
- `full_cost`
  - C の `S -> H` 最小コスト
- `terrain_extra_cost`
  - `terrain_only_cost - plain_cost`
  - 地形コスト差だけで基準経路にどれだけ追加消費が生じるか
- `rift_detour_cost`
  - `full_cost - terrain_only_cost`
  - 現行地形上で断層を加えたことでどれだけ追加消費が生じるか

`full` 条件だけは断層により到達不能になり得ます。その場合は `full_cost` と `rift_detour_cost` を内部では `None` とし、レポートでは `N/A` と表示します。`plain_cost`、`terrain_only_cost`、`terrain_extra_cost` は、その場合でも数値のまま保持されます。

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
